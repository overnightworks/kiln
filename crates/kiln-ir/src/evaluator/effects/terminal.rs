use std::collections::HashMap;

use crate::ir::{Slot, Terminal};
use crate::store::Store;

use super::super::resolve::{condition_holds, resolve};
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
                Err(error) => {
                    return Outcome::Aborted {
                        reason: error.into(),
                    }
                }
            };
            let block = if present { when_present } else { when_absent };
            execute_block(block, ctx, store, bindings, access_log)
        }
    }
}
