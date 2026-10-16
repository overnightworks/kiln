mod support;

use kiln_e2::accounts::{self, Account, AccountCall::*, AccountsState, Status, BALANCE_BOUND};
use kiln_e2::inventory::{self, InventoryCall, InventoryCall::*, StockState};
use kiln_e2::quota::{self, QuotaCall, QuotaCall::*, QuotaState};
use kiln_e2::{batch_generator, BatchSize, BatchSizeOutOfRange, Family, SplitMix64};
use kiln_ir::{Call, Outcome, Value};
use support::{final_store, replay_all};

fn size(n: u32) -> BatchSize {
    BatchSize::new(n).expect("a size in range")
}

fn first_outputs(state: u64) -> [u64; 3] {
    let mut generator = SplitMix64::from_state(state);
    [(); 3].map(|()| generator.next_u64())
}

mod kiln_e2_18 {
    use super::*;

    #[test]
    fn the_frozen_test_vectors_are_reproduced() {
        assert_eq!(
            first_outputs(0),
            [
                16294208416658607535,
                7960286522194355700,
                487617019471545679
            ]
        );
        assert_eq!(
            first_outputs(0x0001_0040_0000_0000),
            [
                14475248428079389989,
                3002686106093258379,
                170901929721367164
            ]
        );
    }
}

mod kiln_e2_19 {
    use super::*;

    #[test]
    fn uniform_is_lo_plus_the_output_modulo_the_span() {
        // From state 0 the outputs are 16294208416658607535, 7960286522194355700, 487617019471545679.
        let mut generator = SplitMix64::from_state(0);
        assert_eq!(generator.uniform(0, 99), 35);
        assert_eq!(generator.uniform(10, 14), 10);
        assert_eq!(generator.uniform(-5, 94), 74);
    }
}

mod kiln_e2_20 {
    use super::*;

    fn first_output(family: Family, n: u32, index: u32) -> u64 {
        batch_generator(family, size(n), index).next_u64()
    }

    #[test]
    fn the_seed_is_family_size_and_index_in_their_bit_fields() {
        let first_after = |state| SplitMix64::from_state(state).next_u64();
        assert_eq!(
            first_output(Family::Quota, 64, 0),
            first_after(0x0001_0040_0000_0000)
        );
        assert_eq!(
            first_output(Family::Inventory, 3, 5),
            first_after(0x0002_0003_0000_0005)
        );
        assert_eq!(
            first_output(Family::Accounts, 256, 7),
            first_after(0x0003_0100_0000_0007)
        );
    }

    #[test]
    fn the_first_quota_draw_comes_from_the_frozen_vector() {
        // 14475248428079389989 mod 1001
        assert_eq!(
            quota::generate(size(64), 0).initial,
            QuotaState { remaining: 38 }
        );
    }

    #[test]
    fn a_size_outside_one_to_two_hundred_fifty_six_is_refused() {
        assert_eq!(BatchSize::new(0), Err(BatchSizeOutOfRange(0)));
        assert_eq!(BatchSize::new(257), Err(BatchSizeOutOfRange(257)));
        assert_eq!(
            BatchSizeOutOfRange(257).to_string(),
            "batch size 257 is outside 1..=256"
        );
        assert_eq!(BatchSize::new(1).map(BatchSize::get), Ok(1));
        assert_eq!(BatchSize::new(256).map(BatchSize::get), Ok(256));
    }
}

mod kiln_e2_21 {
    use super::*;

    #[test]
    fn the_same_family_size_and_index_give_the_same_batch() {
        assert_eq!(quota::generate(size(64), 3), quota::generate(size(64), 3));
        assert_eq!(
            inventory::generate(size(64), 3),
            inventory::generate(size(64), 3)
        );
        assert_eq!(
            accounts::generate(size(64), 3),
            accounts::generate(size(64), 3)
        );
    }

    #[test]
    fn different_indices_and_sizes_give_different_batches() {
        assert_ne!(quota::generate(size(64), 3), quota::generate(size(64), 4));
        assert_ne!(
            inventory::generate(size(64), 3),
            inventory::generate(size(64), 4)
        );
        assert_ne!(
            accounts::generate(size(64), 3),
            accounts::generate(size(64), 4)
        );
        assert_ne!(
            accounts::generate(size(64), 3).initial,
            accounts::generate(size(65), 3).initial
        );
    }
}

/// Written from the freeze's draw orders and distributions, derived once by a
/// separate implementation of section 5 and the family input sections.
mod kiln_e2_22 {
    use super::*;

    #[test]
    fn batches_follow_the_frozen_draws() {
        let cases: [(u32, u32, i64, Vec<QuotaCall>); 3] = [
            (
                3,
                5,
                296,
                vec![
                    Allocate { quantity: 40 },
                    Allocate { quantity: 35 },
                    Allocate { quantity: 31 },
                ],
            ),
            (
                3,
                3121,
                232,
                vec![
                    Allocate { quantity: 0 },
                    Allocate { quantity: 51 },
                    Refill { quantity: 1 },
                ],
            ),
            (
                7,
                1178,
                65,
                vec![
                    Refill { quantity: 35 },
                    Allocate { quantity: 40 },
                    Refill { quantity: 14 },
                    Allocate { quantity: 0 },
                    Allocate { quantity: 38 },
                    Allocate { quantity: 21 },
                    Allocate { quantity: 51 },
                ],
            ),
        ];
        for (n, index, remaining, calls) in cases {
            let batch = quota::generate(size(n), index);
            assert_eq!(
                batch.initial,
                QuotaState { remaining },
                "size {n} index {index}"
            );
            assert_eq!(batch.calls, calls, "size {n} index {index}");
        }
    }

    #[test]
    fn quantities_are_valid_or_the_two_malformed_replacements() {
        for index in 0..32 {
            for call in quota::generate(size(256), index).calls {
                let (QuotaCall::Allocate { quantity } | QuotaCall::Refill { quantity }) = call;
                assert!((0..=51).contains(&quantity), "{call:?}");
            }
        }
    }
}

mod kiln_e2_23 {
    use super::*;

    #[test]
    fn batches_follow_the_frozen_draws() {
        let cases: [(u32, u32, i64, Vec<InventoryCall>); 3] = [
            (
                3,
                5,
                24,
                vec![
                    OrderLine { quantity: 5 },
                    OrderLine { quantity: 9 },
                    OrderLine { quantity: 5 },
                ],
            ),
            (
                5,
                6836,
                15,
                vec![
                    OrderLine { quantity: 1 },
                    OrderLine { quantity: 0 },
                    OrderLine { quantity: 3 },
                    StockCheck { threshold: 21 },
                    OrderLine { quantity: 4 },
                ],
            ),
            (
                4,
                29761,
                17,
                vec![
                    OrderLine { quantity: 7 },
                    OrderLine { quantity: 11 },
                    StockCheck { threshold: 9 },
                    OrderLine { quantity: 1 },
                ],
            ),
        ];
        for (n, index, quantity, calls) in cases {
            let batch = inventory::generate(size(n), index);
            assert_eq!(
                batch.initial,
                StockState { quantity },
                "size {n} index {index}"
            );
            assert_eq!(batch.calls, calls, "size {n} index {index}");
        }
    }

    #[test]
    fn arguments_are_valid_or_the_malformed_replacements() {
        for index in 0..32 {
            for call in inventory::generate(size(256), index).calls {
                match call {
                    OrderLine { quantity } => assert!((0..=11).contains(&quantity), "{call:?}"),
                    StockCheck { threshold } => assert!((9..=21).contains(&threshold), "{call:?}"),
                }
            }
        }
    }
}

mod kiln_e2_24 {
    use super::*;

    fn state(balances: [i64; 4], statuses: [Status; 4]) -> AccountsState {
        AccountsState {
            accounts: std::array::from_fn(|index| Account {
                balance: balances[index],
                status: statuses[index],
            }),
        }
    }

    #[test]
    fn batches_follow_the_frozen_draws() {
        use Status::{Frozen, Open};
        let first = accounts::generate(size(3), 5);
        assert_eq!(
            first.initial,
            state([7499, 9693, 25602, 2044], [Open, Frozen, Open, Open])
        );
        assert_eq!(
            first.calls,
            [
                Freeze { account: 3 },
                Transfer {
                    from: 3,
                    to: 1,
                    amount: 9400
                },
                Unfreeze { account: 3 }
            ]
        );

        let malformed = accounts::generate(size(5), 51864);
        assert_eq!(
            malformed.initial,
            state([45039, 42540, 11439, 37994], [Open, Open, Open, Frozen])
        );
        assert_eq!(
            malformed.calls,
            [
                Transfer {
                    from: 1,
                    to: 0,
                    amount: 0
                },
                Freeze { account: 4 },
                Transfer {
                    from: 3,
                    to: 2,
                    amount: 9847
                },
                Withdraw {
                    account: 0,
                    amount: 0
                },
                Deposit {
                    account: 3,
                    amount: 5773
                },
            ]
        );
    }

    #[test]
    fn money_operations_never_repeat_an_account_or_name_an_absent_one() {
        for index in 0..32 {
            for call in accounts::generate(size(256), index).calls {
                match call {
                    Deposit { account, amount } | Withdraw { account, amount } => {
                        assert!((0..=3).contains(&account), "{call:?}");
                        assert!((0..=10_000).contains(&amount), "{call:?}");
                    }
                    Transfer { from, to, amount } => {
                        assert!((0..=3).contains(&from) && (0..=3).contains(&to), "{call:?}");
                        assert_ne!(from, to, "{call:?}");
                        assert!((0..=10_000).contains(&amount), "{call:?}");
                    }
                    Freeze { account } | Unfreeze { account } => {
                        assert!((0..=4).contains(&account), "{call:?}");
                    }
                }
            }
        }
    }
}

mod kiln_e2_25 {
    use super::*;

    fn balances_of(store: &kiln_ir::Store) -> [i64; 4] {
        AccountsState::from_store(store)
            .accounts
            .map(|account| account.balance)
    }

    #[test]
    fn the_bound_is_the_initial_maximum_plus_two_hundred_fifty_six_maximal_deposits() {
        assert_eq!(BALANCE_BOUND, 2_610_000);
    }

    #[test]
    fn generated_batches_stay_within_the_bound_at_every_point_of_the_replay() {
        let program = accounts::program();
        for n in [1, 2, 4, 8, 16, 32, 64, 128, 256] {
            for index in 0..4 {
                let batch = accounts::generate(size(n), index);
                let executions = replay_all(&program, batch.initial.to_store(), &batch.calls);
                for store in std::iter::once(batch.initial.to_store())
                    .chain(executions.iter().map(|execution| execution.store.clone()))
                {
                    for balance in balances_of(&store) {
                        assert!(
                            (0..=BALANCE_BOUND).contains(&balance),
                            "size {n} index {index}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_worst_case_batch_reaches_the_bound_exactly() {
        let open = Account {
            balance: 50_000,
            status: Status::Open,
        };
        let deposits = [Deposit {
            account: 0,
            amount: 10_000,
        }; 256];
        let store = final_store(
            &accounts::program(),
            AccountsState {
                accounts: [open; 4],
            }
            .to_store(),
            &deposits,
        );
        assert_eq!(balances_of(&store)[0], BALANCE_BOUND);
    }
}

/// Each row is `(size, index, initial, final, committed calls, sum of
/// committed responses)`, derived by a separate model of the outcome tables
/// of inputs.md sections 2.2, 3.2 and 4.2 on the frozen generators' draws.
mod kiln_e2_26 {
    use super::*;

    fn committed_digest(executions: &[kiln_ir::Execution]) -> (usize, i64) {
        executions
            .iter()
            .filter_map(|execution| match &execution.outcome {
                Outcome::Committed {
                    response: Value::Integer(value),
                } => Some(*value),
                _ => None,
            })
            .fold((0, 0), |(count, sum), value| (count + 1, sum + value))
    }

    fn replayed(
        program: &kiln_ir::Program,
        store: kiln_ir::Store,
        calls: &[impl Copy + Into<Call>],
    ) -> Vec<kiln_ir::Execution> {
        replay_all(program, store, calls)
    }

    #[test]
    fn quota_batches_match_the_reference_model() {
        let rows = [
            (1, 0, 827, 778, (1, 49)),
            (16, 3, 111, 9, (12, 254)),
            (64, 1, 551, 0, (52, 1281)),
            (128, 2, 776, 14, (79, 2068)),
            (256, 0, 490, 82, (187, 4256)),
            (256, 7, 77, 0, (141, 3137)),
        ];
        for (n, index, initial, last, digest) in rows {
            let batch = quota::generate(size(n), index);
            assert_eq!(batch.initial.remaining, initial);
            let executions = replayed(&quota::program(), batch.initial.to_store(), &batch.calls);
            let end = QuotaState::from_store(&executions.last().unwrap().store);
            assert_eq!(
                (end.remaining, committed_digest(&executions)),
                (last, digest),
                "size {n} index {index}"
            );
        }
    }

    #[test]
    fn inventory_batches_match_the_reference_model() {
        let rows = [
            (1, 0, 50, 41, (1, 50)),
            (16, 3, 62, 70, (15, 702)),
            (64, 1, 17, 53, (62, 3165)),
            (128, 2, 78, 11, (127, 6816)),
            (256, 0, 84, 93, (253, 12263)),
            (256, 7, 16, 87, (252, 13499)),
        ];
        for (n, index, initial, last, digest) in rows {
            let batch = inventory::generate(size(n), index);
            assert_eq!(batch.initial.quantity, initial);
            let executions = replayed(
                &inventory::program(),
                batch.initial.to_store(),
                &batch.calls,
            );
            let end = StockState::from_store(&executions.last().unwrap().store);
            assert_eq!(
                (end.quantity, committed_digest(&executions)),
                (last, digest),
                "size {n} index {index}"
            );
        }
    }

    type Balances = [i64; 4];
    type FrozenBits = [i64; 4];
    type Snapshot = (Balances, FrozenBits);
    type AccountsRow = (u32, u32, Snapshot, Snapshot, (usize, i64));

    fn balances_and_frozen(state: AccountsState) -> Snapshot {
        (
            state.accounts.map(|account| account.balance),
            state.accounts.map(|account| account.status as i64),
        )
    }

    #[test]
    fn account_batches_match_the_reference_model() {
        let rows: [AccountsRow; 6] = [
            (
                1,
                0,
                ([48113, 30196, 17209, 25373], [1, 0, 0, 0]),
                ([48113, 39541, 7864, 25373], [1, 0, 0, 0]),
                (1, 7864),
            ),
            (
                16,
                3,
                ([32157, 48901, 38180, 6353], [0, 0, 0, 0]),
                ([8604, 32011, 57812, 7753], [0, 0, 0, 0]),
                (11, 339288),
            ),
            (
                64,
                1,
                ([36657, 6653, 20864, 14543], [1, 0, 0, 0]),
                ([36657, 8757, 10137, 14543], [1, 0, 0, 1]),
                (19, 247116),
            ),
            (
                128,
                2,
                ([28412, 3939, 26381, 37363], [1, 0, 1, 1]),
                ([56931, 11864, 26381, 54908], [0, 1, 0, 0]),
                (38, 1701350),
            ),
            (
                256,
                0,
                ([25216, 30390, 19851, 81], [0, 0, 0, 0]),
                ([47063, 37855, 8152, 17103], [0, 1, 1, 1]),
                (53, 1457162),
            ),
            (
                256,
                7,
                ([27823, 2710, 18159, 3328], [0, 1, 0, 0]),
                ([28010, 8140, 12921, 23470], [1, 0, 1, 1]),
                (60, 1147885),
            ),
        ];
        for (n, index, initial, last, digest) in rows {
            let batch = accounts::generate(size(n), index);
            assert_eq!(
                balances_and_frozen(batch.initial),
                initial,
                "size {n} index {index}"
            );
            let executions = replayed(&accounts::program(), batch.initial.to_store(), &batch.calls);
            let end = AccountsState::from_store(&executions.last().unwrap().store);
            assert_eq!(
                (balances_and_frozen(end), committed_digest(&executions)),
                (last, digest),
                "size {n} index {index}"
            );
        }
    }
}
