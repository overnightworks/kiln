//! The feasibility pilot's report (protocol.md §13): mechanics only, never
//! the verdict quantities protocol §10 judges. It carries per-operation
//! wall time (instrumented vs. the passthrough build, §5's overhead
//! bound), run duration, and monitor sample coverage — never
//! `xsync_time`, `xwrite_time`, `remainder`, or any ratio derived from
//! them. [`PilotReport`] enforces that by construction: its fields have
//! nowhere to put such a number.
//!
//! The pure reductions ([`expected_sample_count`], [`coverage_ratio`], the
//! report rendering and its exit decision) are unit-tested without SQLite,
//! `/proc`, or a real process. The runs that fill the report are
//! [`crate::pilot_run`]'s.

use std::time::Duration;

use crate::cells::all_cells;
use crate::comparison::CellRow;

/// How many 200 ms samples (protocol §9) a window of `duration` should
/// contain, rounding down: a window shorter than one interval expects
/// zero, not a fractional sample.
pub fn expected_sample_count(duration: Duration, interval: Duration) -> usize {
    if interval.is_zero() {
        return 0;
    }
    (duration.as_secs_f64() / interval.as_secs_f64()).floor() as usize
}

/// `actual / expected`, as a ratio the pilot reports against protocol
/// §9's 200 ms cadence — not rounded or clamped, so a reader can see
/// over-coverage (a burst of quick samples) as well as under-coverage.
/// An `expected` of zero (a window shorter than one interval) always
/// reports full coverage: there was nothing to miss.
pub fn coverage_ratio(actual: usize, expected: usize) -> f64 {
    if expected == 0 {
        1.0
    } else {
        actual as f64 / expected as f64
    }
}

/// Whether protocol §7's secondary `perf` breakdown ran. The pilot never
/// runs it itself (`perf` access is a separate operator grant, protocol
/// §12); this records why, instead of silently omitting the section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerfStatus {
    Skipped { reason: String },
}

/// Monitor sample coverage over the pilot's measured windows (protocol
/// §13): the samples §9 expects at its interval versus those written.
#[derive(Debug, Clone, Copy)]
pub struct MonitorCoverageRow {
    pub interval: Duration,
    pub window: Duration,
    pub expected_samples: usize,
    pub actual_samples: usize,
}

impl MonitorCoverageRow {
    pub fn coverage_ratio(&self) -> f64 {
        coverage_ratio(self.actual_samples, self.expected_samples)
    }
}

/// The pilot's whole report (protocol §13): mechanics only. There is no
/// field here a verdict quantity could occupy.
#[derive(Debug, Clone)]
pub struct PilotReport {
    pub source_commit: String,
    pub instrumented_binary_sha256: String,
    pub passthrough_binary_sha256: String,
    pub cells: Vec<CellRow>,
    pub run_duration: Duration,
    pub monitor: MonitorCoverageRow,
    pub rejected_attempts: usize,
    pub perf: PerfStatus,
}

impl PilotReport {
    /// Renders the report as plain text — the pilot's committed output
    /// (protocol §13: "its output never contains `xsync_time`,
    /// `xwrite_time`, `remainder`, or any ratio from them").
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("E4 feasibility pilot (protocol.md §13) — mechanics only\n");
        out.push_str(&format!("source commit: {}\n", self.source_commit));
        out.push_str(&format!(
            "instrumented binary sha256: {}\n",
            self.instrumented_binary_sha256
        ));
        out.push_str(&format!(
            "passthrough binary sha256:  {}\n",
            self.passthrough_binary_sha256
        ));
        out.push_str(&format!("run duration: {:?}\n", self.run_duration));
        out.push_str(&format!("rejected attempts: {}\n", self.rejected_attempts));
        out.push_str(&format!(
            "perf (§7): {}\n",
            match &self.perf {
                PerfStatus::Skipped { reason } => format!("skipped — {reason}"),
            }
        ));
        out.push_str(&format!(
            "monitor sample coverage: {}/{} samples ({:.1}%) at {:?} intervals over {:?} of measured windows\n",
            self.monitor.actual_samples,
            self.monitor.expected_samples,
            self.monitor.coverage_ratio() * 100.0,
            self.monitor.interval,
            self.monitor.window,
        ));
        out.push_str(
            "\noverhead bound (§5, frozen at 2%; gated cells only, every pair reported as measured):\n",
        );
        for cell in &self.cells {
            out.push_str(&cell.render());
        }
        out.push_str(&format!("\npilot outcome: {}\n", self.outcome_line()));
        out
    }

    fn outcome_line(&self) -> &'static str {
        if self.all_gated_cells_pass() {
            "PASS — every gated cell passed the 2% overhead bound"
        } else {
            "FAIL — a gated cell did not pass the 2% overhead bound (protocol §5): it failed, \
             was inconclusive or had insufficient repetitions, or the report lacks a cell"
        }
    }

    /// Whether the report holds exactly one row for each of the pilot's
    /// cells, every gated cell compared and every read-only cell marked
    /// not gated: a missing or surplus row never passes by default.
    fn covers_every_cell_once(&self) -> bool {
        self.cells.len() == all_cells().len() && all_cells().iter().all(|expected| {
            let mut rows = self.cells.iter().filter(|row| row.cell == *expected);
            matches!(
                (rows.next(), rows.next()),
                (Some(row), None) if row.comparison.is_not_gated() != expected.workload.is_gated()
            )
        })
    }

    /// Whether the pilot succeeded: every cell present once and every
    /// gated cell passed its overhead bound.
    pub fn all_gated_cells_pass(&self) -> bool {
        self.covers_every_cell_once()
            && self
                .cells
                .iter()
                .all(|row| row.comparison.allows_pilot_success())
    }

    /// The process exit status the pilot ends with: 0 for a passing
    /// report, 1 otherwise.
    pub fn exit_code(&self) -> u8 {
        if self.all_gated_cells_pass() { 0 } else { 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builds::Build;
    use crate::comparison::{CellComparison, ComparisonPair, OverheadSummary, Verdict};
    use crate::workloads::Workload;

    fn judged(verdict: Verdict) -> CellComparison {
        let ratio = match verdict {
            Verdict::Pass => 0.01,
            Verdict::Fail => 0.05,
            Verdict::Inconclusive => 0.02,
        };
        CellComparison::Judged {
            pairs: vec![ComparisonPair {
                pair_index: 1,
                first_build: Build::Instrumented,
                operations_measured: 2000,
                instrumented_mean_wall_ns: 1000.0 * (1.0 + ratio),
                passthrough_mean_wall_ns: 1000.0,
                overhead_fraction: ratio,
            }],
            summary: OverheadSummary {
                median: ratio,
                min: ratio,
                max: ratio,
                verdict,
            },
        }
    }

    /// A report with a row for every cell: gated ones carry `gated`, the
    /// read-only ones are not gated.
    fn rows_for_every_cell(gated: &CellComparison) -> Vec<CellRow> {
        all_cells()
            .into_iter()
            .map(|cell| CellRow {
                cell,
                comparison: if cell.workload.is_gated() {
                    gated.clone()
                } else {
                    CellComparison::NotGated
                },
            })
            .collect()
    }

    fn report(cells: Vec<CellRow>) -> PilotReport {
        PilotReport {
            source_commit: "c0ffee".to_string(),
            instrumented_binary_sha256: "a".repeat(64),
            passthrough_binary_sha256: "b".repeat(64),
            cells,
            run_duration: Duration::from_millis(500),
            monitor: MonitorCoverageRow {
                interval: Duration::from_millis(200),
                window: Duration::from_millis(500),
                expected_samples: 2,
                actual_samples: 2,
            },
            rejected_attempts: 0,
            perf: PerfStatus::Skipped {
                reason: "perf access not granted this session".to_string(),
            },
        }
    }

    fn passing_report() -> PilotReport {
        report(rows_for_every_cell(&judged(Verdict::Pass)))
    }

    mod kiln_e4_11 {
        use super::*;

        #[test]
        fn the_rendered_report_compares_twelve_gated_cells_and_marks_four_read_cells_not_gated() {
            let rendered = passing_report().render();

            assert_eq!(rendered.matches(": gated, ").count(), 12, "{rendered}");
            assert_eq!(rendered.matches(": not gated").count(), 4, "{rendered}");
            for workload in ["point-update", "insert", "transfer"] {
                for size in ["small", "medium"] {
                    for synchronous in ["full", "normal"] {
                        let line = format!("{workload}/{size}/{synchronous}: gated");
                        assert!(rendered.contains(&line), "missing {line}");
                    }
                }
            }
            for size in ["small", "medium"] {
                for synchronous in ["full", "normal"] {
                    let line = format!("point-read/{size}/{synchronous}: not gated");
                    assert!(rendered.contains(&line), "missing {line}");
                }
            }
        }

        #[test]
        fn a_report_missing_a_cell_never_passes() {
            let mut cells = rows_for_every_cell(&judged(Verdict::Pass));
            for removed in 0..cells.len() {
                let mut without = cells.clone();
                without.remove(removed);
                assert!(
                    !report(without).all_gated_cells_pass(),
                    "a report without {} must not pass",
                    cells[removed].cell.label()
                );
            }
            cells.clear();
            assert!(!report(cells).all_gated_cells_pass());
        }

        #[test]
        fn a_report_with_a_repeated_or_unexpected_row_never_passes() {
            let mut repeated = rows_for_every_cell(&judged(Verdict::Pass));
            repeated.push(repeated[0].clone());
            assert!(!report(repeated).all_gated_cells_pass());

            let mut swapped = rows_for_every_cell(&judged(Verdict::Pass));
            let duplicate = swapped[1].clone();
            swapped[0] = duplicate;
            assert!(!report(swapped).all_gated_cells_pass());
        }

        #[test]
        fn a_gated_cell_reported_as_not_gated_never_passes() {
            let mut cells = rows_for_every_cell(&judged(Verdict::Pass));
            let gated = cells
                .iter_mut()
                .find(|row| row.cell.workload.is_gated())
                .unwrap();
            gated.comparison = CellComparison::NotGated;
            assert!(!report(cells).all_gated_cells_pass());
        }

        #[test]
        fn a_read_cell_reported_with_a_comparison_never_passes() {
            let mut cells = rows_for_every_cell(&judged(Verdict::Pass));
            let read = cells
                .iter_mut()
                .find(|row| row.cell.workload == Workload::PointRead)
                .unwrap();
            read.comparison = judged(Verdict::Pass);
            assert!(!report(cells).all_gated_cells_pass());
        }
    }

    mod kiln_e4_15 {
        use super::*;

        #[test]
        fn rendered_report_never_contains_a_verdict_derivable_field_name() {
            let rendered = passing_report().render();
            for forbidden in ["xsync", "xwrite", "remainder", "xSync", "xWrite"] {
                assert!(
                    !rendered.contains(forbidden),
                    "pilot output must never contain {forbidden:?} (protocol §13); got:\n{rendered}"
                );
            }
        }

        #[test]
        fn rendered_report_carries_commit_hashes_duration_and_the_skipped_perf_reason() {
            let rendered = passing_report().render();
            for expected in [
                "source commit: c0ffee".to_string(),
                "a".repeat(64),
                "b".repeat(64),
                "run duration: 500ms".to_string(),
                "perf (§7): skipped — perf access not granted this session".to_string(),
            ] {
                assert!(
                    rendered.contains(&expected),
                    "missing {expected:?} in:\n{rendered}"
                );
            }
        }
    }

    mod kiln_e4_16 {
        use super::*;

        #[test]
        fn the_pilot_succeeds_when_every_gated_cell_passes_and_the_read_cells_are_not_gated() {
            assert!(passing_report().all_gated_cells_pass());
        }

        #[test]
        fn the_pilot_fails_for_a_failing_or_inconclusive_gated_cell() {
            for verdict in [Verdict::Fail, Verdict::Inconclusive] {
                assert!(!report(rows_for_every_cell(&judged(verdict))).all_gated_cells_pass());
            }
        }

        #[test]
        fn one_failing_cell_among_passing_ones_fails_the_pilot() {
            let mut cells = rows_for_every_cell(&judged(Verdict::Pass));
            let last_gated = cells
                .iter_mut()
                .rfind(|row| row.cell.workload.is_gated())
                .unwrap();
            last_gated.comparison = judged(Verdict::Fail);
            assert!(!report(cells).all_gated_cells_pass());
        }

        #[test]
        fn the_pilot_fails_for_a_gated_cell_with_insufficient_repetitions() {
            let insufficient = CellComparison::InsufficientPairs {
                pairs: Vec::new(),
                required: 9,
            };
            assert!(!report(rows_for_every_cell(&insufficient)).all_gated_cells_pass());
        }

        #[test]
        fn the_pilot_exits_zero_for_a_passing_report_and_one_for_any_other() {
            assert_eq!(passing_report().exit_code(), 0);
            for verdict in [Verdict::Fail, Verdict::Inconclusive] {
                assert_eq!(report(rows_for_every_cell(&judged(verdict))).exit_code(), 1);
            }
            assert_eq!(report(Vec::new()).exit_code(), 1);
        }

        #[test]
        fn the_rendered_report_ends_with_the_outcome_the_exit_status_follows() {
            let passing = passing_report().render();
            assert!(passing.contains("pilot outcome: PASS"), "{passing}");

            let failing = report(rows_for_every_cell(&judged(Verdict::Fail))).render();
            assert!(failing.contains("pilot outcome: FAIL"), "{failing}");
            assert!(!failing.contains("pilot outcome: PASS"), "{failing}");
        }
    }

    mod kiln_e4_17 {
        use super::*;

        #[test]
        fn expected_sample_count_rounds_down_a_partial_interval() {
            assert_eq!(
                expected_sample_count(Duration::from_millis(999), Duration::from_millis(200)),
                4
            );
            assert_eq!(
                expected_sample_count(Duration::from_millis(1000), Duration::from_millis(200)),
                5
            );
        }

        #[test]
        fn expected_sample_count_is_zero_for_a_window_shorter_than_one_interval() {
            assert_eq!(
                expected_sample_count(Duration::from_millis(50), Duration::from_millis(200)),
                0
            );
        }

        #[test]
        fn coverage_ratio_is_the_written_share_of_the_expected_samples() {
            assert_eq!(coverage_ratio(5, 5), 1.0);
            assert_eq!(coverage_ratio(3, 6), 0.5);
        }

        #[test]
        fn coverage_ratio_treats_zero_expected_as_full_coverage() {
            assert_eq!(coverage_ratio(0, 0), 1.0);
        }

        #[test]
        fn monitor_coverage_row_reports_its_samples_share() {
            let row = MonitorCoverageRow {
                interval: Duration::from_millis(200),
                window: Duration::from_millis(1000),
                expected_samples: 5,
                actual_samples: 3,
            };
            assert!((row.coverage_ratio() - 0.6).abs() < 1e-9);
        }

        #[test]
        fn the_rendered_report_states_samples_written_versus_expected() {
            let rendered = passing_report().render();
            assert!(rendered.contains("monitor sample coverage: 2/2 samples (100.0%)"));
        }
    }
}
