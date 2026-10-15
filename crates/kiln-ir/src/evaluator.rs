//! The core evaluator: runs one [`Call`] against a [`Program`] and a
//! [`Store`], producing the store that results, the call's [`Outcome`], and
//! the [`AccessLogEntry`] list the reference meaning requires (D5).
//!
//! `run` is pure: it never mutates the `Store` it is given. It evaluates
//! against a private working copy and, on commit, returns that copy;
//! on abort, it returns a fresh clone of the original instead, discarding
//! every effect the aborted call produced (`docs/concept.md` §2.2).

use std::collections::HashMap;

use crate::call::Call;
use crate::identifiers::{AbortName, ColumnName, ParameterName, TableName};
use crate::ir::{Block, Condition, Effect, Expr, Program, Row, Slot, Terminal, Value};
use crate::store::Store;

/// What a committed or aborted call produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Committed { response: Value },
    Aborted { reason: AbortReason },
}

/// Why a call aborted.
///
/// `MissingRow` and `DuplicateKey` are the evaluator's own structural rules
/// for update/delete and insert (`docs/concept.md` §2.2); `Declared` is a
/// transaction body's own explicit, named rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbortReason {
    Declared(AbortName),
    MissingRow { table: TableName, key: Value },
    DuplicateKey { table: TableName, key: Value },
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
    parameters: &[ParameterName],
    args: &[Value],
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    for effect in &block.effects {
        if let Some(outcome) = apply_effect(effect, parameters, args, store, reads, access_log) {
            return outcome;
        }
    }
    execute_terminal(&block.terminal, parameters, args, store, reads, access_log)
}

/// Applies one effect. Returns `Some(outcome)` if the effect itself aborted
/// the call, `None` if execution should continue with the next effect.
fn apply_effect(
    effect: &Effect,
    parameters: &[ParameterName],
    args: &[Value],
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    match effect {
        Effect::Read { table, key, into } => {
            let key = resolve(key, parameters, args);
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
            let key = resolve(key, parameters, args);
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
            let row = Row {
                columns: columns
                    .iter()
                    .map(|(column, expr)| (column.clone(), resolve(expr, parameters, args)))
                    .collect(),
            };
            store.put_row(table.clone(), key, row);
            None
        }
        Effect::Update {
            table,
            key,
            columns,
        } => {
            let key = resolve(key, parameters, args);
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
                row.columns
                    .insert(column.clone(), resolve(expr, parameters, args));
            }
            store.put_row(table.clone(), key, row);
            None
        }
        Effect::Delete { table, key } => {
            let key = resolve(key, parameters, args);
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
    parameters: &[ParameterName],
    args: &[Value],
    store: &mut Store,
    reads: &mut HashMap<Slot, Option<Row>>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    match terminal {
        Terminal::Return(expr) => Outcome::Committed {
            response: resolve(expr, parameters, args),
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
            execute_block(block, parameters, args, store, reads, access_log)
        }
    }
}

/// Resolves an expression to a concrete value: a constant, or the argument
/// bound to a declared parameter.
fn resolve(expr: &Expr, parameters: &[ParameterName], args: &[Value]) -> Value {
    match expr {
        Expr::Literal(value) => value.clone(),
        Expr::Parameter(name) => {
            let index = parameters
                .iter()
                .position(|parameter| parameter == name)
                .unwrap_or_else(|| panic!("function has no declared parameter named {name:?}"));
            args[index].clone()
        }
    }
}
