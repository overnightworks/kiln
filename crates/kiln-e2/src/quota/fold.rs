use super::{QuotaAbort, QuotaCall, QuotaState, CEILING, QUANTITY_MAX};
use crate::run::{CallOutcome, Run};

/// The batch's calls applied in order (inputs.md section 2.2).
pub fn serial_fold(batch: &super::Batch) -> Run<QuotaAbort, QuotaState> {
    Run::fold(batch.initial, &batch.calls, step)
}

/// One call on the state before it: the state after and the outcome.
fn step(state: QuotaState, call: QuotaCall) -> (QuotaState, CallOutcome<QuotaAbort>) {
    let (QuotaCall::Allocate { quantity } | QuotaCall::Refill { quantity }) = call;
    if !(1..=QUANTITY_MAX).contains(&quantity) {
        return (state, CallOutcome::declared(QuotaAbort::InvalidQuantity));
    }
    let remaining = state.remaining;
    let (abort, moved, after) = match call {
        QuotaCall::Allocate { .. } => {
            let granted = quantity.min(remaining);
            (QuotaAbort::QuotaExhausted, granted, remaining - granted)
        }
        QuotaCall::Refill { .. } => {
            let added = quantity.min(CEILING - remaining);
            (QuotaAbort::QuotaFull, added, remaining + added)
        }
    };
    if moved == 0 {
        return (state, CallOutcome::declared(abort));
    }
    (
        QuotaState { remaining: after },
        CallOutcome::committed(moved),
    )
}
