//! The sequential scan: the three-phase parallel scan's logic on one thread.

use crate::run::Run;
use crate::summary::{Summary, Uncounted};

/// Builds the prefix summaries `P_i = S_1 ⊕ … ⊕ S_i`, derives each call's
/// pre-state as `apply(P_{i-1}, initial)` and its response from that state.
pub fn scan<S: Summary>(initial: S::State, calls: &[S::Call]) -> Run<S::Abort, S::State> {
    let mut prefix: Option<S> = None;
    let outcomes = calls
        .iter()
        .map(|&call| {
            let pre_state = state_after(prefix, initial);
            let atomic = S::atomic(call);
            prefix =
                Some(prefix.map_or(atomic, |earlier| earlier.combine(&atomic, &mut Uncounted)));
            S::response(call, pre_state)
        })
        .collect();
    Run {
        outcomes,
        final_state: state_after(prefix, initial),
    }
}

fn state_after<S: Summary>(prefix: Option<S>, initial: S::State) -> S::State {
    prefix.map_or(initial, |summary| summary.apply(initial, &mut Uncounted))
}
