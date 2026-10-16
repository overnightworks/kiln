//! The unique-constraint check (D1, D2, D5).

use crate::ir::Schema;
use crate::store::Store;

use super::constraints::{constrained_table, log_constraint_lookup};
use super::written::keys_written_on;
use super::{AbortReason, AccessLogEntry, AccessedColumns, ConstraintRef};

/// Checks every unique constraint against `final_store`, returning the
/// first violation.
///
/// A constraint checks exactly the rows the transaction wrote any of its
/// columns on; each checked row logs one constraint lookup naming the
/// constraint and the unique tuple, so no check scans the table. A row deleted
/// by the end, or lacking any of the columns, has no tuple to check.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    for constraint in &schema.unique_constraints {
        let table = constrained_table(schema, &constraint.table);
        let written_keys = keys_written_on(access_log, table, &constraint.columns);
        for key in written_keys {
            let Some(tuple) = final_store
                .row(&table.name, &key)
                .and_then(|row| table.tuple_of(&key, row, &constraint.columns))
            else {
                continue;
            };
            log_constraint_lookup(
                access_log,
                &table.name,
                ConstraintRef::Unique(constraint.clone()),
                tuple.clone(),
                AccessedColumns::Named(constraint.columns.clone()),
            );
            if final_store.tuple_held_by_another_row(table, &constraint.columns, &tuple, &key) {
                return Some(AbortReason::UniqueViolation {
                    constraint: constraint.clone(),
                    tuple,
                });
            }
        }
    }
    None
}
