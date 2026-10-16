use kiln_e2::comparison::compare;
use kiln_e2::corpus::{prefix_corpus, PrefixCase, PROTOCOL_SIZES};
use kiln_e2::cost::measure;
use kiln_e2::quota::{
    self, serial_fold, Batch, QuotaCall, QuotaState, QuotaSummary, CEILING, QUANTITY_MAX,
};
use kiln_e2::scan::scan;
use kiln_e2::summary::{Summary, Uncounted};

fn quota_prefixes() -> impl Iterator<Item = PrefixCase<Batch>> {
    prefix_corpus(quota::generate, |batch: &mut Batch, size| {
        batch.calls.truncate(size)
    })
}

fn fold_of(calls: &[QuotaCall]) -> QuotaSummary {
    calls
        .iter()
        .map(|&call| QuotaSummary::atomic(call))
        .reduce(|earlier, later| earlier.combine(&later, &mut Uncounted))
        .expect("a segment has at least one call")
}

fn epsilon() -> QuotaSummary {
    QuotaSummary::atomic(QuotaCall::Allocate { quantity: 0 })
}

mod kiln_e2_31 {
    use super::*;

    /// Segment boundaries inside a 256-call batch: small and large steps, so
    /// the composites range from two calls to the whole batch.
    const CUTS: [usize; 14] = [0, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 256];

    fn assert_associative(first: QuotaSummary, second: QuotaSummary, third: QuotaSummary) {
        let left_grouped = first
            .combine(&second, &mut Uncounted)
            .combine(&third, &mut Uncounted);
        let right_grouped = first.combine(&second.combine(&third, &mut Uncounted), &mut Uncounted);
        assert_eq!(left_grouped, right_grouped);
    }

    #[test]
    fn combine_is_associative_on_values_over_atomic_triples_of_the_corpus_batches() {
        let mut triples = 0;
        for case in quota_prefixes().filter(|case| case.size == 256) {
            for window in case.batch.calls.windows(3) {
                let [first, second, third] = [0, 1, 2].map(|i| QuotaSummary::atomic(window[i]));
                assert_associative(first, second, third);
                triples += 1;
            }
        }
        assert_eq!(triples, 100 * 254);
    }

    #[test]
    fn combine_is_associative_on_values_over_composite_segments_of_generated_batches() {
        let mut triples = 0;
        for case in quota_prefixes().filter(|case| case.size == 256) {
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
}

mod kiln_e2_32 {
    use super::*;

    fn transformer_of(summary: &QuotaSummary) -> Vec<i64> {
        (0..=CEILING)
            .map(|remaining| {
                summary
                    .apply(QuotaState { remaining }, &mut Uncounted)
                    .remaining
            })
            .collect()
    }

    fn summaries_under_test() -> Vec<QuotaSummary> {
        let atomics = (1..=QUANTITY_MAX).flat_map(|quantity| {
            [
                QuotaSummary::atomic(QuotaCall::Allocate { quantity }),
                QuotaSummary::atomic(QuotaCall::Refill { quantity }),
            ]
        });
        let composites = quota_prefixes()
            .filter(|case| case.size == 256 && case.seed < 10)
            .flat_map(|case| [fold_of(&case.batch.calls[..7]), fold_of(&case.batch.calls)]);
        atomics.chain(composites).chain([epsilon()]).collect()
    }

    #[test]
    fn epsilon_is_an_identity_through_apply_on_the_whole_domain() {
        for summary in summaries_under_test() {
            let expected = transformer_of(&summary);
            let epsilon_first = epsilon().combine(&summary, &mut Uncounted);
            let epsilon_last = summary.combine(&epsilon(), &mut Uncounted);
            assert_eq!(transformer_of(&epsilon_first), expected, "{summary:?}");
            assert_eq!(transformer_of(&epsilon_last), expected, "{summary:?}");
        }
    }

    #[test]
    fn an_invalid_quantity_call_is_epsilon_whichever_kind_it_is() {
        let identity: Vec<i64> = (0..=CEILING).collect();
        for quantity in [i64::MIN, -1, 0, QUANTITY_MAX + 1, i64::MAX] {
            for call in [
                QuotaCall::Allocate { quantity },
                QuotaCall::Refill { quantity },
            ] {
                assert_eq!(transformer_of(&QuotaSummary::atomic(call)), identity);
            }
        }
    }
}

mod kiln_e2_33 {
    use super::*;

    #[test]
    fn the_scan_reproduces_every_response_and_the_final_state_of_the_serial_fold() {
        let mut prefixes = 0;
        for case in quota_prefixes() {
            let scanned = scan::<QuotaSummary>(case.batch.initial, &case.batch.calls);
            let outcome = compare(&serial_fold(&case.batch), &scanned);
            assert_eq!(
                outcome,
                Ok(()),
                "prefix {} of seed {}",
                case.size,
                case.seed
            );
            prefixes += 1;
        }
        assert_eq!(prefixes, 100 * PROTOCOL_SIZES.len());
    }

    #[test]
    fn the_scan_reproduces_partial_grants_partial_refills_and_both_declared_aborts() {
        let calls = [
            QuotaCall::Allocate { quantity: 30 },
            QuotaCall::Allocate { quantity: 30 },
            QuotaCall::Allocate { quantity: 5 },
            QuotaCall::Refill { quantity: 0 },
            QuotaCall::Refill { quantity: 50 },
            QuotaCall::Refill { quantity: 50 },
            QuotaCall::Allocate { quantity: 51 },
        ];
        for remaining in [0, 20, 40, 970, 1000] {
            let batch = Batch {
                initial: QuotaState { remaining },
                calls: calls.to_vec(),
            };
            let scanned = scan::<QuotaSummary>(batch.initial, &batch.calls);
            assert_eq!(
                compare(&serial_fold(&batch), &scanned),
                Ok(()),
                "{remaining}"
            );
        }
    }

    #[test]
    fn the_scan_of_no_calls_leaves_the_initial_state() {
        let initial = QuotaState { remaining: 17 };
        let scanned = scan::<QuotaSummary>(initial, &[]);
        assert!(scanned.outcomes.is_empty());
        assert_eq!(scanned.final_state, initial);
    }
}

mod kiln_e2_34 {
    use super::*;

    const COMBINE_OPERATIONS: u64 = 5;
    const APPLY_OPERATIONS: u64 = 2;
    const SUMMARY_BYTES: usize = 24;

    #[test]
    fn every_prefix_costs_the_frozen_bytes_and_operation_counts() {
        for case in quota_prefixes() {
            let cost = measure::<QuotaSummary>(case.batch.initial, &case.batch.calls)
                .expect("a prefix has calls");
            let label = format!("prefix {} of seed {}", case.size, case.seed);
            assert_eq!(cost.bytes, SUMMARY_BYTES, "{label}");
            assert_eq!(cost.apply_operations, APPLY_OPERATIONS, "{label}");
            let expected_combine = (case.size > 1).then_some(COMBINE_OPERATIONS);
            assert_eq!(cost.largest_combine_operations, expected_combine, "{label}");
        }
    }

    #[test]
    fn a_batch_of_odd_size_costs_the_same_as_one_of_even_size() {
        let calls = [QuotaCall::Refill { quantity: 3 }; 7];
        let cost = measure::<QuotaSummary>(QuotaState { remaining: 0 }, &calls)
            .expect("a batch with calls");
        assert_eq!(cost.largest_combine_operations, Some(COMBINE_OPERATIONS));
        assert_eq!(cost.apply_operations, APPLY_OPERATIONS);
    }

    #[test]
    fn the_report_gives_bytes_and_counts_per_protocol_size() {
        let report = quota::cost_report();
        let sizes: Vec<u32> = report.iter().map(|figures| figures.size).collect();
        assert_eq!(sizes, PROTOCOL_SIZES);
        for figures in report {
            assert_eq!(figures.bytes, SUMMARY_BYTES);
            assert_eq!(figures.apply_operations, APPLY_OPERATIONS);
            let expected_combine = (figures.size > 1).then_some(COMBINE_OPERATIONS);
            assert_eq!(figures.largest_combine_operations, expected_combine);
        }
    }

    /// A summary whose combine cost depends on its operands, so the fold's
    /// grouping and the largest-combine report are observable. A call is its
    /// weight; combining two summaries costs the difference of their total
    /// weights, applying one costs its call count, and its size is its call count.
    mod weighted {
        use kiln_e2::cost::{measure, Cost};
        use kiln_e2::run::CallOutcome;
        use kiln_e2::summary::{OperationCounter, Summary};

        struct Weighted {
            total_weight: u32,
            calls: usize,
        }

        impl Summary for Weighted {
            type Call = u32;
            type State = ();
            type Abort = ();

            fn atomic(weight: u32) -> Self {
                Self {
                    total_weight: weight,
                    calls: 1,
                }
            }

            fn combine<C: OperationCounter>(&self, later: &Self, counter: &mut C) -> Self {
                (0..self.total_weight.abs_diff(later.total_weight)).for_each(|_| counter.count());
                Self {
                    total_weight: self.total_weight + later.total_weight,
                    calls: self.calls + later.calls,
                }
            }

            fn apply<C: OperationCounter>(&self, state: (), counter: &mut C) {
                (0..self.calls).for_each(|_| counter.count());
                state
            }

            fn response(weight: u32, _pre_state: ()) -> CallOutcome<()> {
                CallOutcome::committed(i64::from(weight))
            }

            fn size_in_bytes(&self) -> usize {
                self.calls
            }
        }

        fn cost_of(weights: &[u32]) -> Cost {
            measure::<Weighted>((), weights).expect("a batch with calls")
        }

        /// Balanced combine costs per case: [5,5,10,0] gives 0,10,0 (a sequential
        /// fold gives a largest of 20; the first or last combine reports 0),
        /// [10,0,5,5] gives 10,0,0 (a last-combine report gives 0), [4,1,1] gives
        /// 3,4 (grouping from the right gives 0,2).
        #[test]
        fn the_largest_combine_of_the_balanced_pairwise_fold_is_reported() {
            let cases: [(&[u32], u64); 4] = [
                (&[5, 5, 10, 0], 10),
                (&[10, 0, 5, 5], 10),
                (&[4, 1, 1], 4),
                (&[1, 2, 3, 4, 5], 5),
            ];
            for (weights, expected) in cases {
                assert_eq!(
                    cost_of(weights).largest_combine_operations,
                    Some(expected),
                    "{weights:?}"
                );
            }
        }

        #[test]
        fn an_odd_trailing_summary_is_carried_into_the_final_summary() {
            for size in [3_u32, 5, 7] {
                let weights: Vec<u32> = (1..=size).collect();
                let cost = cost_of(&weights);
                assert_eq!(cost.bytes, size as usize, "{size}");
                assert_eq!(cost.apply_operations, u64::from(size), "{size}");
            }
        }

        #[test]
        fn a_single_call_has_no_combine() {
            let cost = cost_of(&[9]);
            assert_eq!(cost.largest_combine_operations, None);
            assert_eq!(cost.bytes, 1);
        }
    }
}
