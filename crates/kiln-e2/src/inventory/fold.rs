use super::{
    InventoryCall, StockState, ORDER_QUANTITY_MAX, RESTOCK_AMOUNT, STOCK_MIN, THRESHOLD_MAX,
    THRESHOLD_MIN,
};
use crate::run::{declared_aborts, CallOutcome, Run};

declared_aborts! {
    InventoryAbort {
        InvalidQuantity => "invalid_quantity",
        InvalidThreshold => "invalid_threshold",
    }
}

/// The batch's calls applied in order (inputs.md section 3.2).
pub fn serial_fold(batch: &super::Batch) -> Run<InventoryAbort, StockState> {
    Run::fold(batch.initial, &batch.calls, step)
}

/// One call on the state before it: the state after and the outcome.
fn step(state: StockState, call: InventoryCall) -> (StockState, CallOutcome<InventoryAbort>) {
    let stock = state.quantity;
    match call {
        InventoryCall::OrderLine { quantity } => {
            if !(1..=ORDER_QUANTITY_MAX).contains(&quantity) {
                return (
                    state,
                    CallOutcome::declared(InventoryAbort::InvalidQuantity),
                );
            }
            let left = stock - quantity;
            let after = if left >= STOCK_MIN {
                left
            } else {
                left + RESTOCK_AMOUNT
            };
            (
                StockState { quantity: after },
                CallOutcome::committed(stock),
            )
        }
        InventoryCall::StockCheck { threshold } => {
            if !(THRESHOLD_MIN..=THRESHOLD_MAX).contains(&threshold) {
                return (
                    state,
                    CallOutcome::declared(InventoryAbort::InvalidThreshold),
                );
            }
            (state, CallOutcome::committed(i64::from(stock < threshold)))
        }
    }
}
