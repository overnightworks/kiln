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
    ArithmeticOp, Block, ClampDomain, Column, Condition, Effect, Expr, Function, IntegerWidth,
    Parameter, ParameterType, Program, Row, Schema, SchemaError, Slot, Terminal, Value,
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
/// `MissingRow`, `DuplicateKey`, `Overflow` and `BoundExceeded` are the
/// evaluator's own structural rules for update/delete, insert, checked
/// arithmetic and bounded range iteration (`docs/concept.md` §2.2, D3, D6);
/// `Declared` is a transaction body's own explicit, named rejection. The
/// remaining variants are every other way a compiled program's own
/// expressions can fail to resolve against the data a call actually ran
/// against — never a panic, since whether an argument, a range bound, or a
/// projection resolves is a fact about that data, not something `run` can
/// rule out ahead of time.
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
    Overflow(OverflowReason),
    /// A range read on `table` matched `matched` rows, more than its
    /// declared `bound` — bounded iteration never runs open-ended (§2.1,
    /// §2.2 D6).
    BoundExceeded {
        table: TableName,
        bound: i64,
        matched: usize,
    },
    /// A range read on `table` resolved its `bound` expression to a value
    /// that was not an integer — a bound counts rows, so only an integer
    /// can satisfy it (§2.1, "never an open-ended loop").
    NonIntegerBound {
        table: TableName,
    },
    /// The argument bound to `parameter` is not the kind its declared
    /// [`ParameterType`] requires (an integer where text was declared, or
    /// text where an integer was declared).
    ArgumentTypeMismatch {
        parameter: ParameterName,
    },
    /// The call supplied no argument at the position `parameter` declares.
    MissingArgument {
        parameter: ParameterName,
    },
    /// A projection has no single current row to read from: the slot it
    /// names was bound by a point read that found no row, or by a range
    /// read, whose slot never holds one current row on its own (a range's
    /// current row is only available inside an [`Effect::Iterate`] body).
    ProjectionSourceAbsent {
        slot: Slot,
    },
    /// A projection named `column`, but the row bound to `slot` carries no
    /// column by that name.
    MissingColumn {
        slot: Slot,
        column: ColumnName,
    },
}

/// Why checked arithmetic overflowed, or a value fell outside its
/// declared width (D3): `i64` itself overflowed before any width-or-clamp
/// decision ever saw the result, a write's final value fell outside its
/// target column's declared width with no clamping domain to saturate
/// into instead, or an argument fell outside its parameter's declared
/// width — a column clamps, but a parameter, declaring no domain, never
/// does.
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
    /// The argument bound to `parameter` fell outside its declared width.
    Argument { parameter: ParameterName },
}

/// Which columns an access touched: every column of the row, or exactly the
/// named ones a write gave values for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessedColumns {
    EntireRow,
    Named(Vec<ColumnName>),
}

/// What kind of access an [`AccessLogEntry`] records (D5 names a point
/// read and a range read as one family of access kinds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessKind {
    PointRead,
    RangeRead,
    Insert,
    Update,
    Delete,
}

/// Which rows an [`AccessLogEntry`] names: one row by key, or every row
/// whose key falls in `[start, end]` — recorded even when the range
/// matches no row, since the range itself was still attempted
/// (`docs/concept.md` §2.2, D5: "range bounds (including empty ranges)").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessTarget {
    Point(Value),
    Range { start: Value, end: Value },
}

/// One attempted access, recorded whether or not the call that made it
/// went on to commit (D5: "attempted accesses are kept on abort").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLogEntry {
    pub table: TableName,
    pub kind: AccessKind,
    pub target: AccessTarget,
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
    parameters: &'a [Parameter],
    args: &'a [Value],
}

/// What a [`Slot`] holds once the effect that bound it has run: a single
/// row or its absence (`Effect::Read`), or the ordered sequence of rows a
/// range matched (`Effect::ReadRange`). [`Condition::RowPresent`] reads
/// either kind as one branchable fact — "did this read find anything" —
/// without the rest of `run` needing to know which effect produced it.
enum Binding {
    Row(Option<Row>),
    Range(Vec<Row>),
}

impl Binding {
    fn is_present(&self) -> bool {
        match self {
            Binding::Row(row) => row.is_some(),
            Binding::Range(rows) => !rows.is_empty(),
        }
    }
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

    if let Some(reason) = check_argument_types(function, &call.args) {
        return Ok(Execution {
            store: store.clone(),
            outcome: Outcome::Aborted { reason },
            access_log: Vec::new(),
        });
    }

    let mut working_store = store.clone();
    let mut bindings: HashMap<Slot, Binding> = HashMap::new();
    let mut access_log = Vec::new();

    let outcome = execute_block(
        &function.body,
        &ctx,
        &mut working_store,
        &mut bindings,
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

/// Checks every argument in `args` against the declared type of the
/// parameter at the same position, exactly as a write checks its value
/// against its target column's declared width (D3, `docs/concept.md` §2.2:
/// "the column's or argument's declared width"). Returns the typed abort
/// reason for the first argument that does not match, or `None` once every
/// argument has been checked.
///
/// A parameter declares no clamping domain, so an out-of-width or
/// wrong-kind argument always aborts — unlike a column, which may clamp an
/// out-of-width write instead.
///
/// An `args` shorter than `function.parameters` is not checked here: `zip`
/// simply stops at the shorter side, and a later position's own
/// [`Expr::Parameter`] reference reports the typed
/// [`AbortReason::MissingArgument`] once the body actually resolves it.
fn check_argument_types(function: &Function, args: &[Value]) -> Option<AbortReason> {
    for (parameter, arg) in function.parameters.iter().zip(args) {
        match (parameter.kind, arg) {
            (ParameterType::Integer(width), Value::Integer(value)) => {
                if check_width(*value, width).is_err() {
                    return Some(AbortReason::Overflow(OverflowReason::Argument {
                        parameter: parameter.name.clone(),
                    }));
                }
            }
            (ParameterType::Text, Value::Text(_)) => {}
            (ParameterType::Integer(_), Value::Text(_))
            | (ParameterType::Text, Value::Integer(_)) => {
                return Some(AbortReason::ArgumentTypeMismatch {
                    parameter: parameter.name.clone(),
                });
            }
        }
    }
    None
}

/// Applies a block's effects in program order, then its terminal. Returns
/// as soon as an effect aborts, before any later effect runs.
fn execute_block(
    block: &Block,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    for effect in &block.effects {
        if let Some(outcome) = apply_effect(effect, ctx, store, bindings, access_log) {
            return outcome;
        }
    }
    execute_terminal(&block.terminal, ctx, store, bindings, access_log)
}

/// Applies one effect by dispatching to the function that owns its kind.
/// Returns `Some(outcome)` if the effect itself aborted the call, `None` if
/// execution should continue with the next effect.
fn apply_effect(
    effect: &Effect,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    match effect {
        Effect::Read { table, key, into } => {
            apply_read(table, key, *into, ctx, store, bindings, access_log)
        }
        Effect::Insert {
            table,
            key,
            columns,
        } => apply_insert(table, key, columns, ctx, store, bindings, access_log),
        Effect::Update {
            table,
            key,
            columns,
        } => apply_update(table, key, columns, ctx, store, bindings, access_log),
        Effect::Delete { table, key } => apply_delete(table, key, ctx, store, bindings, access_log),
        Effect::ReadRange {
            table,
            start,
            end,
            bound,
            into,
        } => apply_read_range(
            RangeRead {
                table,
                start,
                end,
                bound,
                into: *into,
            },
            ctx,
            store,
            bindings,
            access_log,
        ),
        Effect::Iterate { source, row, body } => {
            apply_iterate(*source, *row, body, ctx, store, bindings, access_log)
        }
    }
}

/// Resolves an effect's expression, turning any failure to resolve it into
/// the aborted [`Outcome`] every `apply_*` function below returns on error —
/// shared so every effect kind reports a failed key, a range's start, end,
/// or bound, or any other expression the same way.
fn resolve_or_abort(
    expr: &Expr,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Value, Outcome> {
    resolve(expr, ctx, bindings).map_err(|error| Outcome::Aborted {
        reason: error.into(),
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
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_or_abort(key, ctx, bindings) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    let row = store.row(table, &key).cloned();
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(key),
        columns: AccessedColumns::EntireRow,
    });
    bindings.insert(into, Binding::Row(row));
    None
}

/// An [`Effect::ReadRange`]'s own fields, bundled so `apply_read_range`
/// takes one value instead of five — the same reason [`CallContext`]
/// bundles the call's constant context, applied to one effect's fields
/// instead.
struct RangeRead<'a> {
    table: &'a TableName,
    start: &'a Expr,
    end: &'a Expr,
    bound: &'a Expr,
    into: Slot,
}

/// Reads every row of `range.table` whose key falls in `[range.start,
/// range.end]`, binding the matched sequence, in key order, to
/// `range.into` (`Effect::ReadRange`).
///
/// Aborts with [`AbortReason::BoundExceeded`] if more rows match than
/// `range.bound` resolves to, without binding `range.into` at all —
/// bounded iteration never runs open-ended (`docs/concept.md` §2.1, §2.2
/// D6).
fn apply_read_range(
    range: RangeRead,
    ctx: &CallContext,
    store: &Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let start = match resolve_or_abort(range.start, ctx, bindings) {
        Ok(start) => start,
        Err(outcome) => return Some(outcome),
    };
    let end = match resolve_or_abort(range.end, ctx, bindings) {
        Ok(end) => end,
        Err(outcome) => return Some(outcome),
    };
    let bound = match resolve_or_abort(range.bound, ctx, bindings) {
        Ok(Value::Integer(bound)) => bound,
        Ok(Value::Text(_)) => {
            return Some(Outcome::Aborted {
                reason: AbortReason::NonIntegerBound {
                    table: range.table.clone(),
                },
            });
        }
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: range.table.clone(),
        kind: AccessKind::RangeRead,
        target: AccessTarget::Range {
            start: start.clone(),
            end: end.clone(),
        },
        columns: AccessedColumns::EntireRow,
    });
    let rows = store.rows_in_range(range.table, &start, &end);
    if rows.len() as i64 > bound {
        return Some(Outcome::Aborted {
            reason: AbortReason::BoundExceeded {
                table: range.table.clone(),
                bound,
                matched: rows.len(),
            },
        });
    }
    bindings.insert(range.into, Binding::Range(rows));
    None
}

/// Runs `body` once for each row `source` bound, in key order, binding the
/// current row to `row` so the body's own effects and expressions can use
/// it (`Effect::Iterate`, `docs/concept.md` §2.1 "bounded iteration", §2.2
/// D6: "bounded iteration proceeds in key order").
///
/// `source` must already be bound by an [`Effect::ReadRange`] in the same
/// body: that read already resolved and bound-checked its range, so
/// `Iterate` re-reads nothing and checks no bound of its own. Panics if
/// `source` is unbound or was bound by a point read instead — a contract
/// violation by whatever built the effect, not a semantic outcome `run`
/// reports.
fn apply_iterate(
    source: Slot,
    row: Slot,
    body: &[Effect],
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let rows = match bindings.get(&source) {
        Some(Binding::Range(rows)) => rows.clone(),
        Some(Binding::Row(_)) => {
            panic!("iteration reads slot {source:?} as its range, but a point read bound it")
        }
        None => panic!("iteration reads slot {source:?} before any range read bound it"),
    };
    for current in rows {
        bindings.insert(row, Binding::Row(Some(current)));
        for effect in body {
            if let Some(outcome) = apply_effect(effect, ctx, store, bindings, access_log) {
                return Some(outcome);
            }
        }
    }
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
    bindings: &HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_or_abort(key, ctx, bindings) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::Insert,
        target: AccessTarget::Point(key.clone()),
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
    let resolved_columns = match resolve_write_columns(table, &key, columns, ctx, bindings) {
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
    bindings: &HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_or_abort(key, ctx, bindings) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::Update,
        target: AccessTarget::Point(key.clone()),
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
    let resolved_columns = match resolve_write_columns(table, &key, columns, ctx, bindings) {
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
    bindings: &HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_or_abort(key, ctx, bindings) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::Delete,
        target: AccessTarget::Point(key.clone()),
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
    bindings: &HashMap<Slot, Binding>,
) -> Result<BTreeMap<ColumnName, Value>, Outcome> {
    let mut resolved = BTreeMap::new();
    for (column, expr) in columns {
        let declared = ctx.schema.column(table, column);
        let value =
            resolve_for_write(expr, ctx, bindings, declared).map_err(|error| match error {
                WriteOverflow::Resolve(error) => Outcome::Aborted {
                    reason: error.into(),
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
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    match terminal {
        Terminal::Return(expr) => match resolve(expr, ctx, bindings) {
            Ok(response) => Outcome::Committed { response },
            Err(error) => Outcome::Aborted {
                reason: error.into(),
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
                Condition::RowPresent(slot) => bindings
                    .get(slot)
                    .unwrap_or_else(|| {
                        panic!("branch condition reads slot {slot:?} before any read bound it")
                    })
                    .is_present(),
            };
            let block = if present { when_present } else { when_absent };
            execute_block(block, ctx, store, bindings, access_log)
        }
    }
}

/// Why [`resolve`] could not produce a value for an expression: `i64`
/// arithmetic overflowed before any column or argument was reached (D3), or
/// a parameter or projection expression could not find the value it names
/// in the data this call actually ran against. Every variant maps to a
/// typed [`AbortReason`] through [`resolve_or_abort`] or
/// [`resolve_for_write`]'s own caller — never a panic, since each is a fact
/// about that data, not a malformed expression.
enum ResolveError {
    Overflow(OverflowReason),
    MissingArgument { parameter: ParameterName },
    ProjectionSourceAbsent { slot: Slot },
    MissingColumn { slot: Slot, column: ColumnName },
}

impl From<ResolveError> for AbortReason {
    fn from(error: ResolveError) -> Self {
        match error {
            ResolveError::Overflow(reason) => AbortReason::Overflow(reason),
            ResolveError::MissingArgument { parameter } => {
                AbortReason::MissingArgument { parameter }
            }
            ResolveError::ProjectionSourceAbsent { slot } => {
                AbortReason::ProjectionSourceAbsent { slot }
            }
            ResolveError::MissingColumn { slot, column } => {
                AbortReason::MissingColumn { slot, column }
            }
        }
    }
}

/// Resolves an expression to a concrete value: a constant, the argument
/// bound to a declared parameter, a column projected from an earlier read's
/// row, or an arithmetic operation over two such values.
///
/// Arithmetic here is checked at `i64`'s own width and returns
/// [`ResolveError::Overflow`] if it overflows — true outside any column
/// write (a response, a key expression) and inside a nested sub-expression
/// alike, so every intermediate step of a write's own expression is checked
/// the same way before [`resolve_for_write`] ever judges the expression's
/// one final value against its target column (D3).
///
/// Panics if `expr` refers to a parameter `ctx.parameters` does not
/// declare, or a [`Slot`] no read in scope has bound — a contract violation
/// by whatever built the expression, not a semantic outcome this evaluator
/// reports.
fn resolve(
    expr: &Expr,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Value, ResolveError> {
    match expr {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::Parameter(name) => {
            let index = ctx
                .parameters
                .iter()
                .position(|parameter| &parameter.name == name)
                .unwrap_or_else(|| panic!("function has no declared parameter named {name:?}"));
            ctx.args
                .get(index)
                .cloned()
                .ok_or_else(|| ResolveError::MissingArgument {
                    parameter: name.clone(),
                })
        }
        Expr::Projection { slot, column } => {
            let binding = bindings.get(slot).unwrap_or_else(|| {
                panic!("projection reads slot {slot:?} before any read bound it")
            });
            let row = match binding {
                Binding::Row(Some(row)) => row,
                Binding::Row(None) | Binding::Range(_) => {
                    return Err(ResolveError::ProjectionSourceAbsent { slot: *slot });
                }
            };
            row.columns
                .get(column)
                .cloned()
                .ok_or_else(|| ResolveError::MissingColumn {
                    slot: *slot,
                    column: column.clone(),
                })
        }
        Expr::Arithmetic(op, lhs, rhs) => {
            let lhs = as_integer(resolve(lhs, ctx, bindings)?);
            let rhs = as_integer(resolve(rhs, ctx, bindings)?);
            let result = checked_apply(*op, lhs, rhs)
                .ok_or(ResolveError::Overflow(OverflowReason::Arithmetic))?;
            Ok(Value::Integer(result))
        }
    }
}

/// Why [`resolve_for_write`] could not produce a value for its target
/// column: [`resolve`] itself failed before the column was ever reached, or
/// the expression's final value fell outside the column's declared width
/// with no clamping domain to saturate into instead (D3).
///
/// [`apply_effect`] is the only caller, and it already knows which table,
/// column and key the write targeted — this type carries just enough to
/// tell its D3 width outcome apart from every other resolution failure,
/// leaving that context to the caller that already has it.
enum WriteOverflow {
    Resolve(ResolveError),
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
    bindings: &HashMap<Slot, Binding>,
    declared_column: Option<&Column>,
) -> Result<Value, WriteOverflow> {
    let value = resolve(expr, ctx, bindings).map_err(WriteOverflow::Resolve)?;
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
