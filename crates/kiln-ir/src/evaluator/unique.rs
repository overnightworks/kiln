//! The unique-constraint check (D1, D2, D5).

use crate::ir::Schema;
use crate::store::Store;

use super::constraints::log_lookup;
use super::written::keys_written_on;
use super::{AbortReason, AccessLogEntry, AccessedColumns};

/// Checks every unique constraint against `final_store`, returning the
/// first violation.
///
/// A constraint checks exactly the rows the transaction wrote its column on;
/// each checked row logs one constraint-origin point read on its unique
/// value, so no check scans the table. A row deleted by the end, or lacking
/// the column, has no value to check.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    for constraint in &schema.unique_constraints {
        let written_keys = keys_written_on(access_log, &constraint.table, &constraint.column);
        for key in written_keys {
            let Some(value) = final_store
                .row(&constraint.table, &key)
                .and_then(|row| row.columns.get(&constraint.column))
            else {
                continue;
            };
            log_lookup(
                access_log,
                &constraint.table,
                value,
                AccessedColumns::Named(vec![constraint.column.clone()]),
            );
            if final_store.value_held_by_another_row(
                &constraint.table,
                &constraint.column,
                value,
                &key,
            ) {
                return Some(AbortReason::UniqueViolation {
                    constraint: constraint.clone(),
                    value: value.clone(),
                });
            }
        }
    }
    None
}
