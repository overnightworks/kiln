//! The foreign-key check (D1, D2, D5): written child rows must name a
//! present parent, and a deleted parent must have no remaining children.

use crate::ir::{ForeignKey, Schema};
use crate::store::Store;

use super::constraints::log_lookup;
use super::written::{keys_deleted_from, keys_written_on};
use super::{AbortReason, AccessLogEntry, AccessedColumns};

/// Checks every foreign key against `final_store`, returning the first
/// violation: for each key, the child rows first, then the deleted parents.
pub(super) fn first_violation(
    schema: &Schema,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    schema.foreign_keys.iter().find_map(|foreign_key| {
        dangling_reference(foreign_key, final_store, access_log)
            .or_else(|| referenced_parent_deleted(foreign_key, final_store, access_log))
    })
}

/// Each child row written on the key's column logs one point read on the
/// referenced key, touching no column; a row deleted by the end, or lacking
/// the column, names nothing.
fn dangling_reference(
    foreign_key: &ForeignKey,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    let written_keys = keys_written_on(access_log, &foreign_key.table, &foreign_key.column);
    for key in written_keys {
        let Some(referenced) = final_store
            .row(&foreign_key.table, &key)
            .and_then(|row| row.columns.get(&foreign_key.column))
        else {
            continue;
        };
        log_lookup(
            access_log,
            &foreign_key.references,
            referenced,
            AccessedColumns::Named(vec![]),
        );
        if final_store
            .row(&foreign_key.references, referenced)
            .is_none()
        {
            return Some(AbortReason::DanglingReference {
                foreign_key: foreign_key.clone(),
                value: referenced.clone(),
            });
        }
    }
    None
}

/// Each parent key deleted and absent at the end logs one lookup of the
/// children naming it, by the same representation the unique check uses.
fn referenced_parent_deleted(
    foreign_key: &ForeignKey,
    final_store: &Store,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<AbortReason> {
    let deleted_keys = keys_deleted_from(access_log, &foreign_key.references);
    for key in deleted_keys {
        if final_store.row(&foreign_key.references, &key).is_some() {
            continue;
        }
        log_lookup(
            access_log,
            &foreign_key.table,
            &key,
            AccessedColumns::Named(vec![foreign_key.column.clone()]),
        );
        if final_store.any_row_holds(&foreign_key.table, &foreign_key.column, &key) {
            return Some(AbortReason::ReferencedParentDeleted {
                foreign_key: foreign_key.clone(),
                key,
            });
        }
    }
    None
}
