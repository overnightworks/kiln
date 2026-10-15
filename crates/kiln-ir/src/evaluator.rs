//! The core evaluator: runs one [`Call`] against a [`Program`] and a
//! [`Store`], producing the store that results, the call's [`Outcome`], and
//! the [`AccessLogEntry`] list the reference meaning requires (D5).
//!
//! `run` is pure: it never mutates the `Store` it is given. It evaluates
//! against a private working copy and, on commit, returns that copy;
//! on abort, it returns a fresh clone of the original instead, discarding
//! every effect the aborted call produced (`docs/concept.md` §2.2).

use std::collections::{BTreeMap, HashMap};

use crate::call::Call;
use crate::identifiers::{AbortName, ColumnName, ParameterName, TableName};
use crate::ir::{
    ArithmeticOp, Block, ClampDomain, Column, Condition, Effect, Expr, IntegerWidth, Program, Row,
    Schema, SchemaError, Slot, Terminal, Value,
};
use crate::store::Store;

/// What a committed or aborted call produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Committed { response: Value },
    Aborted { reason: AbortReason },
}

/// Why a call aborted.
///
/// `MissingRow`, `DuplicateKey` and `Overflow` are the evaluator's own
/// structural rules for update/delete, insert and checked arithmetic
/// (`docs/concept.md` §2.2, D3); `Declared` is a transaction body's own
/// explicit, named rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbortReason {
    Declared(AbortName),
    MissingRow { table: TableName, key: Value },
    DuplicateKey { table: TableName, key: Value },
    Overflow(OverflowReason),
}

/// Why checked arithmetic overflowed (D3): either `i64` itself overflowed
/// before any column write ever saw the result, or a write's final value
/// fell outside its target column's declared width with no clamping domain
/// to saturate into instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowReason {
    /// Arithmetic overflowed `i64` itself — in a response, a key expression,
    /// or a nested sub-expression — before reaching any column write's
    /// width-or-clamp decision.
    Arithmetic,
    /// A write's final value fell outside `column`'s declared width, and
    /// `column` declares no clamping domain to saturate into instead.
    Width {
        table: TableName,
        column: ColumnName,
        key: Value,
    },
}

/// Which columns an access touched: every column of the row, or exactly the
/// named ones a write gave values for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessedColumns {
    EntireRow,
    Named(Vec<ColumnName>),
}

/// What kind of access an [`AccessLogEntry`] records.
///
/// Named `PointRead` rather than `Read` so a later slice can add
/// `RangeRead` alongside it without renaming this one (D5 names both as
/// one family of access kinds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessKind {
    PointRead,
    Insert,
    Update,
    Delete,
}

/// One attempted access, recorded whether or not the call that made it
/// went on to commit (D5: "attempted accesses are kept on abort").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLogEntry {
    pub table: TableName,
    pub kind: AccessKind,
    pub key: Value,
    pub columns: AccessedColumns,
}

/// The result of running one [`Call`]: the store it produced, its outcome,
/// and the accesses it attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub store: Store,
    pub outcome: Outcome,
    pub access_log: Vec<AccessLogEntry>,
}

/// The read-only context every step of one [`run`] resolves expressions
/// against: the schema a write checks its target column against, and the
/// call's own declared parameters and argument values (D1, D4). Bundled so
/// a function below takes one value instead of carrying all three
/// separately through every call in the recursion.
struct CallContext<'a> {
    schema: &'a Schema,
    parameters: &'a [ParameterName],
    args: &'a [Value],
}

/// Runs `call` against `program` starting from `store`. Does not mutate
/// `store`.
///
/// Rejects `program`'s schema with a typed [`SchemaError`] before executing
/// anything if that schema is not internally satisfiable (D3,
/// [`Schema::validate`]).
///
/// Panics if `call` names a function `program` does not declare, or if that
/// function's body refers to a parameter it did not declare or a
/// [`Slot`] no read in scope has bound — `Program` and `Call` are the
/// compiler's trusted output, not raw input this boundary validates.
pub fn run(program: &Program, call: &Call, store: &Store) -> Result<Execution, SchemaError> {
    program.schema.validate()?;

    let function = program.function(&call.function);
    let ctx = CallContext {
        schema: &program.schema,
        parameters: &function.parameters,
        args: &call.args,
    };
    let mut working_store = store.clone();
    let mut reads: HashMap<Slot, Option<Row>> = HashMap::new();
    let mut access_log = Vec::new();

    let outcome = execute_block(
        &function.body,
        &ctx,
        &mut working_store,
        &mut reads,
        &mut access_log,
    );

    let final_store = match &outcome {
        Outcome::Committed { .. } => working_store,
        Outcome::Aborted { .. } => store.clone(),
    };

    Ok(Execution {
        store: final_store,
        outcome,
        access_log,
    })
}

/// Applies a block's effects in program order, then its terminal. Returns
/// as soon as an effect aborts, before any later effect runs.
fn execute_block(
    block: &Block,
    ctx: &CallContext,
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    for effect in &block.effects {
        if let Some(outcome) = apply_effect(effect, ctx, store, reads, access_log) {
            return outcome;
        }
    }
    execute_terminal(&block.terminal, ctx, store, reads, access_log)
}

/// Applies one effect by dispatching to the function that owns its kind.
/// Returns `Some(outcome)` if the effect itself aborted the call, `None` if
/// execution should continue with the next effect.
fn apply_effect(
    effect: &Effect,
    ctx: &CallContext,
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    match effect {
        Effect::Read { table, key, into } => {
            apply_read(table, key, *into, ctx, store, reads, access_log)
        }
        Effect::Insert {
            table,
            key,
            columns,
        } => apply_insert(table, key, columns, ctx, store, reads, access_log),
        Effect::Update {
            table,
            key,
            columns,
        } => apply_update(table, key, columns, ctx, store, reads, access_log),
        Effect::Delete { table, key } => apply_delete(table, key, ctx, store, reads, access_log),
    }
}

/// Resolves an effect's key expression, turning an `i64`-overflow failure
/// into the aborted [`Outcome`] every `apply_*` function below returns on
/// error — shared so every effect kind reports a key's own arithmetic
/// overflow the same way (D3).
fn resolve_key(
    key: &Expr,
    ctx: &CallContext,
    reads: &HashMap<Slot, Option<Row>>,
) -> Result<Value, Outcome> {
    resolve(key, ctx, reads).map_err(|reason| Outcome::Aborted {
        reason: AbortReason::Overflow(reason),
    })
}

/// Reads one row by key, binding its presence or absence to `into`
/// (`Effect::Read`).
fn apply_read(
    table: &TableName,
    key: &Expr,
    into: Slot,
    ctx: &CallContext,
    store: &Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(key, ctx, reads) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    let row = store.row(table, &key).cloned();
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::PointRead,
        key,
        columns: AccessedColumns::EntireRow,
    });
    reads.insert(into, row);
    None
}

/// Inserts a new row, aborting with `DuplicateKey` if one already exists at
/// `key` (`Effect::Insert`).
fn apply_insert(
    table: &TableName,
    key: &Expr,
    columns: &BTreeMap<ColumnName, Expr>,
    ctx: &CallContext,
    store: &mut Store,
    reads: &HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(key, ctx, reads) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::Insert,
        key: key.clone(),
        columns: AccessedColumns::Named(columns.keys().cloned().collect()),
    });
    if store.row(table, &key).is_some() {
        return Some(Outcome::Aborted {
            reason: AbortReason::DuplicateKey {
                table: table.clone(),
                key,
            },
        });
    }
    let resolved_columns = match resolve_write_columns(table, &key, columns, ctx, reads) {
        Ok(resolved) => resolved,
        Err(outcome) => return Some(outcome),
    };
    store.put_row(
        table.clone(),
        key,
        Row {
            columns: resolved_columns,
        },
    );
    None
}

/// Overwrites the named columns of an existing row, aborting with
/// `MissingRow` if none exists at `key` (`Effect::Update`).
fn apply_update(
    table: &TableName,
    key: &Expr,
    columns: &BTreeMap<ColumnName, Expr>,
    ctx: &CallContext,
    store: &mut Store,
    reads: &HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(key, ctx, reads) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::Update,
        key: key.clone(),
        columns: AccessedColumns::Named(columns.keys().cloned().collect()),
    });
    let Some(mut row) = store.row(table, &key).cloned() else {
        return Some(Outcome::Aborted {
            reason: AbortReason::MissingRow {
                table: table.clone(),
                key,
            },
        });
    };
    let resolved_columns = match resolve_write_columns(table, &key, columns, ctx, reads) {
        Ok(resolved) => resolved,
        Err(outcome) => return Some(outcome),
    };
    row.columns.extend(resolved_columns);
    store.put_row(table.clone(), key, row);
    None
}

/// Removes a row, aborting with `MissingRow` if none exists at `key`
/// (`Effect::Delete`).
fn apply_delete(
    table: &TableName,
    key: &Expr,
    ctx: &CallContext,
    store: &mut Store,
    reads: &HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(key, ctx, reads) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::Delete,
        key: key.clone(),
        columns: AccessedColumns::EntireRow,
    });
    if store.row(table, &key).is_none() {
        return Some(Outcome::Aborted {
            reason: AbortReason::MissingRow {
                table: table.clone(),
                key,
            },
        });
    }
    store.remove_row(table, &key);
    None
}

/// Resolves every `columns` entry for an insert or update into a write's
/// target table, applying each one's D3 width-or-clamp decision against its
/// schema declaration (`resolve_for_write`) — the resolution loop the two
/// effects shared verbatim before this slice.
///
/// Returns the resolved column map once every column resolved, or the
/// aborted [`Outcome`] for the first column whose arithmetic or width check
/// overflowed.
fn resolve_write_columns(
    table: &TableName,
    key: &Value,
    columns: &BTreeMap<ColumnName, Expr>,
    ctx: &CallContext,
    reads: &HashMap<Slot, Option<Row>>,
) -> Result<BTreeMap<ColumnName, Value>, Outcome> {
    let mut resolved = BTreeMap::new();
    for (column, expr) in columns {
        let declared = ctx.schema.column(table, column);
        let value = resolve_for_write(expr, ctx, reads, declared).map_err(|error| match error {
            WriteOverflow::Arithmetic => Outcome::Aborted {
                reason: AbortReason::Overflow(OverflowReason::Arithmetic),
            },
            WriteOverflow::Width => Outcome::Aborted {
                reason: AbortReason::Overflow(OverflowReason::Width {
                    table: table.clone(),
                    column: column.clone(),
                    key: key.clone(),
                }),
            },
        })?;
        resolved.insert(column.clone(), value);
    }
    Ok(resolved)
}

/// Decides a block's outcome once its effects are done: return a response,
/// abort by name, or take one of two branches on whether a prior read found
/// a row.
fn execute_terminal(
    terminal: &Terminal,
    ctx: &CallContext,
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    match terminal {
        Terminal::Return(expr) => match resolve(expr, ctx, reads) {
            Ok(response) => Outcome::Committed { response },
            Err(reason) => Outcome::Aborted {
                reason: AbortReason::Overflow(reason),
            },
        },
        Terminal::Abort(name) => Outcome::Aborted {
            reason: AbortReason::Declared(name.clone()),
        },
        Terminal::Branch {
            condition,
            when_present,
            when_absent,
        } => {
            let present = match condition {
                Condition::RowPresent(slot) => reads
                    .get(slot)
                    .unwrap_or_else(|| {
                        panic!("branch condition reads slot {slot:?} before any read bound it")
                    })
                    .is_some(),
            };
            let block = if present { when_present } else { when_absent };
            execute_block(block, ctx, store, reads, access_log)
        }
    }
}

/// Resolves an expression to a concrete value: a constant, the argument
/// bound to a declared parameter, a column projected from an earlier read's
/// row, or an arithmetic operation over two such values.
///
/// Arithmetic here is checked at `i64`'s own width and returns
/// [`OverflowReason::Arithmetic`] if it overflows — true outside any column
/// write (a response, a key expression) and inside a nested sub-expression
/// alike, so every intermediate step of a write's own expression is checked
/// the same way before [`resolve_for_write`] ever judges the expression's
/// one final value against its target column (D3).
fn resolve(
    expr: &Expr,
    ctx: &CallContext,
    reads: &HashMap<Slot, Option<Row>>,
) -> Result<Value, OverflowReason> {
    match expr {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::Parameter(name) => {
            let index = ctx
                .parameters
                .iter()
                .position(|parameter| parameter == name)
                .unwrap_or_else(|| panic!("function has no declared parameter named {name:?}"));
            Ok(ctx.args[index].clone())
        }
        Expr::Projection { slot, column } => Ok(reads
            .get(slot)
            .unwrap_or_else(|| panic!("projection reads slot {slot:?} before any read bound it"))
            .as_ref()
            .unwrap_or_else(|| {
                panic!("projection reads column {column:?} from slot {slot:?}, whose read found no row")
            })
            .columns
            .get(column)
            .unwrap_or_else(|| panic!("row bound to slot {slot:?} has no column named {column:?}"))
            .clone()),
        Expr::Arithmetic(op, lhs, rhs) => {
            let lhs = as_integer(resolve(lhs, ctx, reads)?);
            let rhs = as_integer(resolve(rhs, ctx, reads)?);
            let result = checked_apply(*op, lhs, rhs).ok_or(OverflowReason::Arithmetic)?;
            Ok(Value::Integer(result))
        }
    }
}

/// Why [`resolve_for_write`] could not produce a value for its target
/// column: `i64` arithmetic overflowed before the column was ever reached,
/// or the expression's final value fell outside the column's declared
/// width with no clamping domain to saturate into instead (D3).
///
/// [`apply_effect`] is the only caller, and it already knows which table,
/// column and key the write targeted — this type carries just enough to
/// tell its two D3 outcomes apart, leaving that context to the caller that
/// already has it.
enum WriteOverflow {
    Arithmetic,
    Width,
}

/// Resolves `expr` for a write into a column declared as `declared_column`,
/// applying D3's width-or-clamp decision to the expression's one final
/// value. Nested arithmetic inside `expr` was already checked at `i64` by
/// `resolve`; this only ever judges the result `resolve` already produced,
/// never an intermediate step of a computed write.
///
/// An undeclared column (`declared_column` is `None`) carries no width or
/// domain, so its write resolves exactly as [`resolve`] would on its own —
/// the behavior this evaluator has always had outside this slice's reach.
fn resolve_for_write(
    expr: &Expr,
    ctx: &CallContext,
    reads: &HashMap<Slot, Option<Row>>,
    declared_column: Option<&Column>,
) -> Result<Value, WriteOverflow> {
    let value = resolve(expr, ctx, reads).map_err(|_| WriteOverflow::Arithmetic)?;
    let Some(column) = declared_column else {
        return Ok(value);
    };

    let raw = as_integer(value);
    let raw = match column.domain {
        Some(domain) => clamp_to_domain(raw, domain),
        None => check_width(raw, column.width).map_err(|_| WriteOverflow::Width)?,
    };

    Ok(Value::Integer(raw))
}

/// Extracts the integer an arithmetic operand must resolve to.
///
/// Panics on a text value: D3's arithmetic runs only over checked integers,
/// so a non-integer operand is a contract violation by whatever built the
/// expression, not a semantic outcome this evaluator reports.
fn as_integer(value: Value) -> i64 {
    match value {
        Value::Integer(value) => value,
        Value::Text(text) => {
            panic!("arithmetic operand resolved to the non-integer value {text:?} (D3)")
        }
    }
}

/// `lhs op rhs`, checked at `i64`'s own width: `None` on true `i64` overflow.
fn checked_apply(op: ArithmeticOp, lhs: i64, rhs: i64) -> Option<i64> {
    match op {
        ArithmeticOp::Add => lhs.checked_add(rhs),
        ArithmeticOp::Subtract => lhs.checked_sub(rhs),
    }
}

/// `value` if it lies within `width`'s representable range, `Err(())`
/// otherwise (D3).
fn check_width(value: i64, width: IntegerWidth) -> Result<i64, ()> {
    let (min, max) = width.range();
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(())
    }
}

/// Saturates `value` into `domain`'s inclusive bound (D3): a value already
/// inside the domain is left unchanged.
fn clamp_to_domain(value: i64, domain: ClampDomain) -> i64 {
    value.clamp(domain.min, domain.max)
}
