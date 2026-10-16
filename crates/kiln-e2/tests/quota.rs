mod support;

use kiln_e2::quota::{
    program, QuotaCall,
    QuotaCall::{Allocate, Refill},
    QuotaState,
};
use support::{aborted, assert_rows, committed, final_store, outcomes_of};

fn state(remaining: i64) -> QuotaState {
    QuotaState { remaining }
}

/// A single call from `remaining`: its outcome and the remaining after it.
type Case = (i64, QuotaCall, kiln_ir::Outcome, i64);

fn assert_cases(cases: Vec<Case>) {
    for (before, call, outcome, after) in cases {
        assert_rows(
            &program(),
            state(before).to_store(),
            &[(call, outcome, state(after))],
            QuotaState::from_store,
        );
    }
}

mod kiln_e2_01 {
    use super::*;

    #[test]
    fn allocate_grants_in_full_partially_or_aborts_when_exhausted() {
        assert_cases(vec![
            (10, Allocate { quantity: 7 }, committed(7), 3),
            (7, Allocate { quantity: 7 }, committed(7), 0),
            (3, Allocate { quantity: 7 }, committed(3), 0),
            (1, Allocate { quantity: 50 }, committed(1), 0),
            (0, Allocate { quantity: 5 }, aborted("quota_exhausted"), 0),
            (1000, Allocate { quantity: 50 }, committed(50), 950),
        ]);
    }
}

mod kiln_e2_02 {
    use super::*;

    #[test]
    fn refill_adds_in_full_partially_or_aborts_at_the_ceiling() {
        assert_cases(vec![
            (0, Refill { quantity: 20 }, committed(20), 20),
            (950, Refill { quantity: 50 }, committed(50), 1000),
            (995, Refill { quantity: 50 }, committed(5), 1000),
            (999, Refill { quantity: 1 }, committed(1), 1000),
            (1000, Refill { quantity: 1 }, aborted("quota_full"), 1000),
        ]);
    }
}

mod kiln_e2_03 {
    use super::*;

    #[test]
    fn a_quantity_outside_one_to_fifty_aborts_before_the_quota_is_read() {
        let invalid = aborted("invalid_quantity");
        let mut cases: Vec<Case> = Vec::new();
        for remaining in [0, 20, 1000] {
            for quantity in [0, 51, -1, 1_000] {
                cases.push((remaining, Allocate { quantity }, invalid.clone(), remaining));
                cases.push((remaining, Refill { quantity }, invalid.clone(), remaining));
            }
        }
        assert_cases(cases);
    }

    #[test]
    fn the_quantity_limits_one_and_fifty_are_valid() {
        assert_cases(vec![
            (500, Allocate { quantity: 1 }, committed(1), 499),
            (500, Refill { quantity: 1 }, committed(1), 501),
            (500, Allocate { quantity: 50 }, committed(50), 450),
            (500, Refill { quantity: 50 }, committed(50), 550),
        ]);
    }
}

mod kiln_e2_04 {
    use super::*;

    #[test]
    fn q1_exhaustion_and_partial_grant() {
        assert_rows(
            &program(),
            state(10).to_store(),
            &[
                (Allocate { quantity: 7 }, committed(7), state(3)),
                (Allocate { quantity: 7 }, committed(3), state(0)),
                (
                    Allocate { quantity: 5 },
                    aborted("quota_exhausted"),
                    state(0),
                ),
                (Refill { quantity: 20 }, committed(20), state(20)),
                (
                    Allocate { quantity: 0 },
                    aborted("invalid_quantity"),
                    state(20),
                ),
            ],
            QuotaState::from_store,
        );
    }

    #[test]
    fn q2_the_ceiling() {
        assert_rows(
            &program(),
            state(990).to_store(),
            &[
                (Refill { quantity: 5 }, committed(5), state(995)),
                (Refill { quantity: 50 }, committed(5), state(1000)),
                (Refill { quantity: 1 }, aborted("quota_full"), state(1000)),
                (
                    Allocate { quantity: 51 },
                    aborted("invalid_quantity"),
                    state(1000),
                ),
                (Allocate { quantity: 50 }, committed(50), state(950)),
            ],
            QuotaState::from_store,
        );
    }

    #[test]
    fn q3_the_same_two_calls_in_two_orders_differ_in_responses_and_state() {
        let refill_then_allocate = [Refill { quantity: 5 }, Allocate { quantity: 7 }];
        let allocate_then_refill = [Allocate { quantity: 7 }, Refill { quantity: 5 }];
        let initial = state(998).to_store();

        assert_eq!(
            outcomes_of(&program(), initial.clone(), &refill_then_allocate),
            [committed(2), committed(7)]
        );
        assert_eq!(
            QuotaState::from_store(&final_store(
                &program(),
                initial.clone(),
                &refill_then_allocate
            )),
            state(993)
        );
        assert_eq!(
            outcomes_of(&program(), initial.clone(), &allocate_then_refill),
            [committed(7), committed(5)]
        );
        assert_eq!(
            QuotaState::from_store(&final_store(&program(), initial, &allocate_then_refill)),
            state(996)
        );
    }

    #[test]
    fn q4_the_escrow_contrast_grants_the_partial_remainder() {
        assert_rows(
            &program(),
            state(10).to_store(),
            &[
                (Allocate { quantity: 7 }, committed(7), state(3)),
                (Allocate { quantity: 7 }, committed(3), state(0)),
            ],
            QuotaState::from_store,
        );
    }
}
