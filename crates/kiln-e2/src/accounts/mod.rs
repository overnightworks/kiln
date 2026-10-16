//! Mixed account operations (inputs.md section 4): deposits, withdrawals,
//! transfers and freezes over `K = 4` accounts, gated by status and funds.

mod fold;
mod generator;
mod program;

use kiln_ir::{Call, Program, Store};

use crate::oracle::{calls_of, FamilyBatch};
use crate::rng::BATCH_SIZE_MAX;
use crate::rows::{call, integer_at, integer_row, store_of};

pub use fold::{serial_fold, AccountsAbort};
pub use generator::generate;
pub use program::program;

const ACCOUNTS_TABLE: &str = "accounts";
const BALANCE_COLUMN: &str = "balance";
const STATUS_COLUMN: &str = "status";

/// The number of accounts, keys `0..ACCOUNT_COUNT`.
pub const ACCOUNT_COUNT: usize = 4;
/// The largest valid amount.
pub const AMOUNT_MAX: i64 = 10_000;
/// The largest initial balance the generator draws.
pub const INITIAL_BALANCE_MAX: i64 = 50_000;
/// No balance of a generated batch, at any point of its replay, exceeds
/// this: each call raises one balance by at most [`AMOUNT_MAX`]. Within it,
/// no `i64` overflow is reachable and the summary is exact.
pub const BALANCE_BOUND: i64 = INITIAL_BALANCE_MAX + BATCH_SIZE_MAX as i64 * AMOUNT_MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open = 0,
    Frozen = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    pub balance: i64,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountsState {
    pub accounts: [Account; ACCOUNT_COUNT],
}

impl AccountsState {
    pub fn to_store(self) -> Store {
        store_of(
            ACCOUNTS_TABLE,
            (0..).zip(self.accounts).map(|(key, account)| {
                (
                    key,
                    integer_row(&[
                        (BALANCE_COLUMN, account.balance),
                        (STATUS_COLUMN, account.status as i64),
                    ]),
                )
            }),
        )
    }

    pub fn from_store(store: &Store) -> Self {
        let mut keys = 0..;
        Self {
            accounts: [(); ACCOUNT_COUNT].map(|()| {
                let key = keys.next().expect("an unbounded range");
                let status = integer_at(store, ACCOUNTS_TABLE, key, STATUS_COLUMN);
                Account {
                    balance: integer_at(store, ACCOUNTS_TABLE, key, BALANCE_COLUMN),
                    status: if status == Status::Frozen as i64 {
                        Status::Frozen
                    } else {
                        Status::Open
                    },
                }
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountCall {
    Deposit { account: i64, amount: i64 },
    Withdraw { account: i64, amount: i64 },
    Transfer { from: i64, to: i64, amount: i64 },
    Freeze { account: i64 },
    Unfreeze { account: i64 },
}

impl From<AccountCall> for Call {
    fn from(account_call: AccountCall) -> Self {
        match account_call {
            AccountCall::Deposit { account, amount } => call("deposit", &[account, amount]),
            AccountCall::Withdraw { account, amount } => call("withdraw", &[account, amount]),
            AccountCall::Transfer { from, to, amount } => call("transfer", &[from, to, amount]),
            AccountCall::Freeze { account } => call("freeze", &[account]),
            AccountCall::Unfreeze { account } => call("unfreeze", &[account]),
        }
    }
}

/// A generated batch: the initial state and the calls in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub initial: AccountsState,
    pub calls: Vec<AccountCall>,
}

impl FamilyBatch for Batch {
    type Abort = AccountsAbort;
    type State = AccountsState;

    fn program() -> Program {
        program()
    }

    fn initial_state(&self) -> AccountsState {
        self.initial
    }

    fn store_of(state: AccountsState) -> Store {
        state.to_store()
    }

    fn state_of(store: &Store) -> AccountsState {
        AccountsState::from_store(store)
    }

    fn calls(&self) -> Vec<Call> {
        calls_of(&self.calls)
    }
}
