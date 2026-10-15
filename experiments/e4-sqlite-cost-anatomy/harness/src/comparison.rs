//! The pilot's instrumented/passthrough overhead comparison: protocol §5's
//! "How the bound is judged" (as amended in #30), which §5 assigns to the
//! pilot (§13). Only cells whose timed transaction calls `xWrite` or
//! `xSync` are gated; each gated cell runs
//! [`machine::REPETITIONS_PER_CELL`] counterbalanced pairs and is judged
//! pass, fail, or inconclusive from their overhead ratios. A read-only
//! cell is reported as not gated, never as pass or fail.
//!
//! The reductions here are pure and tested without a process; the pair
//! runs that feed them are [`crate::pilot_run`]'s.

use crate::builds::Build;
use crate::driver;
use crate::machine;
use crate::workloads::Cell;

/// Protocol §5's frozen overhead-acceptance bound: the instrumented
/// build's mean per-operation wall time must not exceed the passthrough
/// build's by more than this fraction.
pub const OVERHEAD_BOUND: f64 = 0.02;

/// Protocol §5's frozen counterbalanced order, fixed per pair index in
/// advance: pair `k` (1-based) runs the instrumented build first when `k`
/// is odd and the passthrough build first when `k` is even. Takes the
/// driver's 0-based repetition index, so index 0 is pair 1.
pub fn first_build(repetition_index: usize) -> Build {
    if repetition_index.is_multiple_of(2) {
        Build::Instrumented
    } else {
        Build::Passthrough
    }
}

/// `(instrumented - passthrough) / passthrough` — positive when the
/// instrumented build costs more, as the timer calls it carries always
/// should. `f64::INFINITY` for a non-positive passthrough mean: a zero or
/// negative baseline makes the ratio meaningless, and silently returning 0
/// would hide that instead of failing the bound check.
pub fn overhead_fraction(instrumented_mean_ns: f64, passthrough_mean_ns: f64) -> f64 {
    if passthrough_mean_ns <= 0.0 {
        return f64::INFINITY;
    }
    (instrumented_mean_ns - passthrough_mean_ns) / passthrough_mean_ns
}

/// Protocol §5's three mutually exclusive outcomes for a gated cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Inconclusive,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Inconclusive => "INCONCLUSIVE",
        }
    }
}

/// The judged statistic of protocol §5: the median of the pairs' overhead
/// ratios with their min-max range, and the three-way outcome that range
/// implies against [`OVERHEAD_BOUND`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverheadSummary {
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub verdict: Verdict,
}

/// Judges a cell's overhead ratios; `None` for no ratios at all. Pass when
/// the range's max is below the bound, fail when its min is above it,
/// inconclusive when the range straddles it (`min <= bound <= max`).
pub fn summarize(ratios: &[f64]) -> Option<OverheadSummary> {
    if ratios.is_empty() {
        return None;
    }
    let mut sorted = ratios.to_vec();
    sorted.sort_by(f64::total_cmp);
    let (min, max) = (sorted[0], sorted[sorted.len() - 1]);
    let middle = sorted.len() / 2;
    let median = if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    };
    let verdict = if max < OVERHEAD_BOUND {
        Verdict::Pass
    } else if min > OVERHEAD_BOUND {
        Verdict::Fail
    } else {
        Verdict::Inconclusive
    };
    Some(OverheadSummary {
        median,
        min,
        max,
        verdict,
    })
}

/// One comparison attempt's accepted measurement (protocol §13's
/// mechanics, never a verdict quantity): both builds' mean wall time for
/// the same workload, seed, and operation count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComparisonMeasurement {
    pub operations_measured: usize,
    pub instrumented_mean_wall_ns: f64,
    pub passthrough_mean_wall_ns: f64,
}

/// One reported pair, with the pair index (1-based, as §5 numbers them)
/// and the build that ran first (§11 requires both).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComparisonPair {
    pub pair_index: usize,
    pub first_build: Build,
    pub operations_measured: usize,
    pub instrumented_mean_wall_ns: f64,
    pub passthrough_mean_wall_ns: f64,
    pub overhead_fraction: f64,
}

/// What the pilot reports for one workload.
#[derive(Debug, Clone, PartialEq)]
pub enum CellComparison {
    /// A read-only cell: nothing runs, nothing is judged (protocol §5:
    /// satisfied by construction).
    NotGated,
    /// Every pair completed; `summary` is their three-way judgement.
    Judged {
        pairs: Vec<ComparisonPair>,
        summary: OverheadSummary,
    },
    /// A pair exhausted its retry budget (protocol §5 repeat policy, §9):
    /// "inconclusive — insufficient repetitions". The pairs that did
    /// complete stay, each with its index and starting build (§11).
    InsufficientPairs {
        pairs: Vec<ComparisonPair>,
        required: usize,
    },
}

impl CellComparison {
    pub fn is_not_gated(&self) -> bool {
        matches!(self, CellComparison::NotGated)
    }

    /// Whether this cell lets the pilot succeed: a not-gated cell does not
    /// vote, a judged cell must pass, an insufficient one never does.
    pub fn allows_pilot_success(&self) -> bool {
        match self {
            CellComparison::NotGated => true,
            CellComparison::Judged { summary, .. } => summary.verdict == Verdict::Pass,
            CellComparison::InsufficientPairs { .. } => false,
        }
    }
}

/// One cell's row in the pilot report.
#[derive(Debug, Clone, PartialEq)]
pub struct CellRow {
    pub cell: Cell,
    pub comparison: CellComparison,
}

impl CellRow {
    /// The report lines for this cell.
    pub fn render(&self) -> String {
        let label = self.cell.label();
        match &self.comparison {
            CellComparison::NotGated => format!(
                "  {label}: not gated — its read-only transaction never reaches the timed VFS \
                 calls, so the bound holds by construction (§5); no comparison ran\n"
            ),
            CellComparison::InsufficientPairs { pairs, required } => format!(
                "  {label}: inconclusive — insufficient repetitions ({}/{required} pairs succeeded)\n{}",
                pairs.len(),
                render_pairs(pairs),
            ),
            CellComparison::Judged { pairs, summary } => format!(
                "  {label}: gated, {} pairs\n{}    median={:.2}% range=[{:.2}%, {:.2}%] -> {}\n",
                pairs.len(),
                render_pairs(pairs),
                summary.median * 100.0,
                summary.min * 100.0,
                summary.max * 100.0,
                summary.verdict.label(),
            ),
        }
    }
}

/// One line per pair, with its index and the build that ran first (§11).
fn render_pairs(pairs: &[ComparisonPair]) -> String {
    pairs
        .iter()
        .map(|pair| {
            format!(
                "    pair={:<2} first={:<12} ops={:<6} instrumented_mean_ns={:>10.1} passthrough_mean_ns={:>10.1} overhead={:>6.2}%\n",
                pair.pair_index,
                pair.first_build.label(),
                pair.operations_measured,
                pair.instrumented_mean_wall_ns,
                pair.passthrough_mean_wall_ns,
                pair.overhead_fraction * 100.0,
            )
        })
        .collect()
}

/// The reported pair for one completed repetition: the repetition's
/// 0-based index gives the pair index (1-based, §5) and the counterbalanced
/// starting build.
fn pair_of(finished: &driver::FinishedRepetition<ComparisonMeasurement>) -> ComparisonPair {
    let measurement = &finished.value;
    ComparisonPair {
        pair_index: finished.repetition_index + 1,
        first_build: first_build(finished.repetition_index),
        operations_measured: measurement.operations_measured,
        instrumented_mean_wall_ns: measurement.instrumented_mean_wall_ns,
        passthrough_mean_wall_ns: measurement.passthrough_mean_wall_ns,
        overhead_fraction: overhead_fraction(
            measurement.instrumented_mean_wall_ns,
            measurement.passthrough_mean_wall_ns,
        ),
    }
}

/// Reduces one gated cell's driver outcome to its judged cell — pure, so
/// it is tested without spawning any process.
pub fn judge_cell(outcome: &driver::CellOutcome<ComparisonMeasurement>) -> CellComparison {
    match outcome {
        driver::CellOutcome::Completed(finished) => {
            let pairs: Vec<ComparisonPair> = finished.iter().map(pair_of).collect();
            let ratios: Vec<f64> = pairs.iter().map(|pair| pair.overhead_fraction).collect();
            match summarize(&ratios) {
                Some(summary) => CellComparison::Judged { pairs, summary },
                None => CellComparison::InsufficientPairs {
                    pairs,
                    required: machine::REPETITIONS_PER_CELL,
                },
            }
        }
        driver::CellOutcome::InsufficientRepetitions { finished, required } => {
            CellComparison::InsufficientPairs {
                pairs: finished.iter().map(pair_of).collect(),
                required: *required,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workloads::{Size, Synchronous, Workload};

    fn small_full(workload: Workload) -> Cell {
        Cell {
            workload,
            size: Size::Small,
            synchronous: Synchronous::Full,
        }
    }

    fn measurement(instrumented: f64, passthrough: f64) -> ComparisonMeasurement {
        ComparisonMeasurement {
            operations_measured: 2000,
            instrumented_mean_wall_ns: instrumented,
            passthrough_mean_wall_ns: passthrough,
        }
    }

    fn finished(
        overheads_percent: &[(usize, f64)],
    ) -> Vec<driver::FinishedRepetition<ComparisonMeasurement>> {
        overheads_percent
            .iter()
            .map(|(repetition_index, percent)| driver::FinishedRepetition {
                repetition_index: *repetition_index,
                value: measurement(1000.0 * (1.0 + percent / 100.0), 1000.0),
            })
            .collect()
    }

    fn completed(overheads_percent: &[f64]) -> driver::CellOutcome<ComparisonMeasurement> {
        let indexed: Vec<(usize, f64)> = overheads_percent.iter().copied().enumerate().collect();
        driver::CellOutcome::Completed(finished(&indexed))
    }

    mod kiln_e4_11 {
        use super::*;

        #[test]
        fn a_not_gated_cell_renders_as_not_gated_without_pass_or_fail() {
            let row = CellRow {
                cell: small_full(Workload::PointRead),
                comparison: CellComparison::NotGated,
            };
            let rendered = row.render();
            assert!(rendered.contains("point-read/small/full: not gated"));
            for verdict in ["PASS", "FAIL", "INCONCLUSIVE"] {
                assert!(!rendered.contains(verdict), "got:\n{rendered}");
            }
        }

        #[test]
        fn a_not_gated_cell_does_not_vote_on_the_pilot_outcome() {
            assert!(CellComparison::NotGated.allows_pilot_success());
        }
    }

    mod kiln_e4_12 {
        use super::*;

        #[test]
        fn pairs_alternate_starting_build_beginning_with_instrumented() {
            let order: Vec<Build> = (0..5).map(first_build).collect();
            assert_eq!(
                order,
                vec![
                    Build::Instrumented,
                    Build::Passthrough,
                    Build::Instrumented,
                    Build::Passthrough,
                    Build::Instrumented,
                ]
            );
        }

        #[test]
        fn nine_pairs_start_five_with_instrumented_and_four_with_passthrough() {
            let instrumented_first = (0..machine::REPETITIONS_PER_CELL)
                .filter(|index| first_build(*index) == Build::Instrumented)
                .count();
            assert_eq!(instrumented_first, 5);
        }

        #[test]
        fn every_reported_pair_carries_its_one_based_index_and_starting_build() {
            let CellComparison::Judged { pairs, .. } = judge_cell(&completed(&[1.0, 1.0, 1.0]))
            else {
                panic!("three completed pairs must be judged");
            };
            let indexed: Vec<(usize, Build)> = pairs
                .iter()
                .map(|pair| (pair.pair_index, pair.first_build))
                .collect();
            assert_eq!(
                indexed,
                vec![
                    (1, Build::Instrumented),
                    (2, Build::Passthrough),
                    (3, Build::Instrumented)
                ]
            );
        }

        #[test]
        fn the_rendered_report_shows_each_pairs_index_and_starting_build() {
            let row = CellRow {
                cell: small_full(Workload::PointUpdate),
                comparison: judge_cell(&completed(&[1.0, 1.0])),
            };
            let rendered = row.render();
            assert!(rendered.contains("pair=1  first=instrumented"));
            assert!(rendered.contains("pair=2  first=passthrough"));
        }
    }

    mod kiln_e4_13 {
        use super::*;

        #[test]
        fn overhead_fraction_is_the_instrumented_excess_over_the_passthrough_mean() {
            assert_eq!(overhead_fraction(1000.0, 1000.0), 0.0);
            assert!((overhead_fraction(1020.0, 1000.0) - 0.02).abs() < 1e-9);
        }

        #[test]
        fn overhead_fraction_is_infinite_for_a_non_positive_passthrough_baseline() {
            assert_eq!(overhead_fraction(1000.0, 0.0), f64::INFINITY);
            assert_eq!(overhead_fraction(1000.0, -5.0), f64::INFINITY);
        }

        #[test]
        fn summarize_has_nothing_to_judge_without_ratios() {
            assert_eq!(summarize(&[]), None);
        }

        #[test]
        fn passes_when_even_the_largest_ratio_is_below_the_bound() {
            let summary = summarize(&[0.001, 0.019, 0.01]).unwrap();
            assert_eq!(summary.verdict, Verdict::Pass);
        }

        #[test]
        fn fails_when_even_the_smallest_ratio_is_above_the_bound() {
            let summary = summarize(&[0.021, 0.5, 0.03]).unwrap();
            assert_eq!(summary.verdict, Verdict::Fail);
        }

        #[test]
        fn is_inconclusive_when_the_range_straddles_the_bound() {
            let summary = summarize(&[0.01, 0.03]).unwrap();
            assert_eq!(summary.verdict, Verdict::Inconclusive);
        }

        #[test]
        fn a_ratio_exactly_at_the_bound_is_inconclusive_never_pass_or_fail() {
            assert_eq!(summarize(&[0.02]).unwrap().verdict, Verdict::Inconclusive);
            assert_eq!(
                summarize(&[0.02, 0.02, 0.02]).unwrap().verdict,
                Verdict::Inconclusive
            );
        }

        #[test]
        fn reports_the_median_with_the_min_max_range() {
            let summary = summarize(&[0.03, 0.01, 0.02]).unwrap();
            assert_eq!(
                (summary.median, summary.min, summary.max),
                (0.02, 0.01, 0.03)
            );
        }

        #[test]
        fn the_median_of_an_even_count_averages_the_two_middle_ratios() {
            assert!((summarize(&[0.0, 0.01, 0.03, 0.04]).unwrap().median - 0.02).abs() < 1e-12);
        }

        #[test]
        fn a_completed_cell_is_judged_from_its_pairs_overhead_ratios() {
            let CellComparison::Judged { pairs, summary } = judge_cell(&completed(&[1.0, 3.0]))
            else {
                panic!("completed pairs must be judged");
            };
            assert_eq!(pairs.len(), 2);
            assert!((pairs[1].overhead_fraction - 0.03).abs() < 1e-9);
            assert_eq!(summary.verdict, Verdict::Inconclusive);
        }

        #[test]
        fn the_rendered_cell_names_its_median_range_and_verdict() {
            for (percents, line) in [
                (&[0.5][..], "median=0.50% range=[0.50%, 0.50%] -> PASS"),
                (&[5.0][..], "median=5.00% range=[5.00%, 5.00%] -> FAIL"),
                (
                    &[1.0, 3.0][..],
                    "median=2.00% range=[1.00%, 3.00%] -> INCONCLUSIVE",
                ),
            ] {
                let row = CellRow {
                    cell: small_full(Workload::PointUpdate),
                    comparison: judge_cell(&completed(percents)),
                };
                assert!(row.render().contains(line), "{}", row.render());
            }
        }

        #[test]
        fn the_pilot_succeeds_only_for_a_passing_judged_cell() {
            for (overheads, expected) in [
                (&[0.5][..], true),
                (&[1.0, 3.0][..], false),
                (&[5.0][..], false),
            ] {
                assert_eq!(
                    judge_cell(&completed(overheads)).allows_pilot_success(),
                    expected,
                    "{overheads:?}"
                );
            }
        }
    }

    mod kiln_e4_14 {
        use super::*;

        /// Pairs 1, 2, 3 and 5 of nine completed; pair 4 exhausted its retries.
        fn eight_of_nine_outcome() -> driver::CellOutcome<ComparisonMeasurement> {
            driver::CellOutcome::InsufficientRepetitions {
                finished: finished(&[(0, 1.0), (1, 1.0), (2, 1.0), (4, 1.0)]),
                required: 9,
            }
        }

        #[test]
        fn a_cell_with_an_exhausted_pair_keeps_every_completed_pair_with_its_index_and_build() {
            let CellComparison::InsufficientPairs { pairs, required } =
                judge_cell(&eight_of_nine_outcome())
            else {
                panic!("an exhausted pair makes the cell insufficient");
            };

            let kept: Vec<(usize, Build)> = pairs
                .iter()
                .map(|pair| (pair.pair_index, pair.first_build))
                .collect();
            assert_eq!(
                kept,
                vec![
                    (1, Build::Instrumented),
                    (2, Build::Passthrough),
                    (3, Build::Instrumented),
                    (5, Build::Instrumented),
                ]
            );
            assert_eq!(required, 9);
        }

        #[test]
        fn the_rendered_insufficient_cell_names_its_reason_and_lists_its_completed_pairs() {
            let row = CellRow {
                cell: small_full(Workload::PointUpdate),
                comparison: judge_cell(&eight_of_nine_outcome()),
            };
            let rendered = row.render();

            assert!(
                rendered.contains("inconclusive — insufficient repetitions (4/9 pairs succeeded)")
            );
            for line in [
                "pair=1  first=instrumented",
                "pair=2  first=passthrough",
                "pair=3  first=instrumented",
                "pair=5  first=instrumented",
            ] {
                assert!(rendered.contains(line), "{line} missing from:\n{rendered}");
            }
            assert!(!rendered.contains("pair=4"), "{rendered}");
            for verdict in ["PASS", "FAIL", "INCONCLUSIVE"] {
                assert!(!rendered.contains(verdict), "no verdict without nine pairs");
            }
        }

        #[test]
        fn an_insufficient_cell_never_lets_the_pilot_succeed() {
            assert!(!judge_cell(&eight_of_nine_outcome()).allows_pilot_success());
        }
    }
}
