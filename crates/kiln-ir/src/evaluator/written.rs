//! The rows a transaction wrote, derived from its access log: the one
//! owner every end-of-transaction check asks which rows it must judge.
//! Deriving them from the log is sound only because a failed write always
//! aborts the call; a write that logged and then did nothing (an upsert or
//! insert-if-absent) would make every check judge a row never written.

use std::collections::BTreeSet;

use crate::identifiers::{ColumnName, TableName};
use crate::ir::{Key, Table};

use super::{AccessKind, AccessLogEntry, AccessTarget, AccessedColumns};

/// Keys of the rows of `table` the transaction inserted or updated with a
/// value for `column`, however often it wrote each. An insert writes every
/// key column through its key (D8); an update never writes one.
pub(super) fn keys_written_on(
    access_log: &[AccessLogEntry],
    table: &Table,
    column: &ColumnName,
) -> BTreeSet<Key> {
    let is_key_column = table.key_position(column).is_some();
    point_keys(access_log, &table.name, |entry| {
        let names_column = match &entry.columns {
            AccessedColumns::Named(columns) => columns.contains(column),
            AccessedColumns::EntireRow => false,
        };
        match entry.kind {
            AccessKind::Insert => is_key_column || names_column,
            AccessKind::Update => names_column,
            _ => false,
        }
    })
}

/// Keys of the rows of `table` the transaction deleted.
pub(super) fn keys_deleted_from(access_log: &[AccessLogEntry], table: &TableName) -> BTreeSet<Key> {
    point_keys(access_log, table, |entry| entry.kind == AccessKind::Delete)
}

fn point_keys(
    access_log: &[AccessLogEntry],
    table: &TableName,
    selects: impl Fn(&AccessLogEntry) -> bool,
) -> BTreeSet<Key> {
    access_log
        .iter()
        .filter(|entry| &entry.table == table && selects(entry))
        .filter_map(|entry| match &entry.target {
            AccessTarget::Point(key) => Some(key.clone()),
            AccessTarget::Range { .. }
            | AccessTarget::IndexRange { .. }
            | AccessTarget::ConstraintLookup { .. } => None,
        })
        .collect()
}
