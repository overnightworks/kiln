use kiln_e2::comparison::compare;
use kiln_e2::corpus::{prefix_corpus, PrefixCase, PROTOCOL_SIZES};
use kiln_e2::cost::measure;
use kiln_e2::inventory::{
    self, serial_fold, Batch, InventoryAbort,
    InventoryCall::{self, OrderLine, StockCheck},
    InventorySummary, StockState, ORDER_QUANTITY_MAX, RESTOCK_AMOUNT, STOCK_MAX, STOCK_MIN,
};
use kiln_e2::run::CallOutcome;
use kiln_e2::scan::scan;
use kiln_e2::summary::{OperationCount, Summary, Uncounted};

fn inventory_prefixes() -> impl Iterator<Item = PrefixCase<Batch>> {
    prefix_corpus(inventory::generate, |batch: &mut Batch, size| {
        batch.calls.truncate(size)
    })
}

fn fold_of(calls: &[InventoryCall]) -> InventorySummary {
    calls
        .iter()
        .map(|&call| InventorySummary::atomic(call))
        .reduce(|earlier, later| earlier.combine(&later, &mut Uncounted))
        .expect("a segment has at least one call")
}

fn epsilon() -> InventorySummary {
    InventorySummary::atomic(StockCheck { threshold: 15 })
}

/// The summary denoting a shift of `residue` mod 91, built from order lines
/// of quantity 1 so that it does not depend on the algebra under test beyond
/// `combine`.
fn shift_of(residue: i64) -> InventorySummary {
    (0..residue).fold(epsilon(), |shifted, _| {
        shifted.combine(
            &InventorySummary::atomic(OrderLine { quantity: 1 }),
            &mut Uncounted,
        )
    })
}

fn stock_domain() -> impl Iterator<Item = i64> {
    STOCK_MIN..=STOCK_MAX
}

fn transformer_of(summary: &InventorySummary) -> Vec<i64> {
    stock_domain()
        .map(|quantity| {
            summary
                .apply(StockState { quantity }, &mut Uncounted)
                .quantity
        })
        .collect()
}

/// `t ↦ (t - shift) mod 91` on `t = stock - 10`, written out from §3.3.
fn expected_transformer(shift: i64) -> Vec<i64> {
    stock_domain()
        .map(|stock| STOCK_MIN + (stock - STOCK_MIN - shift).rem_euclid(RESTOCK_AMOUNT))
        .collect()
}

mod kiln_e2_35 {
    use super::*;

    const CUTS: [usize; 14] = [0, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 256];

    fn assert_associative(
        first: InventorySummary,
        second: InventorySummary,
        third: InventorySummary,
    ) {
        let left_grouped = first
            .combine(&second, &mut Uncounted)
            .combine(&third, &mut Uncounted);
        let right_grouped = first.combine(&second.combine(&third, &mut Uncounted), &mut Uncounted);
        assert_eq!(left_grouped, right_grouped);
    }

    #[test]
    fn combine_is_associative_on_values_over_atomic_triples_of_the_corpus_batches() {
        let mut triples = 0;
        for case in inventory_prefixes().filter(|case| case.size == 256) {
            for window in case.batch.calls.windows(3) {
                let [first, second, third] = [0, 1, 2].map(|i| InventorySummary::atomic(window[i]));
                assert_associative(first, second, third);
                triples += 1;
            }
        }
        assert_eq!(triples, 100 * 254);
    }

    #[test]
    fn combine_is_associative_on_values_over_composite_segments_of_generated_batches() {
        let mut triples = 0;
        for case in inventory_prefixes().filter(|case| case.size == 256) {
            let calls = &case.batch.calls;
            let segment = |from, to| fold_of(&calls[CUTS[from]..CUTS[to]]);
            for a in 0..CUTS.len() {
                for b in a + 1..CUTS.len() {
                    for c in b + 1..CUTS.len() {
                        for d in c + 1..CUTS.len() {
                            assert_associative(segment(a, b), segment(b, c), segment(c, d));
                            triples += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(triples, 100 * 1001);
    }

    #[test]
    fn combine_is_associative_on_values_over_every_residue_triple() {
        for first in 0..RESTOCK_AMOUNT {
            for second in 0..RESTOCK_AMOUNT {
                for third in (0..RESTOCK_AMOUNT).step_by(7) {
                    assert_associative(shift_of(first), shift_of(second), shift_of(third));
                }
            }
        }
    }

    #[test]
    fn every_residue_applies_as_a_translation_mod_91_on_the_whole_stock_domain() {
        for shift in 0..RESTOCK_AMOUNT {
            assert_eq!(
                transformer_of(&shift_of(shift)),
                expected_transformer(shift),
                "{shift}"
            );
        }
    }

    #[test]
    fn combining_two_residues_composes_their_translations() {
        for first in 0..RESTOCK_AMOUNT {
            for second in 0..RESTOCK_AMOUNT {
                let combined = shift_of(first).combine(&shift_of(second), &mut Uncounted);
                assert_eq!(
                    transformer_of(&combined),
                    expected_transformer(first + second),
                    "{first} then {second}"
                );
            }
        }
    }

    fn summaries_under_test() -> Vec<InventorySummary> {
        let atomics = (1..=ORDER_QUANTITY_MAX)
            .map(|quantity| InventorySummary::atomic(OrderLine { quantity }));
        let composites = inventory_prefixes()
            .filter(|case| case.size == 256 && case.seed < 10)
            .flat_map(|case| [fold_of(&case.batch.calls[..7]), fold_of(&case.batch.calls)]);
        atomics.chain(composites).chain([epsilon()]).collect()
    }

    #[test]
    fn epsilon_is_an_identity_through_apply_on_the_whole_stock_domain() {
        for summary in summaries_under_test() {
            let expected = transformer_of(&summary);
            let epsilon_first = epsilon().combine(&summary, &mut Uncounted);
            let epsilon_last = summary.combine(&epsilon(), &mut Uncounted);
            assert_eq!(transformer_of(&epsilon_first), expected, "{summary:?}");
            assert_eq!(transformer_of(&epsilon_last), expected, "{summary:?}");
        }
        assert_eq!(
            transformer_of(&epsilon()),
            stock_domain().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_stock_check_and_every_malformed_call_is_epsilon() {
        let identity: Vec<i64> = stock_domain().collect();
        let calls = [
            StockCheck { threshold: 10 },
            StockCheck { threshold: 20 },
            StockCheck { threshold: 9 },
            StockCheck { threshold: 21 },
            OrderLine { quantity: 0 },
            OrderLine {
                quantity: ORDER_QUANTITY_MAX + 1,
            },
            OrderLine { quantity: i64::MIN },
            OrderLine { quantity: i64::MAX },
        ];
        for call in calls {
            assert_eq!(
                transformer_of(&InventorySummary::atomic(call)),
                identity,
                "{call:?}"
            );
        }
    }
}

mod kiln_e2_36 {
    use super::*;

    const COMBINE_OPERATIONS: u64 = 2;
    const APPLY_OPERATIONS: u64 = 3;
    const SUMMARY_BYTES: usize = 1;

    fn assert_matches_serial_fold(batch: &Batch, label: &str) {
        let scanned = scan::<InventorySummary>(batch.initial, &batch.calls);
        assert_eq!(compare(&serial_fold(batch), &scanned), Ok(()), "{label}");
    }

    #[test]
    fn the_scan_reproduces_every_response_and_the_final_state_of_the_serial_fold() {
        let mut prefixes = 0;
        for case in inventory_prefixes() {
            let label = format!("prefix {} of seed {}", case.size, case.seed);
            assert_matches_serial_fold(&case.batch, &label);
            prefixes += 1;
        }
        assert_eq!(prefixes, 100 * PROTOCOL_SIZES.len());
    }

    fn batch(quantity: i64, calls: &[InventoryCall]) -> Batch {
        Batch {
            initial: StockState { quantity },
            calls: calls.to_vec(),
        }
    }

    #[test]
    fn the_scan_reproduces_every_row_of_the_outcome_table_from_every_initial_stock() {
        let calls = [
            OrderLine { quantity: 10 },
            OrderLine { quantity: 1 },
            OrderLine { quantity: 0 },
            OrderLine { quantity: 11 },
            StockCheck { threshold: 10 },
            StockCheck { threshold: 20 },
            StockCheck { threshold: 9 },
            StockCheck { threshold: 21 },
            OrderLine { quantity: 7 },
            StockCheck { threshold: 15 },
        ];
        for quantity in stock_domain() {
            assert_matches_serial_fold(&batch(quantity, &calls), &quantity.to_string());
        }
    }

    #[test]
    fn the_scan_reproduces_worked_example_i1_with_both_argument_failures() {
        let calls = [
            OrderLine { quantity: 10 },
            OrderLine { quantity: 6 },
            StockCheck { threshold: 20 },
            OrderLine { quantity: 10 },
            OrderLine { quantity: 11 },
            StockCheck { threshold: 9 },
        ];
        let scanned = scan::<InventorySummary>(StockState { quantity: 25 }, &calls);
        let expected = [
            CallOutcome::committed(25),
            CallOutcome::committed(15),
            CallOutcome::committed(0),
            CallOutcome::committed(100),
            CallOutcome::declared(InventoryAbort::InvalidQuantity),
            CallOutcome::declared(InventoryAbort::InvalidThreshold),
        ];
        assert_eq!(scanned.outcomes, expected);
        assert_eq!(scanned.final_state.quantity, 90);
        assert_eq!(transformer_of(&fold_of(&calls)), expected_transformer(26));
    }

    #[test]
    fn the_scan_reproduces_worked_example_i2_at_the_exact_margin() {
        let calls = [
            OrderLine { quantity: 10 },
            StockCheck { threshold: 11 },
            OrderLine { quantity: 1 },
        ];
        let scanned = scan::<InventorySummary>(StockState { quantity: 20 }, &calls);
        let expected = [
            CallOutcome::committed(20),
            CallOutcome::committed(1),
            CallOutcome::committed(10),
        ];
        assert_eq!(scanned.outcomes, expected);
        assert_eq!(scanned.final_state.quantity, 100);
        assert_eq!(transformer_of(&fold_of(&calls)), expected_transformer(11));
    }

    #[test]
    fn the_scan_reproduces_worked_example_i3_with_order_dependent_responses() {
        let cases = [([5, 3], [15, 10]), ([3, 5], [15, 12])];
        for (quantities, responses) in cases {
            let calls = quantities.map(|quantity| OrderLine { quantity });
            let scanned = scan::<InventorySummary>(StockState { quantity: 15 }, &calls);
            assert_eq!(scanned.outcomes, responses.map(CallOutcome::committed));
            assert_eq!(scanned.final_state.quantity, 98);
            assert_eq!(transformer_of(&fold_of(&calls)), expected_transformer(8));
        }
    }

    #[test]
    fn a_stock_check_answers_from_the_prefix_stock_at_the_threshold_boundary() {
        let calls = [StockCheck { threshold: 10 }, StockCheck { threshold: 11 }];
        let scanned = scan::<InventorySummary>(StockState { quantity: 10 }, &calls);
        assert_eq!(
            scanned.outcomes,
            [CallOutcome::committed(0), CallOutcome::committed(1)]
        );
    }

    #[test]
    fn the_scan_of_no_calls_leaves_the_initial_state() {
        let initial = StockState { quantity: 42 };
        let scanned = scan::<InventorySummary>(initial, &[]);
        assert!(scanned.outcomes.is_empty());
        assert_eq!(scanned.final_state, initial);
    }

    #[test]
    fn every_prefix_costs_the_frozen_bytes_and_operation_counts() {
        for case in inventory_prefixes() {
            let cost = measure::<InventorySummary>(case.batch.initial, &case.batch.calls)
                .expect("a prefix has calls");
            let label = format!("prefix {} of seed {}", case.size, case.seed);
            assert_eq!(cost.bytes, SUMMARY_BYTES, "{label}");
            assert_eq!(cost.apply_operations, APPLY_OPERATIONS, "{label}");
            let expected_combine = (case.size > 1).then_some(COMBINE_OPERATIONS);
            assert_eq!(cost.largest_combine_operations, expected_combine, "{label}");
        }
    }

    #[test]
    fn combine_and_apply_count_the_conditional_step_whether_or_not_it_fires() {
        let wraps = shift_of(60).combine(&shift_of(60), &mut Uncounted);
        let stays = shift_of(1);
        let mut counter = OperationCount::default();
        shift_of(60).combine(&shift_of(60), &mut counter);
        assert_eq!(counter.total(), COMBINE_OPERATIONS);
        let mut counter = OperationCount::default();
        stays.combine(&stays, &mut counter);
        assert_eq!(counter.total(), COMBINE_OPERATIONS);
        for (summary, stock) in [(wraps, 10), (stays, 100), (epsilon(), 55)] {
            let mut counter = OperationCount::default();
            summary.apply(StockState { quantity: stock }, &mut counter);
            assert_eq!(counter.total(), APPLY_OPERATIONS, "{summary:?} at {stock}");
        }
    }

    #[test]
    fn the_cost_of_a_batch_does_not_grow_with_its_size() {
        for size in PROTOCOL_SIZES
            .iter()
            .map(|&size| size as usize)
            .chain([7, 100])
        {
            let calls = vec![OrderLine { quantity: 9 }; size];
            let cost = measure::<InventorySummary>(StockState { quantity: 30 }, &calls)
                .expect("a batch with calls");
            assert_eq!(cost.bytes, SUMMARY_BYTES, "{size}");
            assert_eq!(cost.apply_operations, APPLY_OPERATIONS, "{size}");
            let expected_combine = (size > 1).then_some(COMBINE_OPERATIONS);
            assert_eq!(cost.largest_combine_operations, expected_combine, "{size}");
        }
    }
}
