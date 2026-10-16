use std::collections::{BTreeMap, HashMap};

use crate::identifiers::{ColumnName, TableName};
use crate::ir::{Expr, Key, Row, Slot, Value};
use crate::store::Store;

use super::super::resolve::{resolve_for_write, WriteOverflow};
use super::super::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns, Binding,
    CallContext, Outcome,
};
use super::key::resolve_key;

/// Inserts a new row, aborting with `DuplicateKey` if one already exists at
/// `key` (`Effect::Insert`).
pub(super) fn apply_insert(
    table: &TableName,
    key: &[Expr],
    columns: &BTreeMap<ColumnName, Expr>,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(table, key, ctx, bindings) {
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
pub(super) fn apply_update(
    table: &TableName,
    key: &[Expr],
    columns: &BTreeMap<ColumnName, Expr>,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(table, key, ctx, bindings) {
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
pub(super) fn apply_delete(
    table: &TableName,
    key: &[Expr],
    ctx: &CallContext,
    store: &mut Store,
    bindings: &HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(table, key, ctx, bindings) {
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
pub(super) fn resolve_write_columns(
    table: &TableName,
    key: &Key,
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
                    reason: AbortReason::Overflow(crate::evaluator::OverflowReason::Width {
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
