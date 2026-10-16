mod support;

use kiln_e2::inventory::{
    program, InventoryCall,
    InventoryCall::{OrderLine, StockCheck},
    StockState,
};
use support::{aborted, assert_rows, committed};

fn stock(quantity: i64) -> StockState {
    StockState { quantity }
}

/// A single call from a stock: its outcome and the stock after it.
type Case = (i64, InventoryCall, kiln_ir::Outcome, i64);

fn assert_cases(cases: Vec<Case>) {
    for (before, call, outcome, after) in cases {
        assert_rows(
            &program(),
            stock(before).to_store(),
            &[(call, outcome, stock(after))],
            StockState::from_store,
        );
    }
}

mod kiln_e2_05 {
    use super::*;

    #[test]
    fn order_line_returns_the_stock_found_and_restocks_below_the_margin() {
        assert_cases(vec![
            (25, OrderLine { quantity: 10 }, committed(25), 15),
            (15, OrderLine { quantity: 6 }, committed(15), 100),
            (100, OrderLine { quantity: 10 }, committed(100), 90),
            (20, OrderLine { quantity: 10 }, committed(20), 10),
            (10, OrderLine { quantity: 1 }, committed(10), 100),
            (19, OrderLine { quantity: 10 }, committed(19), 100),
            (11, OrderLine { quantity: 2 }, committed(11), 100),
            (12, OrderLine { quantity: 2 }, committed(12), 10),
            (100, OrderLine { quantity: 1 }, committed(100), 99),
        ]);
    }
}

mod kiln_e2_06 {
    use super::*;

    #[test]
    fn stock_check_answers_whether_the_stock_is_below_the_threshold() {
        assert_cases(vec![
            (100, StockCheck { threshold: 20 }, committed(0), 100),
            (10, StockCheck { threshold: 11 }, committed(1), 10),
            (10, StockCheck { threshold: 10 }, committed(0), 10),
            (19, StockCheck { threshold: 20 }, committed(1), 19),
            (20, StockCheck { threshold: 20 }, committed(0), 20),
            (21, StockCheck { threshold: 20 }, committed(0), 21),
        ]);
    }
}

mod kiln_e2_07 {
    use super::*;

    #[test]
    fn arguments_outside_their_ranges_abort_and_leave_the_stock() {
        let mut cases: Vec<Case> = Vec::new();
        for quantity in [0, 11, -1] {
            cases.push((50, OrderLine { quantity }, aborted("invalid_quantity"), 50));
        }
        for threshold in [9, 21, 0] {
            cases.push((
                50,
                StockCheck { threshold },
                aborted("invalid_threshold"),
                50,
            ));
        }
        assert_cases(cases);
    }
}

mod kiln_e2_08 {
    use super::*;

    #[test]
    fn i1_both_branches_a_read_only_check_and_both_argument_failures() {
        assert_rows(
            &program(),
            stock(25).to_store(),
            &[
                (OrderLine { quantity: 10 }, committed(25), stock(15)),
                (OrderLine { quantity: 6 }, committed(15), stock(100)),
                (StockCheck { threshold: 20 }, committed(0), stock(100)),
                (OrderLine { quantity: 10 }, committed(100), stock(90)),
                (
                    OrderLine { quantity: 11 },
                    aborted("invalid_quantity"),
                    stock(90),
                ),
                (
                    StockCheck { threshold: 9 },
                    aborted("invalid_threshold"),
                    stock(90),
                ),
            ],
            StockState::from_store,
        );
    }

    #[test]
    fn i2_the_exact_margin() {
        assert_rows(
            &program(),
            stock(20).to_store(),
            &[
                (OrderLine { quantity: 10 }, committed(20), stock(10)),
                (StockCheck { threshold: 11 }, committed(1), stock(10)),
                (OrderLine { quantity: 1 }, committed(10), stock(100)),
            ],
            StockState::from_store,
        );
    }

    #[test]
    fn i3_responses_depend_on_the_order_and_the_state_does_not() {
        let initial = stock(15).to_store();
        assert_rows(
            &program(),
            initial.clone(),
            &[
                (OrderLine { quantity: 5 }, committed(15), stock(10)),
                (OrderLine { quantity: 3 }, committed(10), stock(98)),
            ],
            StockState::from_store,
        );
        assert_rows(
            &program(),
            initial,
            &[
                (OrderLine { quantity: 3 }, committed(15), stock(12)),
                (OrderLine { quantity: 5 }, committed(12), stock(98)),
            ],
            StockState::from_store,
        );
    }
}
