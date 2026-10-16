use super::{
    Account, AccountCall, AccountsState, Batch, Status, ACCOUNT_COUNT, AMOUNT_MAX,
    INITIAL_BALANCE_MAX,
};
use crate::rng::{batch_generator, BatchSize, Family, SplitMix64};

const MALFORMED_ONE_IN: i64 = 100;
const FROZEN_ONE_IN: i64 = 4;
/// Cumulative shares of the call mix, out of 100.
const DEPOSIT_BELOW: i64 = 30;
const WITHDRAW_BELOW: i64 = 60;
const TRANSFER_BELOW: i64 = 90;
const FREEZE_BELOW: i64 = 95;
const LAST_ACCOUNT: i64 = ACCOUNT_COUNT as i64 - 1;
const ABSENT_ACCOUNT: i64 = ACCOUNT_COUNT as i64;
const MALFORMED_AMOUNT: i64 = 0;

/// Batch `batch_index` of `size` calls (inputs.md section 4.5).
pub fn generate(size: BatchSize, batch_index: u32) -> Batch {
    let mut generator = batch_generator(Family::Accounts, size, batch_index);
    let initial = AccountsState {
        accounts: [(); ACCOUNT_COUNT].map(|()| draw_account(&mut generator)),
    };
    let calls = (0..size.get()).map(|_| draw_call(&mut generator)).collect();
    Batch { initial, calls }
}

fn draw_account(generator: &mut SplitMix64) -> Account {
    let balance = generator.uniform(0, INITIAL_BALANCE_MAX);
    let status = if generator.uniform(0, FROZEN_ONE_IN - 1) == 0 {
        Status::Frozen
    } else {
        Status::Open
    };
    Account { balance, status }
}

fn draw_call(generator: &mut SplitMix64) -> AccountCall {
    let kind = generator.uniform(0, 99);
    if kind < WITHDRAW_BELOW {
        let account = generator.uniform(0, LAST_ACCOUNT);
        let amount = draw_amount(generator);
        if kind < DEPOSIT_BELOW {
            AccountCall::Deposit { account, amount }
        } else {
            AccountCall::Withdraw { account, amount }
        }
    } else if kind < TRANSFER_BELOW {
        let from = generator.uniform(0, LAST_ACCOUNT);
        let skipping_from = generator.uniform(0, LAST_ACCOUNT - 1);
        let to = if skipping_from < from {
            skipping_from
        } else {
            skipping_from + 1
        };
        let amount = draw_amount(generator);
        AccountCall::Transfer { from, to, amount }
    } else {
        let drawn = generator.uniform(0, LAST_ACCOUNT);
        let account = if is_malformed(generator) {
            ABSENT_ACCOUNT
        } else {
            drawn
        };
        if kind < FREEZE_BELOW {
            AccountCall::Freeze { account }
        } else {
            AccountCall::Unfreeze { account }
        }
    }
}

/// The amount, then the malformed share `m`, in the freeze's draw order.
fn draw_amount(generator: &mut SplitMix64) -> i64 {
    let drawn = generator.uniform(1, AMOUNT_MAX);
    if is_malformed(generator) {
        MALFORMED_AMOUNT
    } else {
        drawn
    }
}

fn is_malformed(generator: &mut SplitMix64) -> bool {
    generator.uniform(0, MALFORMED_ONE_IN - 1) == 0
}
