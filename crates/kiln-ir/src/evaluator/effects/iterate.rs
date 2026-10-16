use std::collections::HashMap;

use crate::ir::{Effect, Slot};
use crate::store::Store;

use super::super::{AccessLogEntry, Binding, BoundRow, CallContext, Outcome};
use super::apply_effect;

/// Runs `body` once for each row `source` bound, in the order the read
/// bound them, binding the current keyed row to `row` so the body's own
/// effects and expressions can use it (`Effect::Iterate`,
/// `docs/concept.md` §2.1 "bounded iteration", §2.2 D6: key order for a
/// primary-key range, index order, narrowed to the indexed columns, for an
/// index range).
///
/// `source` must already be bound by an [`Effect::ReadRange`] or
/// [`Effect::ReadIndexRange`] in the same body: that read already resolved
/// and bound-checked its range, so
/// `Iterate` re-reads nothing and checks no bound of its own. When the loop
/// ends, the row slot and every slot the body bound read as absent. Panics if
/// `source` is unbound or was bound by a point read instead — a contract
/// violation by whatever built the effect, not a semantic outcome `run`
/// reports.
pub(super) fn apply_iterate(
    source: Slot,
    row: Slot,
    body: &[Effect],
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let (table, rows) = match bindings.get(&source) {
        Some(Binding::Range { table, rows }) => (table.clone(), rows.clone()),
        Some(Binding::Row(_)) => {
            panic!("iteration reads slot {source:?} as its range, but a point read bound it")
        }
        None => panic!("iteration reads slot {source:?} before any range read bound it"),
    };
    for current in rows {
        let bound = BoundRow {
            table: table.clone(),
            keyed: current,
        };
        bindings.insert(row, Binding::Row(Some(bound)));
        for effect in body {
            if let Some(outcome) = apply_effect(effect, ctx, store, bindings, access_log) {
                return Some(outcome);
            }
        }
    }
    release_iteration_slots(row, body, bindings);
    None
}

/// Rebinds `row` and every slot `body` bound to an absent row, so nothing an
/// iteration read outlives it: after the loop a projection or branch on
/// those slots sees an absence, whether the range held rows or not.
pub(super) fn release_iteration_slots(
    row: Slot,
    body: &[Effect],
    bindings: &mut HashMap<Slot, Binding>,
) {
    bindings.insert(row, Binding::Row(None));
    for effect in body {
        match effect {
            Effect::Read { into, .. }
            | Effect::ReadRange { into, .. }
            | Effect::ReadIndexRange { into, .. } => {
                bindings.insert(*into, Binding::Row(None));
            }
            Effect::Iterate { row, body, .. } => release_iteration_slots(*row, body, bindings),
            Effect::Insert { .. } | Effect::Update { .. } | Effect::Delete { .. } => {}
        }
    }
}
