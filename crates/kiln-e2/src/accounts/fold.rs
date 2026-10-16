use super::{Account, AccountCall, AccountsState, Status, ACCOUNT_COUNT, AMOUNT_MAX};
use crate::run::{declared_aborts, CallOutcome, Run};

declared_aborts! {
    AccountsAbort {
        InvalidAmount => "invalid_amount",
        SameAccount => "same_account",
        AccountNotFound => "account_not_found",
        AccountFrozen => "account_frozen",
        InsufficientFunds => "insufficient_funds",
        AlreadyFrozen => "already_frozen",
        NotFrozen => "not_frozen",
    }
}

/// The batch's calls applied in order (inputs.md section 4.2).
pub fn serial_fold(batch: &super::Batch) -> Run<AccountsAbort, AccountsState> {
    Run::fold(batch.initial, &batch.calls, step)
}

/// One call on the state before it: the state after and the outcome. An
/// aborted call leaves the state as it was.
fn step(state: AccountsState, call: AccountCall) -> (AccountsState, CallOutcome<AccountsAbort>) {
    match apply(state, call) {
        Ok((after, response)) => (after, CallOutcome::committed(response)),
        Err(abort) => (state, CallOutcome::declared(abort)),
    }
}

type Committed = (AccountsState, i64);

fn apply(state: AccountsState, call: AccountCall) -> Result<Committed, AccountsAbort> {
    match call {
        AccountCall::Deposit { account, amount } => {
            check_amount(amount)?;
            let index = locate(account)?;
            ensure_open(state, index)?;
            Ok(move_funds(state, index, amount))
        }
        AccountCall::Withdraw { account, amount } => {
            check_amount(amount)?;
            let index = locate(account)?;
            ensure_open(state, index)?;
            ensure_funds(state, index, amount)?;
            Ok(move_funds(state, index, -amount))
        }
        AccountCall::Transfer { from, to, amount } => transfer(state, from, to, amount),
        AccountCall::Freeze { account } => {
            let index = locate(account)?;
            if state.accounts[index].status == Status::Frozen {
                return Err(AccountsAbort::AlreadyFrozen);
            }
            Ok(set_status(state, index, Status::Frozen))
        }
        AccountCall::Unfreeze { account } => {
            let index = locate(account)?;
            if state.accounts[index].status == Status::Open {
                return Err(AccountsAbort::NotFrozen);
            }
            Ok(set_status(state, index, Status::Open))
        }
    }
}

fn transfer(
    state: AccountsState,
    from: i64,
    to: i64,
    amount: i64,
) -> Result<Committed, AccountsAbort> {
    check_amount(amount)?;
    if from == to {
        return Err(AccountsAbort::SameAccount);
    }
    let source = locate(from)?;
    let target = locate(to)?;
    ensure_open(state, source)?;
    ensure_open(state, target)?;
    ensure_funds(state, source, amount)?;
    let (debited, response) = move_funds(state, source, -amount);
    Ok((move_funds(debited, target, amount).0, response))
}

fn check_amount(amount: i64) -> Result<(), AccountsAbort> {
    if (1..=AMOUNT_MAX).contains(&amount) {
        Ok(())
    } else {
        Err(AccountsAbort::InvalidAmount)
    }
}

fn locate(key: i64) -> Result<usize, AccountsAbort> {
    usize::try_from(key)
        .ok()
        .filter(|index| *index < ACCOUNT_COUNT)
        .ok_or(AccountsAbort::AccountNotFound)
}

fn ensure_open(state: AccountsState, index: usize) -> Result<(), AccountsAbort> {
    match state.accounts[index].status {
        Status::Open => Ok(()),
        Status::Frozen => Err(AccountsAbort::AccountFrozen),
    }
}

fn ensure_funds(state: AccountsState, index: usize, amount: i64) -> Result<(), AccountsAbort> {
    if state.accounts[index].balance < amount {
        Err(AccountsAbort::InsufficientFunds)
    } else {
        Ok(())
    }
}

/// Adds `delta` to the account and answers with its balance after.
fn move_funds(mut state: AccountsState, index: usize, delta: i64) -> Committed {
    state.accounts[index].balance += delta;
    (state, state.accounts[index].balance)
}

/// Sets the status and answers with the account's balance.
fn set_status(mut state: AccountsState, index: usize, status: Status) -> Committed {
    let Account { balance, .. } = state.accounts[index];
    state.accounts[index].status = status;
    (state, balance)
}
