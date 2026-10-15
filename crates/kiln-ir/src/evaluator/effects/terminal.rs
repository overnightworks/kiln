use std::collections::HashMap;

use crate::ir::{Condition, Slot, Terminal};
use crate::store::Store;

use super::super::resolve::resolve;
use super::super::{AbortReason, AccessLogEntry, Binding, CallContext, Outcome};
use super::execute_block;

/// Decides a block's outcome once its effects are done: return a response,
/// abort by name, or take one of two branches on whether a prior read found
/// a row.
pub(super) fn execute_terminal(
    terminal: &Terminal,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    match terminal {
        Terminal::Return(expr) => match resolve(expr, ctx, bindings) {
            Ok(response) => Outcome::Committed { response },
            Err(error) => Outcome::Aborted {
                reason: error.into(),
            },
        },
        Terminal::Abort(name) => Outcome::Aborted {
            reason: AbortReason::Declared(name.clone()),
        },
        Terminal::Branch {
            condition,
            when_present,
            when_absent,
        } => {
            let present = match condition_holds(condition, ctx, bindings) {
                Ok(present) => present,
                Err(reason) => return Outcome::Aborted { reason },
            };
            let block = if present { when_present } else { when_absent };
            execute_block(block, ctx, store, bindings, access_log)
        }
    }
}

/// Whether `condition` holds against what this call has read so far; a
/// comparison resolves both operands and logs no access.
pub(super) fn condition_holds(
    condition: &Condition,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<bool, AbortReason> {
    match condition {
        Condition::RowPresent(slot) => Ok(bindings.get(slot).is_some_and(Binding::is_present)),
        Condition::Compare {
            left,
            relation,
            right,
        } => {
            let left = resolve(left, ctx, bindings)?;
            let right = resolve(right, ctx, bindings)?;
            relation
                .holds(&left, &right)
                .ok_or(AbortReason::ComparisonTypeMismatch)
        }
    }
}
