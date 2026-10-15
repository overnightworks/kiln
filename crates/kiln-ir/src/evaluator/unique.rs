//! The end-of-transaction check of the schema's unique constraints
//! (D1, D2, D5): serial, against the transaction's tentative final state.

use std::collections::BTreeSet;

use crate::ir::{Schema, UniqueConstraint, Value};
use crate::store::Store;

use super::{AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns};

/// Checks every unique constraint against `final_store`, returning the
/// first violation.
///
/// A constraint checks exactly the rows the transaction wrote its column on,
/// found in `access_log`, which already records every attempted write; each
/// checked row logs one constraint-origin point read on its unique value, so
/// no check scans the table. A row deleted by the end, or lacking the
/// column, has no value to check.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    for constraint in &schema.unique_constraints {
        let written_keys = keys_written_on(constraint, access_log);
        for key in written_keys {
            let Some(value) = final_store
                .row(&constraint.table, &key)
                .and_then(|row| row.columns.get(&constraint.column))
            else {
                continue;
            };
            access_log.push(AccessLogEntry {
                table: constraint.table.clone(),
                kind: AccessKind::PointRead,
                target: AccessTarget::Point(value.clone()),
                columns: AccessedColumns::Named(vec![constraint.column.clone()]),
                origin: AccessOrigin::ConstraintCheck,
            });
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

fn keys_written_on(
    constraint: &UniqueConstraint,
    access_log: &[AccessLogEntry],
) -> BTreeSet<Value> {
    access_log
        .iter()
        .filter(|entry| entry.table == constraint.table)
        .filter(|entry| matches!(entry.kind, AccessKind::Insert | AccessKind::Update))
        .filter(|entry| match &entry.columns {
            AccessedColumns::Named(columns) => columns.contains(&constraint.column),
            AccessedColumns::EntireRow => false,
        })
        .filter_map(|entry| match &entry.target {
            AccessTarget::Point(key) => Some(key.clone()),
            AccessTarget::Range { .. } => None,
        })
        .collect()
}
