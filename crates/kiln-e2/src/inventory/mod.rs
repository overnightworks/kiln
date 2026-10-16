//! Conditional inventory (inputs.md section 3): TPC-C's New-Order stock update on
//! one hot row, whose response reveals the stock each caller finds.

mod fold;
mod generator;
mod program;

use kiln_ir::{Call, Program, Store};

use crate::oracle::{calls_of, FamilyBatch};
use crate::rows::{call, integer_at, integer_row, store_of};

pub use fold::{serial_fold, InventoryAbort};
pub use generator::generate;
pub use program::program;

const STOCK_TABLE: &str = "stock";
const QUANTITY_COLUMN: &str = "quantity";
const HOT_KEY: i64 = 0;

pub const STOCK_MIN: i64 = 10;
pub const STOCK_MAX: i64 = 100;
/// The largest valid order quantity.
pub const ORDER_QUANTITY_MAX: i64 = 10;
pub const RESTOCK_AMOUNT: i64 = 91;
pub const THRESHOLD_MIN: i64 = 10;
pub const THRESHOLD_MAX: i64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockState {
    pub quantity: i64,
}

impl StockState {
    pub fn to_store(self) -> Store {
        store_of(
            STOCK_TABLE,
            [(HOT_KEY, integer_row(&[(QUANTITY_COLUMN, self.quantity)]))],
        )
    }

    pub fn from_store(store: &Store) -> Self {
        Self {
            quantity: integer_at(store, STOCK_TABLE, HOT_KEY, QUANTITY_COLUMN),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryCall {
    OrderLine { quantity: i64 },
    StockCheck { threshold: i64 },
}

impl From<InventoryCall> for Call {
    fn from(inventory_call: InventoryCall) -> Self {
        match inventory_call {
            InventoryCall::OrderLine { quantity } => call("order_line", &[quantity]),
            InventoryCall::StockCheck { threshold } => call("stock_check", &[threshold]),
        }
    }
}

/// A generated batch: the initial state and the calls in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub initial: StockState,
    pub calls: Vec<InventoryCall>,
}

impl FamilyBatch for Batch {
    type Abort = InventoryAbort;
    type State = StockState;

    fn program() -> Program {
        program()
    }

    fn initial_state(&self) -> StockState {
        self.initial
    }

    fn store_of(state: StockState) -> Store {
        state.to_store()
    }

    fn state_of(store: &Store) -> StockState {
        StockState::from_store(store)
    }

    fn calls(&self) -> Vec<Call> {
        calls_of(&self.calls)
    }
}
