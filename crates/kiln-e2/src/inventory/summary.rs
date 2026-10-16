//! Inventory's summary algebra (inputs.md section 3.3): with `t = stock - 10`
//! every valid order line is a translation `t ↦ (t - d) mod 91`.

use super::{
    InventoryAbort, InventoryCall, StockState, ORDER_QUANTITY_MAX, RESTOCK_AMOUNT, STOCK_MIN,
    THRESHOLD_MAX, THRESHOLD_MIN,
};
use crate::run::CallOutcome;
use crate::summary::{OperationCounter, Summary};

/// The shift `d` with `0 <= d <= 90`; one byte on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventorySummary {
    shift: u8,
}

const REPRESENTATION_BYTES: usize = 1;

fn is_valid_quantity(quantity: i64) -> bool {
    (1..=ORDER_QUANTITY_MAX).contains(&quantity)
}

fn is_valid_threshold(threshold: i64) -> bool {
    (THRESHOLD_MIN..=THRESHOLD_MAX).contains(&threshold)
}

fn add<C: OperationCounter>(counter: &mut C, left: i64, right: i64) -> i64 {
    counter.count();
    left + right
}

fn subtract<C: OperationCounter>(counter: &mut C, left: i64, right: i64) -> i64 {
    counter.count();
    left - right
}

fn subtract_if<C: OperationCounter>(
    counter: &mut C,
    condition: bool,
    value: i64,
    amount: i64,
) -> i64 {
    counter.count();
    if condition {
        value - amount
    } else {
        value
    }
}

fn add_if<C: OperationCounter>(counter: &mut C, condition: bool, value: i64, amount: i64) -> i64 {
    counter.count();
    if condition {
        value + amount
    } else {
        value
    }
}

impl Summary for InventorySummary {
    type Call = InventoryCall;
    type State = StockState;
    type Abort = InventoryAbort;

    fn atomic(call: InventoryCall) -> Self {
        let shift = match call {
            InventoryCall::OrderLine { quantity } if is_valid_quantity(quantity) => quantity,
            InventoryCall::OrderLine { .. } | InventoryCall::StockCheck { .. } => 0,
        };
        Self {
            shift: u8::try_from(shift).expect("a valid quantity is at most 10"),
        }
    }

    fn combine<C: OperationCounter>(&self, later: &Self, counter: &mut C) -> Self {
        let sum = add(counter, self.shift.into(), later.shift.into());
        let reduced = subtract_if(counter, sum >= RESTOCK_AMOUNT, sum, RESTOCK_AMOUNT);
        Self {
            shift: u8::try_from(reduced).expect("a reduced shift is below 91"),
        }
    }

    fn apply<C: OperationCounter>(&self, state: StockState, counter: &mut C) -> StockState {
        let shifted = subtract(counter, state.quantity, self.shift.into());
        let margin = subtract(counter, shifted, STOCK_MIN);
        StockState {
            quantity: add_if(counter, margin < 0, shifted, RESTOCK_AMOUNT),
        }
    }

    fn response(call: InventoryCall, pre_state: StockState) -> CallOutcome<InventoryAbort> {
        let stock = pre_state.quantity;
        match call {
            InventoryCall::OrderLine { quantity } if !is_valid_quantity(quantity) => {
                CallOutcome::declared(InventoryAbort::InvalidQuantity)
            }
            InventoryCall::OrderLine { .. } => CallOutcome::committed(stock),
            InventoryCall::StockCheck { threshold } if !is_valid_threshold(threshold) => {
                CallOutcome::declared(InventoryAbort::InvalidThreshold)
            }
            InventoryCall::StockCheck { threshold } => {
                CallOutcome::committed(i64::from(stock < threshold))
            }
        }
    }

    fn size_in_bytes(&self) -> usize {
        REPRESENTATION_BYTES
    }
}
