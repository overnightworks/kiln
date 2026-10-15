//! The check-constraint check (D1, D2, D5): each written row is judged on
//! its own columns, so the check reads no other row and logs nothing.

use crate::ir::Schema;
use crate::store::Store;

use super::written::keys_written_on;
use super::{AbortReason, AccessLogEntry};

/// Checks every check constraint against `final_store`, returning the first
/// violation. A row deleted by the end, or lacking the column, has no value
/// to judge.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &[AccessLogEntry],
) -> Option<AbortReason> {
    for constraint in &schema.check_constraints {
        for key in keys_written_on(access_log, &constraint.table, &constraint.column) {
            let value = final_store
                .row(&constraint.table, &key)
                .and_then(|row| row.columns.get(&constraint.column));
            if value.is_some_and(|value| !constraint.accepts(value)) {
                return Some(AbortReason::CheckViolation {
                    constraint: constraint.clone(),
                    key,
                });
            }
        }
    }
    None
}
