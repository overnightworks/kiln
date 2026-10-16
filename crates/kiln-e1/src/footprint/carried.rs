//! Slots an iteration's body carries from one iteration to the next. The walk
//! visits a body once, as its first iteration, so it sees a slot as the loop
//! found it. A body that uses a slot before binding it in the iteration, and
//! also binds or releases that slot, hands a later iteration what an earlier
//! one left there, which that single visit cannot see (`KILN-E1-27`).

use std::collections::HashSet;

use kiln_ir::{slots_released_by_iteration, Condition, Effect, Expr, Slot};

use super::node_path::NodePath;

/// A use of `slot` at `used_at` that a later iteration resolves through what
/// an earlier one left in it.
pub struct CarriedSlot {
    pub slot: Slot,
    pub used_at: NodePath,
}

/// The first use, in reading order, that the body of an iteration with row
/// slot `row` makes of a slot it carries across iterations. The row slot is
/// never carried: each iteration binds it afresh.
pub fn first_carried_slot(row: Slot, body: &[Effect], body_at: &NodePath) -> Option<CarriedSlot> {
    let rebound: HashSet<Slot> = slots_released_by_iteration(row, body).into_iter().collect();
    let mut uses = Vec::new();
    collect_uses_before_binding(body, body_at, HashSet::from([row]), &mut uses);
    uses.into_iter()
        .find(|carried| rebound.contains(&carried.slot))
}

/// Collects every use in `body` of a slot outside `bound`, the slots this
/// iteration has already bound or released. A nested iteration binds its row
/// for its own body and, once it ends, has released every slot it bound.
fn collect_uses_before_binding(
    body: &[Effect],
    body_at: &NodePath,
    mut bound: HashSet<Slot>,
    uses: &mut Vec<CarriedSlot>,
) {
    for (index, effect) in body.iter().enumerate() {
        let node = body_at.effect(index);
        let unbound = slots_used_by(effect)
            .into_iter()
            .filter(|slot| !bound.contains(slot));
        uses.extend(unbound.map(|slot| CarriedSlot {
            slot,
            used_at: node.clone(),
        }));
        match effect {
            Effect::Read { into, .. }
            | Effect::ReadRange { into, .. }
            | Effect::ReadIndexRange { into, .. } => {
                bound.insert(*into);
            }
            Effect::Iterate { row, body, .. } => {
                let mut entering = bound.clone();
                entering.insert(*row);
                collect_uses_before_binding(body, &node.iteration_body(), entering, uses);
                bound.extend(slots_released_by_iteration(*row, body));
            }
            Effect::Insert { .. } | Effect::Update { .. } | Effect::Delete { .. } => {}
        }
    }
}

/// The slots `effect` projects or iterates over before it binds anything.
fn slots_used_by(effect: &Effect) -> Vec<Slot> {
    let expressions: Vec<&Expr> = match effect {
        Effect::Read { key, .. } | Effect::Delete { key, .. } => key.iter().collect(),
        Effect::Insert { key, columns, .. } | Effect::Update { key, columns, .. } => {
            key.iter().chain(columns.values()).collect()
        }
        Effect::ReadRange {
            start, end, bound, ..
        }
        | Effect::ReadIndexRange {
            start, end, bound, ..
        } => start.iter().chain(end).chain([bound]).collect(),
        Effect::Iterate { source, .. } => return vec![*source],
    };
    expressions.into_iter().flat_map(projected_slots).collect()
}

fn projected_slots(expression: &Expr) -> Vec<Slot> {
    match expression {
        Expr::Literal(_) | Expr::Parameter(_) => Vec::new(),
        Expr::Projection { slot, .. } => vec![*slot],
        Expr::Arithmetic(_, left, right)
        | Expr::Concat(left, right)
        | Expr::Truncate {
            text: left,
            length: right,
        } => {
            let mut slots = projected_slots(left);
            slots.extend(projected_slots(right));
            slots
        }
        Expr::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            let mut slots = condition_slots(condition);
            slots.extend(projected_slots(when_true));
            slots.extend(projected_slots(when_false));
            slots
        }
    }
}

fn condition_slots(condition: &Condition) -> Vec<Slot> {
    match condition {
        Condition::RowPresent(slot) => vec![*slot],
        Condition::Compare { .. } | Condition::Contains { .. } => condition
            .operands()
            .into_iter()
            .flat_map(projected_slots)
            .collect(),
    }
}
