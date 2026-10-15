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
    Schema, Slot, Terminal, Value,
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
    MissingRow {
        table: TableName,
        key: Value,
    },
    DuplicateKey {
        table: TableName,
        key: Value,
    },
    /// A write's value fell outside `column`'s declared width, and `column`
    /// declares no clamping domain to saturate into instead (D3).
    Overflow {
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

/// Runs `call` against `program` starting from `store`. Does not mutate
/// `store`.
///
/// Panics if `call` names a function `program` does not declare, or if that
/// function's body refers to a parameter it did not declare or a
/// [`Slot`] no read in scope has bound — `Program` and `Call` are the
/// compiler's trusted output, not raw input this boundary validates.
pub fn run(program: &Program, call: &Call, store: &Store) -> Execution {
    let function = program.function(&call.function);
    let mut working_store = store.clone();
    let mut reads: HashMap<Slot, Option<Row>> = HashMap::new();
    let mut access_log = Vec::new();

    let outcome = execute_block(
        &function.body,
        &program.schema,
        &function.parameters,
        &call.args,
        &mut working_store,
        &mut reads,
        &mut access_log,
    );

    let final_store = match &outcome {
        Outcome::Committed { .. } => working_store,
        Outcome::Aborted { .. } => store.clone(),
    };

    Execution {
        store: final_store,
        outcome,
        access_log,
    }
}

/// Applies a block's effects in program order, then its terminal. Returns
/// as soon as an effect aborts, before any later effect runs.
fn execute_block(
    block: &Block,
    schema: &Schema,
    parameters: &[ParameterName],
    args: &[Value],
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    for effect in &block.effects {
        if let Some(outcome) =
            apply_effect(effect, schema, parameters, args, store, reads, access_log)
        {
            return outcome;
        }
    }
    execute_terminal(
        &block.terminal,
        schema,
        parameters,
        args,
        store,
        reads,
        access_log,
    )
}

/// Applies one effect. Returns `Some(outcome)` if the effect itself aborted
/// the call, `None` if execution should continue with the next effect.
fn apply_effect(
    effect: &Effect,
    schema: &Schema,
    parameters: &[ParameterName],
    args: &[Value],
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    match effect {
        Effect::Read { table, key, into } => {
            let key = resolve(key, parameters, args, reads);
            let row = store.row(table, &key).cloned();
            access_log.push(AccessLogEntry {
                table: table.clone(),
                kind: AccessKind::PointRead,
                key,
                columns: AccessedColumns::EntireRow,
            });
            reads.insert(*into, row);
            None
        }
        Effect::Insert {
            table,
            key,
            columns,
        } => {
            let key = resolve(key, parameters, args, reads);
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
            let mut resolved_columns = BTreeMap::new();
            for (column, expr) in columns {
                let declared = schema.column(table, column);
                match resolve_for_write(expr, parameters, args, reads, declared) {
                    Ok(value) => {
                        resolved_columns.insert(column.clone(), value);
                    }
                    Err(Overflow) => {
                        return Some(Outcome::Aborted {
                            reason: AbortReason::Overflow {
                                table: table.clone(),
                                column: column.clone(),
                                key,
                            },
                        });
                    }
                }
            }
            let row = Row {
                columns: resolved_columns,
            };
            store.put_row(table.clone(), key, row);
            None
        }
        Effect::Update {
            table,
            key,
            columns,
        } => {
            let key = resolve(key, parameters, args, reads);
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
            for (column, expr) in columns {
                let declared = schema.column(table, column);
                match resolve_for_write(expr, parameters, args, reads, declared) {
                    Ok(value) => {
                        row.columns.insert(column.clone(), value);
                    }
                    Err(Overflow) => {
                        return Some(Outcome::Aborted {
                            reason: AbortReason::Overflow {
                                table: table.clone(),
                                column: column.clone(),
                                key,
                            },
                        });
                    }
                }
            }
            store.put_row(table.clone(), key, row);
            None
        }
        Effect::Delete { table, key } => {
            let key = resolve(key, parameters, args, reads);
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
    }
}

/// Decides a block's outcome once its effects are done: return a response,
/// abort by name, or take one of two branches on whether a prior read found
/// a row.
fn execute_terminal(
    terminal: &Terminal,
    schema: &Schema,
    parameters: &[ParameterName],
    args: &[Value],
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    match terminal {
        Terminal::Return(expr) => Outcome::Committed {
            response: resolve(expr, parameters, args, reads),
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
            execute_block(block, schema, parameters, args, store, reads, access_log)
        }
    }
}

/// Resolves an expression to a concrete value: a constant, the argument
/// bound to a declared parameter, a column projected from an earlier read's
/// row, or an arithmetic operation over two such values.
///
/// Arithmetic here runs over plain `i64` and panics on overflow: a write's
/// own overflow-or-clamp decision against its target column's declared
/// width (D3) is [`resolve_for_write`]'s job, not this general resolver's —
/// an expression that overflows before ever reaching a column write is a
/// contract violation by whatever built it, the same class of trusted-input
/// assumption as an unbound parameter or slot elsewhere in this module.
fn resolve(
    expr: &Expr,
    parameters: &[ParameterName],
    args: &[Value],
    reads: &HashMap<Slot, Option<Row>>,
) -> Value {
    match expr {
        Expr::Literal(value) => value.clone(),
        Expr::Parameter(name) => {
            let index = parameters
                .iter()
                .position(|parameter| parameter == name)
                .unwrap_or_else(|| panic!("function has no declared parameter named {name:?}"));
            args[index].clone()
        }
        Expr::Projection { slot, column } => reads
            .get(slot)
            .unwrap_or_else(|| panic!("projection reads slot {slot:?} before any read bound it"))
            .as_ref()
            .unwrap_or_else(|| {
                panic!("projection reads column {column:?} from slot {slot:?}, whose read found no row")
            })
            .columns
            .get(column)
            .unwrap_or_else(|| panic!("row bound to slot {slot:?} has no column named {column:?}"))
            .clone(),
        Expr::Arithmetic(op, lhs, rhs) => {
            let lhs = as_integer(resolve(lhs, parameters, args, reads));
            let rhs = as_integer(resolve(rhs, parameters, args, reads));
            let result = checked_apply(*op, lhs, rhs).unwrap_or_else(|| {
                panic!(
                    "arithmetic {op:?}({lhs}, {rhs}) overflowed i64 outside any column write — \
                     only a write's target column may absorb overflow (D3)"
                )
            });
            Value::Integer(result)
        }
    }
}

/// A write's value fell outside its target column's declared width, with no
/// clamping domain to saturate into instead (D3).
struct Overflow;

/// Resolves `expr` for a write into a column declared as `declared_column`,
/// applying D3's overflow-or-clamp decision when the schema declares that
/// column.
///
/// An undeclared column (`declared_column` is `None`) carries no width or
/// domain, so its write resolves exactly as [`resolve`] would on its own —
/// the behavior this evaluator has always had outside this slice's reach.
fn resolve_for_write(
    expr: &Expr,
    parameters: &[ParameterName],
    args: &[Value],
    reads: &HashMap<Slot, Option<Row>>,
    declared_column: Option<&Column>,
) -> Result<Value, Overflow> {
    let Some(column) = declared_column else {
        return Ok(resolve(expr, parameters, args, reads));
    };

    let raw = match expr {
        Expr::Arithmetic(op, lhs, rhs) => {
            let lhs = as_integer(resolve(lhs, parameters, args, reads));
            let rhs = as_integer(resolve(rhs, parameters, args, reads));
            match column.domain {
                Some(domain) => clamp_to_domain(saturating_apply(*op, lhs, rhs), domain),
                None => {
                    let sum = checked_apply(*op, lhs, rhs).ok_or(Overflow)?;
                    check_width(sum, column.width)?
                }
            }
        }
        _ => {
            let value = as_integer(resolve(expr, parameters, args, reads));
            match column.domain {
                Some(domain) => clamp_to_domain(value, domain),
                None => check_width(value, column.width)?,
            }
        }
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

/// `lhs op rhs`, saturating at `i64`'s own width — used only when a
/// clamping domain will immediately narrow the result further, so an
/// `i64`-level overflow still saturates toward the domain bound its
/// direction implies.
fn saturating_apply(op: ArithmeticOp, lhs: i64, rhs: i64) -> i64 {
    match op {
        ArithmeticOp::Add => lhs.saturating_add(rhs),
        ArithmeticOp::Subtract => lhs.saturating_sub(rhs),
    }
}

/// `value` if it lies within `width`'s representable range, `Err(Overflow)`
/// otherwise (D3).
fn check_width(value: i64, width: IntegerWidth) -> Result<i64, Overflow> {
    let (min, max) = width.range();
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(Overflow)
    }
}

/// Saturates `value` into `domain`'s inclusive bound (D3): a value already
/// inside the domain is left unchanged.
fn clamp_to_domain(value: i64, domain: ClampDomain) -> i64 {
    value.clamp(domain.min, domain.max)
}
