//! The end-of-transaction check of every schema constraint (D1, D2, D5):
//! serial, against the transaction's tentative final state, in one fixed
//! order, stopping at the first violation so later constraints log nothing.

use crate::identifiers::TableName;
use crate::ir::{Key, Schema, Table, Value};
use crate::store::Store;

use super::{
    check, foreign_key, unique, AbortReason, AccessKind, AccessLogEntry, AccessOrigin,
    AccessTarget, AccessedColumns, ConstraintRef,
};

/// Checks unique constraints, then foreign keys, then check constraints,
/// each in declaration order, returning the first violation.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    unique::first_violation(schema, final_store, access_log)
        .or_else(|| foreign_key::first_violation(schema, final_store, access_log))
        .or_else(|| check::first_violation(schema, final_store, access_log))
}

/// The declared table a validated constraint names.
pub(super) fn constrained_table<'a>(schema: &'a Schema, name: &TableName) -> &'a Table {
    schema
        .table(name)
        .unwrap_or_else(|| panic!("a validated constraint names declared table {name:?}"))
}

/// Logs one constraint-origin point read of `table` at `key`, the only
/// footprint a check of a referenced key may leave: a lookup of one key,
/// never a scan (D5).
pub(super) fn log_point_read(
    access_log: &mut Vec<AccessLogEntry>,
    table: &TableName,
    key: Key,
    columns: AccessedColumns,
) {
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(key),
        columns,
        origin: AccessOrigin::ConstraintCheck,
    });
}

/// Logs one constraint-origin lookup of `table` by `tuple`, naming the
/// constraint it serves: the only footprint a check by value may leave,
/// never a scan (D5).
pub(super) fn log_constraint_lookup(
    access_log: &mut Vec<AccessLogEntry>,
    table: &TableName,
    constraint: ConstraintRef,
    tuple: Vec<Value>,
    columns: AccessedColumns,
) {
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::ConstraintLookup,
        target: AccessTarget::ConstraintLookup { constraint, tuple },
        columns,
        origin: AccessOrigin::ConstraintCheck,
    });
}
