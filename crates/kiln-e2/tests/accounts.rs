mod support;

use kiln_e2::accounts::{
    program, Account, AccountCall,
    AccountCall::{Deposit, Freeze, Transfer, Unfreeze, Withdraw},
    AccountsState, Status,
};
use kiln_ir::Outcome;
use support::{aborted, assert_rows, committed, final_store, outcomes_of};

/// Balances and, per account, its status as the freeze writes it: 0 open, 1 frozen.
fn accounts(balances: [i64; 4], statuses: [i64; 4]) -> AccountsState {
    let [first, second, third, fourth] = std::array::from_fn(|index| Account {
        balance: balances[index],
        status: if statuses[index] == 1 {
            Status::Frozen
        } else {
            Status::Open
        },
    });
    AccountsState {
        accounts: [first, second, third, fourth],
    }
}

const OPEN: [i64; 4] = [0; 4];

/// One call from a state: its outcome and the state after it.
type Case = (AccountsState, AccountCall, Outcome, AccountsState);

fn assert_cases(cases: Vec<Case>) {
    for (before, call, outcome, after) in cases {
        assert_rows(
            &program(),
            before.to_store(),
            &[(call, outcome, after)],
            AccountsState::from_store,
        );
    }
}

fn with_balances(balances: [i64; 4]) -> AccountsState {
    accounts(balances, OPEN)
}

mod kiln_e2_09 {
    use super::*;

    #[test]
    fn deposit_and_withdraw_return_the_balance_after_the_call() {
        assert_cases(vec![
            (
                with_balances([100, 0, 0, 0]),
                Deposit {
                    account: 0,
                    amount: 30,
                },
                committed(130),
                with_balances([130, 0, 0, 0]),
            ),
            (
                with_balances([0, 50, 0, 0]),
                Withdraw {
                    account: 1,
                    amount: 20,
                },
                committed(30),
                with_balances([0, 30, 0, 0]),
            ),
            (
                with_balances([0, 50, 0, 0]),
                Withdraw {
                    account: 1,
                    amount: 50,
                },
                committed(0),
                with_balances([0, 0, 0, 0]),
            ),
            (
                with_balances([0, 50, 0, 0]),
                Withdraw {
                    account: 1,
                    amount: 51,
                },
                aborted("insufficient_funds"),
                with_balances([0, 50, 0, 0]),
            ),
            (
                with_balances([0, 0, 0, 0]),
                Deposit {
                    account: 3,
                    amount: 10_000,
                },
                committed(10_000),
                with_balances([0, 0, 0, 10_000]),
            ),
        ]);
    }
}

mod kiln_e2_10 {
    use super::*;

    #[test]
    fn transfer_moves_the_amount_and_returns_the_source_balance() {
        assert_cases(vec![
            (
                with_balances([130, 50, 0, 20]),
                Transfer {
                    from: 0,
                    to: 2,
                    amount: 40,
                },
                committed(90),
                with_balances([90, 50, 40, 20]),
            ),
            (
                with_balances([40, 0, 0, 0]),
                Transfer {
                    from: 0,
                    to: 3,
                    amount: 40,
                },
                committed(0),
                with_balances([0, 0, 0, 40]),
            ),
            (
                with_balances([40, 0, 0, 0]),
                Transfer {
                    from: 0,
                    to: 3,
                    amount: 41,
                },
                aborted("insufficient_funds"),
                with_balances([40, 0, 0, 0]),
            ),
            (
                with_balances([0, 0, 0, 5]),
                Transfer {
                    from: 3,
                    to: 0,
                    amount: 5,
                },
                committed(0),
                with_balances([5, 0, 0, 0]),
            ),
            (
                with_balances([0, 0, 0, 5]),
                Transfer {
                    from: 3,
                    to: 3,
                    amount: 5,
                },
                aborted("same_account"),
                with_balances([0, 0, 0, 5]),
            ),
        ]);
    }
}

mod kiln_e2_11 {
    use super::*;

    #[test]
    fn freeze_and_unfreeze_return_the_balance_and_set_the_status() {
        assert_cases(vec![
            (
                accounts([0, 7, 0, 0], OPEN),
                Freeze { account: 1 },
                committed(7),
                accounts([0, 7, 0, 0], [0, 1, 0, 0]),
            ),
            (
                accounts([0, 7, 0, 0], [0, 1, 0, 0]),
                Freeze { account: 1 },
                aborted("already_frozen"),
                accounts([0, 7, 0, 0], [0, 1, 0, 0]),
            ),
            (
                accounts([0, 7, 0, 0], [0, 1, 0, 0]),
                Unfreeze { account: 1 },
                committed(7),
                accounts([0, 7, 0, 0], OPEN),
            ),
            (
                accounts([0, 7, 0, 0], OPEN),
                Unfreeze { account: 1 },
                aborted("not_frozen"),
                accounts([0, 7, 0, 0], OPEN),
            ),
        ]);
    }
}

mod kiln_e2_12 {
    use super::*;

    fn unchanged(state: AccountsState, call: AccountCall, name: &str) -> Case {
        (state, call, aborted(name), state)
    }

    #[test]
    fn the_first_failing_test_decides_in_the_frozen_order() {
        let first_frozen_and_poor = accounts([5, 0, 0, 0], [1, 0, 0, 0]);
        let second_frozen = accounts([5, 9, 0, 0], [0, 1, 0, 0]);
        assert_cases(vec![
            // Arguments before presence and status: account 1 is frozen, yet the amount decides.
            unchanged(
                second_frozen,
                Withdraw {
                    account: 1,
                    amount: 20_000,
                },
                "invalid_amount",
            ),
            unchanged(
                second_frozen,
                Deposit {
                    account: 4,
                    amount: 0,
                },
                "invalid_amount",
            ),
            unchanged(
                second_frozen,
                Transfer {
                    from: 1,
                    to: 1,
                    amount: 0,
                },
                "invalid_amount",
            ),
            // The amount is at most 10 000.
            unchanged(
                second_frozen,
                Deposit {
                    account: 0,
                    amount: 10_001,
                },
                "invalid_amount",
            ),
            // same_account before presence.
            unchanged(
                second_frozen,
                Transfer {
                    from: 4,
                    to: 4,
                    amount: 5,
                },
                "same_account",
            ),
            // Presence before status and funds, the source before the target.
            unchanged(
                second_frozen,
                Withdraw {
                    account: 4,
                    amount: 5,
                },
                "account_not_found",
            ),
            unchanged(
                second_frozen,
                Transfer {
                    from: 4,
                    to: 7,
                    amount: 5,
                },
                "account_not_found",
            ),
            unchanged(
                second_frozen,
                Transfer {
                    from: 1,
                    to: 4,
                    amount: 5,
                },
                "account_not_found",
            ),
            // Status before funds: a frozen source with short funds is frozen.
            unchanged(
                first_frozen_and_poor,
                Withdraw {
                    account: 0,
                    amount: 6,
                },
                "account_frozen",
            ),
            unchanged(
                first_frozen_and_poor,
                Transfer {
                    from: 0,
                    to: 1,
                    amount: 6,
                },
                "account_frozen",
            ),
            unchanged(
                second_frozen,
                Transfer {
                    from: 0,
                    to: 1,
                    amount: 6,
                },
                "account_frozen",
            ),
            unchanged(
                second_frozen,
                Transfer {
                    from: 1,
                    to: 0,
                    amount: 6,
                },
                "account_frozen",
            ),
            unchanged(
                second_frozen,
                Deposit {
                    account: 1,
                    amount: 6,
                },
                "account_frozen",
            ),
        ]);
    }
}

mod kiln_e2_13 {
    use super::*;

    #[test]
    fn an_account_outside_keys_zero_to_three_is_not_found() {
        let state = accounts([1, 2, 3, 4], OPEN);
        let missing = |call| (state, call, aborted("account_not_found"), state);
        assert_cases(vec![
            missing(Deposit {
                account: 4,
                amount: 5,
            }),
            missing(Withdraw {
                account: 4,
                amount: 5,
            }),
            missing(Withdraw {
                account: -1,
                amount: 5,
            }),
            missing(Transfer {
                from: 4,
                to: 0,
                amount: 5,
            }),
            missing(Transfer {
                from: 0,
                to: 4,
                amount: 5,
            }),
            missing(Freeze { account: 4 }),
            missing(Unfreeze { account: 4 }),
        ]);
    }
}

mod kiln_e2_14 {
    use super::*;

    type Row = (AccountCall, Outcome, AccountsState);

    #[test]
    fn a1_every_outcome() {
        let rows: Vec<Row> = vec![
            (
                Deposit {
                    account: 0,
                    amount: 30,
                },
                committed(130),
                accounts([130, 50, 0, 20], [0, 0, 1, 0]),
            ),
            (
                Transfer {
                    from: 0,
                    to: 2,
                    amount: 40,
                },
                aborted("account_frozen"),
                accounts([130, 50, 0, 20], [0, 0, 1, 0]),
            ),
            (
                Unfreeze { account: 2 },
                committed(0),
                accounts([130, 50, 0, 20], OPEN),
            ),
            (
                Transfer {
                    from: 0,
                    to: 2,
                    amount: 40,
                },
                committed(90),
                accounts([90, 50, 40, 20], OPEN),
            ),
            (
                Withdraw {
                    account: 1,
                    amount: 70,
                },
                aborted("insufficient_funds"),
                accounts([90, 50, 40, 20], OPEN),
            ),
            (
                Withdraw {
                    account: 1,
                    amount: 50,
                },
                committed(0),
                accounts([90, 0, 40, 20], OPEN),
            ),
            (
                Transfer {
                    from: 2,
                    to: 3,
                    amount: 41,
                },
                aborted("insufficient_funds"),
                accounts([90, 0, 40, 20], OPEN),
            ),
            (
                Freeze { account: 1 },
                committed(0),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Deposit {
                    account: 1,
                    amount: 10,
                },
                aborted("account_frozen"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Withdraw {
                    account: 1,
                    amount: 20_000,
                },
                aborted("invalid_amount"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Freeze { account: 1 },
                aborted("already_frozen"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Unfreeze { account: 3 },
                aborted("not_frozen"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Transfer {
                    from: 3,
                    to: 3,
                    amount: 5,
                },
                aborted("same_account"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Deposit {
                    account: 0,
                    amount: 0,
                },
                aborted("invalid_amount"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
            (
                Withdraw {
                    account: 4,
                    amount: 5,
                },
                aborted("account_not_found"),
                accounts([90, 0, 40, 20], [0, 1, 0, 0]),
            ),
        ];
        assert_rows(
            &program(),
            accounts([100, 50, 0, 20], [0, 0, 1, 0]).to_store(),
            &rows,
            AccountsState::from_store,
        );
    }

    #[test]
    fn a2_a_withdrawal_fails_before_a_deposit_and_succeeds_after_it() {
        let withdraw = Withdraw {
            account: 0,
            amount: 50,
        };
        let deposit = Deposit {
            account: 0,
            amount: 40,
        };
        let initial = with_balances([30, 0, 0, 0]);
        let insufficient = || aborted("insufficient_funds");
        let cases = [
            (
                [withdraw, deposit, withdraw],
                [insufficient(), committed(70), committed(20)],
                20,
            ),
            (
                [deposit, withdraw, withdraw],
                [committed(70), committed(20), insufficient()],
                20,
            ),
            (
                [withdraw, withdraw, deposit],
                [insufficient(), insufficient(), committed(70)],
                70,
            ),
        ];
        for (calls, outcomes, final_balance) in cases {
            assert_eq!(
                outcomes_of(&program(), initial.to_store(), &calls),
                outcomes
            );
            let last =
                AccountsState::from_store(&final_store(&program(), initial.to_store(), &calls));
            assert_eq!(last, with_balances([final_balance, 0, 0, 0]));
        }
    }

    #[test]
    fn a3_a_transfer_couples_two_accounts() {
        let calls = [
            Transfer {
                from: 0,
                to: 1,
                amount: 20,
            },
            Withdraw {
                account: 1,
                amount: 20,
            },
        ];
        let funded = with_balances([30, 0, 0, 0]);
        assert_eq!(
            outcomes_of(&program(), funded.to_store(), &calls),
            [committed(10), committed(0)]
        );
        assert_eq!(
            AccountsState::from_store(&final_store(&program(), funded.to_store(), &calls)),
            with_balances([10, 0, 0, 0])
        );

        let short = with_balances([15, 0, 0, 0]);
        assert_eq!(
            outcomes_of(&program(), short.to_store(), &calls),
            [aborted("insufficient_funds"), aborted("insufficient_funds")]
        );
        assert_eq!(
            AccountsState::from_store(&final_store(&program(), short.to_store(), &calls)),
            short
        );
    }

    #[test]
    fn a4_three_withdrawals_of_two_from_five() {
        let calls = [Withdraw {
            account: 0,
            amount: 2,
        }; 3];
        let initial = with_balances([5, 0, 0, 0]);
        assert_eq!(
            outcomes_of(&program(), initial.to_store(), &calls),
            [committed(3), committed(1), aborted("insufficient_funds")]
        );
        assert_eq!(
            AccountsState::from_store(&final_store(&program(), initial.to_store(), &calls)),
            with_balances([1, 0, 0, 0])
        );
    }

    #[test]
    fn a5_a_freeze_before_a_deposit_blocks_it_and_after_it_does_not() {
        let initial = accounts([100, 50, 0, 20], OPEN);
        let frozen_first = [
            Freeze { account: 0 },
            Deposit {
                account: 0,
                amount: 10,
            },
        ];
        let deposit_first = [
            Deposit {
                account: 0,
                amount: 10,
            },
            Freeze { account: 0 },
        ];

        assert_eq!(
            outcomes_of(&program(), initial.to_store(), &frozen_first),
            [committed(100), aborted("account_frozen")]
        );
        assert_eq!(
            AccountsState::from_store(&final_store(&program(), initial.to_store(), &frozen_first)),
            accounts([100, 50, 0, 20], [1, 0, 0, 0])
        );
        assert_eq!(
            outcomes_of(&program(), initial.to_store(), &deposit_first),
            [committed(110), committed(110)]
        );
        assert_eq!(
            AccountsState::from_store(&final_store(&program(), initial.to_store(), &deposit_first)),
            accounts([110, 50, 0, 20], [1, 0, 0, 0])
        );
    }
}
