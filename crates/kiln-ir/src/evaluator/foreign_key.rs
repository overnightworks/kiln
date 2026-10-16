//! The foreign-key check (D1, D2, D5): written child rows must name a
//! present parent, and a deleted parent must have no remaining children.

use crate::ir::{ForeignKey, Key, Schema};
use crate::store::Store;

use super::constraints::{constrained_table, log_constraint_lookup, log_point_read};
use super::written::{keys_deleted_from, keys_written_on};
use super::{AbortReason, AccessLogEntry, AccessedColumns, ConstraintRef};

/// Checks every foreign key against `final_store`, returning the first
/// violation: for each key, the child rows first, then the deleted parents.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    schema.foreign_keys.iter().find_map(|foreign_key| {
        dangling_reference(schema, foreign_key, final_store, access_log)
            .or_else(|| referenced_parent_deleted(schema, foreign_key, final_store, access_log))
    })
}

/// Each child row written on any of the key's columns logs one point read on
/// the referenced key formed from its final values of all of them, touching no
/// column; a row deleted by the end, or lacking any of the columns, names
/// nothing.
fn dangling_reference(
    schema: &Schema,
    foreign_key: &ForeignKey,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    let child = constrained_table(schema, &foreign_key.table);
    let written_keys = keys_written_on(access_log, child, &foreign_key.columns);
    for key in written_keys {
        let Some(referenced) = final_store
            .row(&child.name, &key)
            .and_then(|row| child.tuple_of(&key, row, &foreign_key.columns))
        else {
            continue;
        };
        let referenced_key = Key(referenced);
        log_point_read(
            access_log,
            &foreign_key.references,
            referenced_key.clone(),
            AccessedColumns::Named(vec![]),
        );
        if final_store
            .row(&foreign_key.references, &referenced_key)
            .is_none()
        {
            return Some(AbortReason::DanglingReference {
                foreign_key: foreign_key.clone(),
                referenced: referenced_key,
            });
        }
    }
    None
}

/// Each parent key deleted and absent at the end logs one lookup of the
/// children naming it, by the same representation the unique check uses.
fn referenced_parent_deleted(
    schema: &Schema,
    foreign_key: &ForeignKey,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    let child = constrained_table(schema, &foreign_key.table);
    let deleted_keys = keys_deleted_from(access_log, &foreign_key.references);
    for key in deleted_keys {
        if final_store.row(&foreign_key.references, &key).is_some() {
            continue;
        }
        log_constraint_lookup(
            access_log,
            &child.name,
            ConstraintRef::ForeignKey(foreign_key.clone()),
            key.0.clone(),
            AccessedColumns::Named(foreign_key.columns.clone()),
        );
        if final_store.any_row_holds(child, &foreign_key.columns, &key.0) {
            return Some(AbortReason::ReferencedParentDeleted {
                foreign_key: foreign_key.clone(),
                key,
            });
        }
    }
    None
}
