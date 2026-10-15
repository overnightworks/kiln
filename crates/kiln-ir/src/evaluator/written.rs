//! The rows a transaction wrote, derived from its access log: the one
//! owner every end-of-transaction check asks which rows it must judge.
//! Deriving them from the log is sound only because a failed write always
//! aborts the call; a write that logged and then did nothing (an upsert or
//! insert-if-absent) would make every check judge a row never written.

use std::collections::BTreeSet;

use crate::identifiers::{ColumnName, TableName};
use crate::ir::Value;

use super::{AccessKind, AccessLogEntry, AccessTarget, AccessedColumns};

/// Keys of the rows of `table` the transaction inserted or updated with a
/// value for `column`, however often it wrote each.
pub(super) fn keys_written_on(
    access_log: &[AccessLogEntry],
    table: &TableName,
    column: &ColumnName,
) -> BTreeSet<Value> {
    point_keys(access_log, table, |entry| {
        matches!(entry.kind, AccessKind::Insert | AccessKind::Update)
            && match &entry.columns {
                AccessedColumns::Named(columns) => columns.contains(column),
                AccessedColumns::EntireRow => false,
            }
    })
}

/// Keys of the rows of `table` the transaction deleted.
pub(super) fn keys_deleted_from(
    access_log: &[AccessLogEntry],
    table: &TableName,
) -> BTreeSet<Value> {
    point_keys(access_log, table, |entry| entry.kind == AccessKind::Delete)
}

fn point_keys(
    access_log: &[AccessLogEntry],
    table: &TableName,
    selects: impl Fn(&AccessLogEntry) -> bool,
) -> BTreeSet<Value> {
    access_log
        .iter()
        .filter(|entry| &entry.table == table && selects(entry))
        .filter_map(|entry| match &entry.target {
            AccessTarget::Point(key) => Some(key.clone()),
            AccessTarget::Range { .. } => None,
        })
        .collect()
}
