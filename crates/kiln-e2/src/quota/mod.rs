//! Quota allocation (inputs.md section 2): one hot row whose partial grants and
//! capped refills make both responses and state order dependent.

mod fold;
mod generator;
mod program;

use kiln_ir::{Call, Program, Store};

use crate::oracle::{calls_of, FamilyBatch};
use crate::rows::{call, integer_at, integer_row, store_of};

pub use fold::{serial_fold, QuotaAbort};
pub use generator::generate;
pub use program::program;

const QUOTA_TABLE: &str = "quota";
const REMAINING_COLUMN: &str = "remaining";
const HOT_KEY: i64 = 0;

/// The quota's ceiling.
pub const CEILING: i64 = 1000;
/// The largest valid allocate or refill quantity.
pub const QUANTITY_MAX: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaState {
    pub remaining: i64,
}

impl QuotaState {
    pub fn to_store(self) -> Store {
        store_of(
            QUOTA_TABLE,
            [(HOT_KEY, integer_row(&[(REMAINING_COLUMN, self.remaining)]))],
        )
    }

    pub fn from_store(store: &Store) -> Self {
        Self {
            remaining: integer_at(store, QUOTA_TABLE, HOT_KEY, REMAINING_COLUMN),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaCall {
    Allocate { quantity: i64 },
    Refill { quantity: i64 },
}

impl From<QuotaCall> for Call {
    fn from(quota_call: QuotaCall) -> Self {
        match quota_call {
            QuotaCall::Allocate { quantity } => call("allocate", &[quantity]),
            QuotaCall::Refill { quantity } => call("refill", &[quantity]),
        }
    }
}

/// A generated batch: the initial state and the calls in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub initial: QuotaState,
    pub calls: Vec<QuotaCall>,
}

impl FamilyBatch for Batch {
    type Abort = QuotaAbort;
    type State = QuotaState;

    fn program() -> Program {
        program()
    }

    fn initial_state(&self) -> QuotaState {
        self.initial
    }

    fn store_of(state: QuotaState) -> Store {
        state.to_store()
    }

    fn state_of(store: &Store) -> QuotaState {
        QuotaState::from_store(store)
    }

    fn calls(&self) -> Vec<Call> {
        calls_of(&self.calls)
    }
}
