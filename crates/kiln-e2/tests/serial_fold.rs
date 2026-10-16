use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::LazyLock;

use kiln_e2::accounts::{
    self, Account, AccountCall, AccountsAbort, AccountsState,
    Status::{Frozen, Open},
};
use kiln_e2::comparison::{compare, CallDifference, Divergence};
use kiln_e2::inventory::{self, InventoryAbort, InventoryCall, StockState};
use kiln_e2::oracle::{oracle_run, FamilyBatch};
use kiln_e2::quota::{self, QuotaAbort, QuotaCall, QuotaState};
use kiln_e2::run::{Abort, CallOutcome, Run};
use kiln_e2::BatchSize;
use kiln_ir::{AbortName, AbortReason, Call, Key, Outcome, TableName, Value};

/// The protocol's batch sizes and seeds (protocol.md section 2).
const SIZES: [u32; 9] = [1, 2, 4, 8, 16, 32, 64, 128, 256];
const SEEDS: u32 = 100;

type Fold<B> = fn(&B) -> Run<<B as FamilyBatch>::Abort, <B as FamilyBatch>::State>;
type RowOf<A> = fn(&Call, &CallOutcome<A>) -> String;

/// The first divergence between a fold and the oracle on one batch.
fn divergence_of<B>(batch: &B, fold: Fold<B>) -> Option<String>
where
    B: FamilyBatch,
    B::Abort: PartialEq + Clone + Debug,
    B::State: PartialEq + Clone + Debug,
{
    let oracle = oracle_run(batch).expect("the oracle's schema is valid");
    compare(&oracle, &fold(batch))
        .err()
        .map(|divergence| format!("{divergence:?}"))
}

fn assert_fold_equals_oracle<B>(batch: &B, fold: Fold<B>)
where
    B: FamilyBatch,
    B::Abort: PartialEq + Clone + Debug,
    B::State: PartialEq + Clone + Debug,
{
    assert_eq!(divergence_of(batch, fold), None);
}

/// A family's generated batches checked against its fold, and the outcome
/// rows the prefix corpus exercised.
struct Sweep {
    family: &'static str,
    expected_rows: &'static [&'static str],
    divergences: Vec<String>,
    rows_seen: BTreeMap<String, usize>,
}

/// How a family's batch is cut to its first calls, keeping its initial state.
type Truncate<B> = fn(&mut B, usize);

/// The protocol's paired corpus: for each seed, the prefixes of
/// `generate(256, seed)` at every protocol size (protocol.md section 2).
fn prefix_corpus<B: Clone>(
    generate: fn(BatchSize, u32) -> B,
    truncate: Truncate<B>,
) -> impl Iterator<Item = (String, B)> {
    let largest = BatchSize::new(*SIZES.last().expect("sizes")).expect("a protocol size");
    (0..SEEDS).flat_map(move |seed| {
        let full = generate(largest, seed);
        SIZES.into_iter().map(move |size| {
            let mut prefix = full.clone();
            truncate(&mut prefix, size as usize);
            (format!("prefix {size} of seed {seed}"), prefix)
        })
    })
}

/// Batches generated independently at each size, as additional evidence.
fn independent_corpus<B>(generate: fn(BatchSize, u32) -> B) -> impl Iterator<Item = (String, B)> {
    SIZES.into_iter().flat_map(move |size| {
        let batch_size = BatchSize::new(size).expect("a protocol size");
        (0..SEEDS).map(move |seed| {
            (
                format!("independent size {size} seed {seed}"),
                generate(batch_size, seed),
            )
        })
    })
}

fn sweep<B>(
    family: &'static str,
    expected_rows: &'static [&'static str],
    generate: fn(BatchSize, u32) -> B,
    truncate: Truncate<B>,
    fold: Fold<B>,
    row_of: RowOf<B::Abort>,
) -> Sweep
where
    B: FamilyBatch + Clone,
    B::Abort: PartialEq + Clone + Debug,
    B::State: PartialEq + Clone + Debug,
{
    let mut sweep = Sweep {
        family,
        expected_rows,
        divergences: Vec::new(),
        rows_seen: BTreeMap::new(),
    };
    for (label, batch) in prefix_corpus(generate, truncate) {
        let oracle = oracle_run(&batch).expect("the oracle's schema is valid");
        for (call, outcome) in batch.calls().iter().zip(&oracle.outcomes) {
            *sweep.rows_seen.entry(row_of(call, outcome)).or_default() += 1;
        }
        if let Some(divergence) = divergence_of(&batch, fold) {
            sweep.divergences.push(format!("{label}: {divergence}"));
        }
    }
    for (label, batch) in independent_corpus(generate) {
        if let Some(divergence) = divergence_of(&batch, fold) {
            sweep.divergences.push(format!("{label}: {divergence}"));
        }
    }
    sweep
}

fn integer_arguments(call: &Call) -> Vec<i64> {
    call.args
        .iter()
        .map(|argument| match argument {
            Value::Integer(value) => *value,
            other => panic!("a non-integer argument {other:?}"),
        })
        .collect()
}

fn committed_response<A>(outcome: &CallOutcome<A>) -> Option<i64> {
    match outcome {
        CallOutcome::Committed(Value::Integer(response)) => Some(*response),
        _ => None,
    }
}

fn plain_row<A: Debug>(call: &Call, outcome: &CallOutcome<A>) -> String {
    let name = &call.function.0;
    match outcome {
        CallOutcome::Committed(_) => format!("{name} commits"),
        CallOutcome::Aborted(Abort::Declared(abort)) => format!("{name} aborts {abort:?}"),
        CallOutcome::Aborted(Abort::Structural(reason)) => panic!("{name} aborts {reason:?}"),
    }
}

fn quota_row(call: &Call, outcome: &CallOutcome<QuotaAbort>) -> String {
    match committed_response(outcome) {
        Some(moved) if moved < integer_arguments(call)[0] => {
            format!("{} commits partially", call.function.0)
        }
        _ => plain_row(call, outcome),
    }
}

fn inventory_row(call: &Call, outcome: &CallOutcome<InventoryAbort>) -> String {
    let name = &call.function.0;
    let arguments = integer_arguments(call);
    match (name.as_str(), committed_response(outcome)) {
        ("order_line", Some(stock)) if stock - arguments[0] < inventory::STOCK_MIN => {
            "order_line restocks".to_owned()
        }
        ("stock_check", Some(answer)) => format!("stock_check answers {answer}"),
        _ => plain_row(call, outcome),
    }
}

const QUOTA_ROWS: &[&str] = &[
    "allocate commits",
    "allocate commits partially",
    "allocate aborts InvalidQuantity",
    "allocate aborts QuotaExhausted",
    "refill commits",
    "refill commits partially",
    "refill aborts InvalidQuantity",
    "refill aborts QuotaFull",
];

const INVENTORY_ROWS: &[&str] = &[
    "order_line commits",
    "order_line restocks",
    "order_line aborts InvalidQuantity",
    "stock_check answers 0",
    "stock_check answers 1",
    "stock_check aborts InvalidThreshold",
];

const ACCOUNTS_ROWS: &[&str] = &[
    "deposit commits",
    "deposit aborts InvalidAmount",
    "deposit aborts AccountFrozen",
    "withdraw commits",
    "withdraw aborts InvalidAmount",
    "withdraw aborts AccountFrozen",
    "withdraw aborts InsufficientFunds",
    "transfer commits",
    "transfer aborts InvalidAmount",
    "transfer aborts AccountFrozen",
    "transfer aborts InsufficientFunds",
    "freeze commits",
    "freeze aborts AccountNotFound",
    "freeze aborts AlreadyFrozen",
    "unfreeze commits",
    "unfreeze aborts AccountNotFound",
    "unfreeze aborts NotFrozen",
];

static SWEEPS: LazyLock<Vec<Sweep>> = LazyLock::new(|| {
    vec![
        sweep(
            "quota",
            QUOTA_ROWS,
            quota::generate,
            |batch, length| batch.calls.truncate(length),
            quota::serial_fold,
            quota_row,
        ),
        sweep(
            "inventory",
            INVENTORY_ROWS,
            inventory::generate,
            |batch, length| batch.calls.truncate(length),
            inventory::serial_fold,
            inventory_row,
        ),
        sweep(
            "accounts",
            ACCOUNTS_ROWS,
            accounts::generate,
            |batch, length| batch.calls.truncate(length),
            accounts::serial_fold,
            plain_row,
        ),
    ]
});

mod kiln_e2_27 {
    use super::*;

    #[test]
    fn every_fold_equals_the_oracle_on_the_generated_batches() {
        for sweep in SWEEPS.iter() {
            assert!(
                sweep.divergences.is_empty(),
                "{}: {:#?}",
                sweep.family,
                sweep.divergences
            );
        }
    }

    #[test]
    fn the_quota_fold_equals_the_oracle_beyond_the_generators_ranges() {
        let calls = [-3, 0, 51, i64::from(i32::MAX), 7, 50]
            .into_iter()
            .flat_map(|quantity| {
                [
                    QuotaCall::Allocate { quantity },
                    QuotaCall::Refill { quantity },
                ]
            })
            .collect();
        for remaining in [0, 1, 999, 1000] {
            let batch = quota::Batch {
                initial: QuotaState { remaining },
                calls: Vec::clone(&calls),
            };
            assert_fold_equals_oracle(&batch, quota::serial_fold);
        }
    }

    #[test]
    fn the_inventory_fold_equals_the_oracle_beyond_the_generators_ranges() {
        let calls = [-3, 0, 9, 10, 11, 20, 21, i64::from(i16::MAX)]
            .into_iter()
            .flat_map(|value| {
                [
                    InventoryCall::OrderLine { quantity: value },
                    InventoryCall::StockCheck { threshold: value },
                ]
            })
            .collect();
        for quantity in [10, 11, 19, 20, 100] {
            let batch = inventory::Batch {
                initial: StockState { quantity },
                calls: Vec::clone(&calls),
            };
            assert_fold_equals_oracle(&batch, inventory::serial_fold);
        }
    }

    #[test]
    fn the_accounts_fold_equals_the_oracle_on_calls_the_generator_never_draws() {
        let account = |balance, status| Account { balance, status };
        let initial = AccountsState {
            accounts: [
                account(100, Open),
                account(50, Frozen),
                account(0, Open),
                account(7, Open),
            ],
        };
        let transfer = |from, to, amount| AccountCall::Transfer { from, to, amount };
        let calls = vec![
            transfer(0, 0, 5),
            transfer(5, 5, 5),
            transfer(0, 0, 0),
            transfer(0, 9, 5),
            transfer(9, 0, 5),
            transfer(9, 8, 5),
            transfer(1, 0, 5),
            transfer(0, 1, 5),
            transfer(1, 9, 5),
            transfer(2, 3, 1),
            transfer(0, 2, 100),
            AccountCall::Withdraw {
                account: 1,
                amount: 1_000,
            },
            AccountCall::Withdraw {
                account: 3,
                amount: 8,
            },
            AccountCall::Withdraw {
                account: 3,
                amount: 7,
            },
            AccountCall::Deposit {
                account: -1,
                amount: 5,
            },
            AccountCall::Deposit {
                account: 0,
                amount: 10_001,
            },
            AccountCall::Freeze { account: -1 },
            AccountCall::Withdraw {
                account: 9,
                amount: 5,
            },
            AccountCall::Unfreeze { account: -1 },
            AccountCall::Unfreeze { account: 0 },
            AccountCall::Unfreeze { account: 1 },
            AccountCall::Freeze { account: 1 },
            AccountCall::Freeze { account: 1 },
        ];
        assert_fold_equals_oracle(&accounts::Batch { initial, calls }, accounts::serial_fold);
    }

    #[test]
    fn an_absent_account_aborts_every_account_call_and_leaves_the_state_as_it_was() {
        let open = Account {
            balance: 10,
            status: Open,
        };
        let initial = AccountsState {
            accounts: [open; 4],
        };
        let absent = [-1, 4, 9];
        let calls = absent.into_iter().flat_map(|account| {
            [
                AccountCall::Deposit { account, amount: 5 },
                AccountCall::Withdraw { account, amount: 5 },
                AccountCall::Freeze { account },
                AccountCall::Unfreeze { account },
            ]
        });
        for call in calls {
            let batch = accounts::Batch {
                initial,
                calls: vec![call],
            };
            let oracle = oracle_run(&batch).expect("the oracle's schema is valid");
            let fold = accounts::serial_fold(&batch);
            for run in [oracle, fold] {
                assert_eq!(
                    run.outcomes,
                    vec![CallOutcome::declared(AccountsAbort::AccountNotFound)],
                    "{call:?}"
                );
                assert_eq!(run.final_state, initial, "{call:?}");
            }
        }
    }
}

mod kiln_e2_28 {
    use super::*;

    #[test]
    fn the_generated_batches_exercise_every_outcome_row_of_every_family() {
        for sweep in SWEEPS.iter() {
            let unseen: Vec<_> = sweep
                .expected_rows
                .iter()
                .filter(|row| !sweep.rows_seen.contains_key(**row))
                .collect();
            assert!(unseen.is_empty(), "{}: never seen {unseen:?}", sweep.family);
        }
    }

    #[test]
    fn the_generated_batches_produce_no_row_outside_the_frozen_tables() {
        for sweep in SWEEPS.iter() {
            let unexpected: Vec<_> = sweep
                .rows_seen
                .keys()
                .filter(|row| !sweep.expected_rows.contains(&row.as_str()))
                .collect();
            assert!(
                unexpected.is_empty(),
                "{}: unexpected {unexpected:?}",
                sweep.family
            );
        }
    }
}

mod kiln_e2_29 {
    use super::*;

    type Outcomes = CallOutcome<QuotaAbort>;
    type QuotaRun = Run<QuotaAbort, QuotaState>;

    fn run_of(outcomes: Vec<Outcomes>, remaining: i64) -> QuotaRun {
        Run {
            outcomes,
            final_state: QuotaState { remaining },
        }
    }

    fn committed(response: i64) -> Outcomes {
        CallOutcome::committed(response)
    }

    fn aborted(abort: QuotaAbort) -> Outcomes {
        CallOutcome::declared(abort)
    }

    fn structural(table: &str) -> Outcomes {
        CallOutcome::Aborted(Abort::Structural(AbortReason::MissingRow {
            table: TableName(table.to_owned()),
            key: Key(vec![Value::Integer(0)]),
        }))
    }

    fn call_divergence(
        index: usize,
        difference: CallDifference,
        expected: Outcomes,
        actual: Outcomes,
    ) -> Divergence<QuotaAbort, QuotaState> {
        Divergence::Call {
            index,
            difference,
            expected: Box::new(expected),
            actual: Box::new(actual),
        }
    }

    #[test]
    fn equal_runs_do_not_diverge() {
        let run = run_of(vec![committed(1), aborted(QuotaAbort::QuotaFull)], 5);
        assert_eq!(compare(&run, &run.clone()), Ok(()));
    }

    #[test]
    fn the_first_diverging_call_is_reported_with_what_differs_and_both_outcomes() {
        use CallDifference::{AbortPayload, AbortReason, OutcomeKind, Response};
        let declared = |abort| aborted(abort);
        let cases = [
            (committed(3), declared(QuotaAbort::QuotaFull), OutcomeKind),
            (declared(QuotaAbort::QuotaFull), committed(3), OutcomeKind),
            (committed(3), committed(4), Response),
            (
                declared(QuotaAbort::QuotaFull),
                declared(QuotaAbort::QuotaExhausted),
                AbortReason,
            ),
            (
                declared(QuotaAbort::QuotaFull),
                structural("quota"),
                AbortReason,
            ),
            (
                structural("quota"),
                declared(QuotaAbort::QuotaFull),
                AbortReason,
            ),
            (structural("quota"), structural("stock"), AbortPayload),
            (structural("quota"), committed(1), OutcomeKind),
        ];
        for (expected, actual, difference) in cases {
            let want = run_of(vec![committed(9), expected.clone(), committed(8)], 5);
            let got = run_of(vec![committed(9), actual.clone(), committed(7)], 6);
            assert_eq!(
                compare(&want, &got),
                Err(call_divergence(1, difference, expected, actual))
            );
        }
    }

    #[test]
    fn unequal_call_counts_are_reported_after_the_common_calls_agree() {
        let short = run_of(vec![committed(1)], 5);
        let long = run_of(vec![committed(1), committed(2)], 5);
        assert_eq!(
            compare(&short, &long),
            Err(Divergence::CallCount {
                expected: 1,
                actual: 2
            })
        );
        assert_eq!(
            compare(&long, &short),
            Err(Divergence::CallCount {
                expected: 2,
                actual: 1
            })
        );
    }

    #[test]
    fn a_differing_final_state_is_reported_when_every_call_agrees() {
        let want = run_of(vec![committed(1)], 5);
        let got = run_of(vec![committed(1)], 6);
        assert_eq!(
            compare(&want, &got),
            Err(Divergence::FinalState {
                expected: QuotaState { remaining: 5 },
                actual: QuotaState { remaining: 6 },
            })
        );
    }

    #[test]
    fn a_differing_call_is_reported_before_a_differing_final_state() {
        let want = run_of(vec![committed(1)], 5);
        let got = run_of(vec![committed(2)], 6);
        assert_eq!(
            compare(&want, &got),
            Err(call_divergence(
                0,
                CallDifference::Response,
                committed(1),
                committed(2)
            ))
        );
    }

    #[test]
    fn an_oracle_outcome_converts_to_the_typed_shape_and_keeps_what_the_family_does_not_declare() {
        let declared = |name: &str| Outcome::Aborted {
            reason: AbortReason::Declared(AbortName(name.to_owned())),
        };
        let missing_row = AbortReason::MissingRow {
            table: TableName("quota".to_owned()),
            key: Key(vec![Value::Integer(0)]),
        };
        let cases = [
            (
                Outcome::Committed {
                    response: Value::Integer(4),
                },
                committed(4),
            ),
            (declared("quota_full"), aborted(QuotaAbort::QuotaFull)),
            (
                declared("no_such_abort"),
                CallOutcome::Aborted(Abort::Structural(AbortReason::Declared(AbortName(
                    "no_such_abort".to_owned(),
                )))),
            ),
            (
                Outcome::Aborted {
                    reason: missing_row.clone(),
                },
                CallOutcome::Aborted(Abort::Structural(missing_row)),
            ),
        ];
        for (outcome, expected) in cases {
            assert_eq!(CallOutcome::<QuotaAbort>::from(&outcome), expected);
        }
    }
}

mod kiln_e2_30 {
    use super::*;

    /// The honest fold, one call at a time, except that a refill is
    /// allowed to fill the quota one above its ceiling.
    fn fold_with_the_ceiling_one_too_high(batch: &quota::Batch) -> Run<QuotaAbort, QuotaState> {
        fn step(state: QuotaState, call: QuotaCall) -> (QuotaState, CallOutcome<QuotaAbort>) {
            if let QuotaCall::Refill { quantity } = call {
                let valid = (1..=quota::QUANTITY_MAX).contains(&quantity);
                if valid && state.remaining < quota::CEILING {
                    let added = quantity.min(quota::CEILING + 1 - state.remaining);
                    let after = QuotaState {
                        remaining: state.remaining + added,
                    };
                    return (after, CallOutcome::committed(added));
                }
            }
            let mut honest = quota::serial_fold(&quota::Batch {
                initial: state,
                calls: vec![call],
            });
            (honest.final_state, honest.outcomes.remove(0))
        }
        Run::fold(batch.initial, &batch.calls, step)
    }

    #[test]
    fn a_ceiling_one_too_high_is_reported_at_the_first_refill_reaching_it() {
        let batch = quota::Batch {
            initial: QuotaState { remaining: 990 },
            calls: vec![
                QuotaCall::Allocate { quantity: 2 },
                QuotaCall::Refill { quantity: 5 },
                QuotaCall::Refill { quantity: 50 },
                QuotaCall::Refill { quantity: 50 },
            ],
        };
        let oracle = oracle_run(&batch).expect("the oracle's schema is valid");
        assert_eq!(
            compare(&oracle, &fold_with_the_ceiling_one_too_high(&batch)),
            Err(Divergence::Call {
                index: 2,
                difference: CallDifference::Response,
                expected: Box::new(CallOutcome::committed(7)),
                actual: Box::new(CallOutcome::committed(8)),
            })
        );
    }
}
