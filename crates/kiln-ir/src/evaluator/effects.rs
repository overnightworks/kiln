//! Applying a function body's effects and terminal against the store:
//! point and range reads, bounded iteration, insert, update, delete, and
//! the branch/return/abort decision a block ends on.

use std::collections::{BTreeMap, HashMap};

use crate::identifiers::{ColumnName, IndexName, TableName};
use crate::ir::{Block, Effect, Expr, Row, Slot, Terminal, Value};
use crate::store::Store;

use super::resolve::{resolve, resolve_for_write, WriteOverflow};
use super::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns, Binding,
    CallContext, Outcome,
};

/// Applies a block's effects in program order, then its terminal. Returns
/// as soon as an effect aborts, before any later effect runs.
pub(super) fn execute_block(
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
        Effect::ReadIndexRange {
            index,
            start,
            end,
            bound,
            into,
        } => apply_read_index_range(
            IndexRangeRead {
                index,
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
        origin: AccessOrigin::TransactionLogic,
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
    let bound = match resolve_bound(range.bound, range.table, ctx, bindings) {
        Ok(bound) => bound,
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
        origin: AccessOrigin::TransactionLogic,
    });
    let rows = store.rows_in_range(range.table, &start, &end);
    bind_within_bound(range.table, bound, rows, range.into, bindings)
}

/// Resolves a range read's bound, which counts rows and so must be an
/// integer.
fn resolve_bound(
    bound: &Expr,
    table: &TableName,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<i64, Outcome> {
    match resolve_or_abort(bound, ctx, bindings)? {
        Value::Integer(bound) => Ok(bound),
        Value::Text(_) => Err(Outcome::Aborted {
            reason: AbortReason::NonIntegerBound {
                table: table.clone(),
            },
        }),
    }
}

/// Binds the matched `rows` to `into`, or aborts with `BoundExceeded`
/// without binding anything if they outnumber `bound`.
fn bind_within_bound(
    table: &TableName,
    bound: i64,
    rows: Vec<Row>,
    into: Slot,
    bindings: &mut HashMap<Slot, Binding>,
) -> Option<Outcome> {
    if usize::try_from(bound).map_or(true, |bound| rows.len() > bound) {
        return Some(Outcome::Aborted {
            reason: AbortReason::BoundExceeded {
                table: table.clone(),
                bound,
                matched: rows.len(),
            },
        });
    }
    bindings.insert(into, Binding::Range(rows));
    None
}

/// An [`Effect::ReadIndexRange`]'s own fields, bundled like [`RangeRead`].
struct IndexRangeRead<'a> {
    index: &'a IndexName,
    start: &'a [Expr],
    end: &'a [Expr],
    bound: &'a Expr,
    into: Slot,
}

/// Reads every row in `[start, end]` of a declared index, binding them in
/// index order to `read.into` (`Effect::ReadIndexRange`). The one log entry
/// names the index and its bounds; no base-row access is logged for the rows
/// it matches.
///
/// Panics if `read.index` is undeclared or a bound's length differs from the
/// index's column count: static faults of the compiled program, like an
/// unbound iteration source.
fn apply_read_index_range(
    read: IndexRangeRead,
    ctx: &CallContext,
    store: &Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let index = ctx
        .schema
        .index(read.index)
        .unwrap_or_else(|| panic!("program declares no index named {:?}", read.index));
    assert!(
        read.start.len() == index.columns.len() && read.end.len() == index.columns.len(),
        "index {:?} has {} columns but its range read gives {} start and {} end bounds",
        read.index,
        index.columns.len(),
        read.start.len(),
        read.end.len(),
    );
    let (start, end) = match (
        resolve_all(read.start, ctx, bindings),
        resolve_all(read.end, ctx, bindings),
    ) {
        (Ok(start), Ok(end)) => (start, end),
        (Err(outcome), _) | (_, Err(outcome)) => return Some(outcome),
    };
    let bound = match resolve_bound(read.bound, &index.table, ctx, bindings) {
        Ok(bound) => bound,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: index.table.clone(),
        kind: AccessKind::IndexRangeRead,
        target: AccessTarget::IndexRange {
            index: index.name.clone(),
            start: start.clone(),
            end: end.clone(),
        },
        columns: AccessedColumns::Named(index.columns.clone()),
        origin: AccessOrigin::TransactionLogic,
    });
    let rows = store.rows_in_index_range(&index.table, &index.columns, &start, &end);
    bind_within_bound(&index.table, bound, rows, read.into, bindings)
}

fn resolve_all(
    exprs: &[Expr],
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Vec<Value>, Outcome> {
    exprs
        .iter()
        .map(|expr| resolve_or_abort(expr, ctx, bindings))
        .collect()
}

/// Runs `body` once for each row `source` bound, in key order, binding the
/// current row to `row` so the body's own effects and expressions can use
/// it (`Effect::Iterate`, `docs/concept.md` §2.1 "bounded iteration", §2.2
/// D6: "bounded iteration proceeds in key order").
///
/// `source` must already be bound by an [`Effect::ReadRange`] in the same
/// body: that read already resolved and bound-checked its range, so
/// `Iterate` re-reads nothing and checks no bound of its own. When the loop
/// ends, the row slot and every slot the body bound read as absent. Panics if
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
    release_iteration_slots(row, body, bindings);
    None
}

/// Rebinds `row` and every slot `body` bound to an absent row, so nothing an
/// iteration read outlives it: after the loop a projection or branch on
/// those slots sees an absence, whether the range held rows or not.
fn release_iteration_slots(row: Slot, body: &[Effect], bindings: &mut HashMap<Slot, Binding>) {
    bindings.insert(row, Binding::Row(None));
    for effect in body {
        match effect {
            Effect::Read { into, .. }
            | Effect::ReadRange { into, .. }
            | Effect::ReadIndexRange { into, .. } => {
                bindings.insert(*into, Binding::Row(None));
            }
            Effect::Iterate { row, body, .. } => release_iteration_slots(*row, body, bindings),
            Effect::Insert { .. } | Effect::Update { .. } | Effect::Delete { .. } => {}
        }
    }
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
        origin: AccessOrigin::TransactionLogic,
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
        origin: AccessOrigin::TransactionLogic,
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
        origin: AccessOrigin::TransactionLogic,
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
                WriteOverflow::NonText => Outcome::Aborted {
                    reason: AbortReason::NonTextValue {
                        table: table.clone(),
                        column: column.clone(),
                        key: key.clone(),
                    },
                },
                WriteOverflow::Width => Outcome::Aborted {
                    reason: AbortReason::Overflow(super::OverflowReason::Width {
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
                crate::ir::Condition::RowPresent(slot) => {
                    bindings.get(slot).is_some_and(Binding::is_present)
                }
            };
            let block = if present { when_present } else { when_absent };
            execute_block(block, ctx, store, bindings, access_log)
        }
    }
}
