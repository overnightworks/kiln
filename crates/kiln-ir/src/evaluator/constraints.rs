//! The end-of-transaction check of every schema constraint (D1, D2, D5):
//! serial, against the transaction's tentative final state, in one fixed
//! order, stopping at the first violation so later constraints log nothing.

use crate::identifiers::TableName;
use crate::ir::{Schema, Value};
use crate::store::Store;

use super::{
    check, foreign_key, unique, AbortReason, AccessKind, AccessLogEntry, AccessOrigin,
    AccessTarget, AccessedColumns,
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

/// Logs one constraint-origin point read of `table` at `key`, the only
/// footprint a check may leave: a lookup by value, never a scan (D5).
pub(super) fn log_lookup(
    access_log: &mut Vec<AccessLogEntry>,
    table: &TableName,
    key: &Value,
    columns: AccessedColumns,
) {
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(key.clone()),
        columns,
        origin: AccessOrigin::ConstraintCheck,
    });
}
