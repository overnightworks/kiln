//! The feasibility pilot (protocol.md §13): checks mechanics only, never
//! the verdict quantities protocol §10 judges. Its report carries
//! per-operation wall time (instrumented vs. the passthrough build, §5's
//! overhead bound), run duration, and monitor sample coverage — never
//! `xsync_time`, `xwrite_time`, `remainder`, or any ratio derived from
//! them. [`PilotReport`] enforces that by construction: its fields have
//! nowhere to put such a number.
//!
//! The pure reductions ([`expected_sample_count`], [`coverage_ratio`], the
//! report rendering) are unit-tested without SQLite, `/proc`, or a real
//! process; [`measure_cell_in_process`] and [`MonitorSampler`] are the
//! thin real adapters, exactly like [`crate::preflight::run_real`] is for
//! the preflight checks. The overhead comparison itself lives in
//! [`crate::comparison`].

use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use libsqlite3_sys as ffi;
use rusqlite::{Connection, OpenFlags};

use crate::comparison::WorkloadComparison;
use crate::generators;
use crate::monitor;
use crate::workloads::{self, PointReadStatements, PointUpdateStatements, Size, Synchronous};

/// The two workloads the pilot drives — one read, one write path, the
/// smallest pair that lets the overhead bound be checked against both a
/// path with no timed VFS calls at all and one with both. Inside the
/// timed transaction, point-read's `BEGIN DEFERRED`/`SELECT`/`COMMIT`
/// calls neither `xWrite` nor `xSync` — a read-only commit appends
/// nothing to the WAL, so there is nothing to sync — while point-update's
/// `BEGIN IMMEDIATE`/`UPDATE`/`COMMIT` always calls `xWrite` and, under
/// `synchronous=FULL`, `xSync` too. (Setup — schema creation, the initial
/// load — does invoke both methods, but that happens before any timed
/// window opens.) Protocol §13 names no required workload subset for the
/// pilot, only that it must not report §10's verdict quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PilotWorkload {
    PointRead,
    PointUpdate,
}

impl PilotWorkload {
    pub const ALL: [PilotWorkload; 2] = [PilotWorkload::PointRead, PilotWorkload::PointUpdate];

    /// Whether protocol §5's overhead bound gates this workload: only a
    /// cell whose timed transaction calls `xWrite` or `xSync` is gated,
    /// and a read-only transaction never does.
    pub fn is_gated(self) -> bool {
        match self {
            PilotWorkload::PointRead => false,
            PilotWorkload::PointUpdate => true,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PilotWorkload::PointRead => "point-read",
            PilotWorkload::PointUpdate => "point-update",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "point-read" => Some(PilotWorkload::PointRead),
            "point-update" => Some(PilotWorkload::PointUpdate),
            _ => None,
        }
    }

    /// Maps to the corresponding frozen [`workloads::Workload`] (protocol
    /// §2), so a caller needing protocol §8.1's frozen operation counts
    /// ([`driver::operation_plan`]) does not have to match on
    /// [`PilotWorkload`] a second time.
    pub fn as_workload(self) -> workloads::Workload {
        match self {
            PilotWorkload::PointRead => workloads::Workload::PointRead,
            PilotWorkload::PointUpdate => workloads::Workload::PointUpdate,
        }
    }
}

/// One in-process measurement: total wall time summed over every timed
/// operation, divided by the operation count. Carries no `xsync`/`xwrite`/
/// `remainder` field — there is nowhere in this type to put one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredCell {
    pub operations_measured: usize,
    pub mean_wall_ns: f64,
}

/// Protocol §8.1/§8.2: one attempt's total operation count is warm-up
/// plus the timed count — named and exactly tested so a wrong
/// combination (the two swapped, multiplied, or otherwise miscombined)
/// is caught directly, rather than only showing up as a plausible-looking
/// wrong magnitude several steps later.
pub fn total_operation_count(warmup: usize, operations: usize) -> usize {
    warmup + operations
}

/// Protocol §8.2: whether operation `index` (0-based, issue order) falls
/// at or after the warm-up boundary, i.e. is one of the timed operations
/// whose wall time counts toward the mean — pure and exactly tested so
/// `>=` cannot silently become `<` or land one operation off.
pub fn is_past_warmup(index: usize, warmup: usize) -> bool {
    index >= warmup
}

/// Protocol §8.5's `Σwall/N`: `total_wall`'s mean per operation, in
/// nanoseconds — pure and exactly tested so `/` cannot silently become
/// `*` or `%`.
pub fn mean_wall_nanos(total_wall: Duration, operations: usize) -> f64 {
    total_wall.as_nanos() as f64 / operations as f64
}

/// Runs `warmup` discarded operations then `operations` timed ones for one
/// workload, through the shim VFS (so the timed methods this binary's
/// feature selection controls are actually on the call path — protocol
/// §5), and reduces the timed operations' wall times to their mean.
/// Real SQLite I/O: the thin adapter, not unit-tested by mutation — the
/// pure reductions above and below carry the tested logic.
pub fn measure_cell_in_process(
    workload: PilotWorkload,
    warmup: usize,
    operations: usize,
    seed: u64,
) -> rusqlite::Result<MeasuredCell> {
    crate::vfs::install().map_err(|reason| {
        rusqlite::Error::SqliteFailure(
            ffi::Error::new(ffi::SQLITE_ERROR),
            Some(format!("installing the shim VFS failed: {reason}")),
        )
    })?;

    let dir = tempfile::tempdir().map_err(io_as_sqlite_error)?;
    let db_path = dir.path().join("pilot.db");
    let conn = Connection::open_with_flags_and_vfs(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        crate::vfs::VFS_NAME,
    )?;
    // The pilot checks mechanics only (protocol §13), so it always uses
    // the small size — [`Size::Small`] is the one production owner of
    // that row count (protocol §3), reused here rather than duplicated.
    let row_count = Size::Small.row_count();
    workloads::configure_connection(&conn, Synchronous::Full)?;
    workloads::create_schema(&conn)?;
    workloads::initial_load(&conn, row_count)?;

    let mut txn = workloads::TransactionStatements::prepare(&conn)?;
    let total_ops = total_operation_count(warmup, operations);

    let mut total_wall = Duration::ZERO;
    match workload {
        PilotWorkload::PointRead => {
            let mut stmts = PointReadStatements::prepare(&conn)?;
            let trace = generators::point_read_trace(seed, row_count, total_ops);
            for (index, params) in trace.iter().enumerate() {
                let (_, timing) = crate::vfs::time_operation(|| {
                    workloads::execute_point_read(&mut txn, &mut stmts, params)
                })
                .map_err(timing_as_sqlite_error)?;
                txn.end_operation()?;
                if is_past_warmup(index, warmup) {
                    total_wall += timing.wall;
                }
            }
        }
        PilotWorkload::PointUpdate => {
            let mut stmts = PointUpdateStatements::prepare(&conn)?;
            let trace = generators::point_update_trace(seed, row_count, total_ops);
            for (index, params) in trace.iter().enumerate() {
                let (_, timing) = crate::vfs::time_operation(|| {
                    workloads::execute_point_update(&mut txn, &mut stmts, params)
                })
                .map_err(timing_as_sqlite_error)?;
                txn.end_operation()?;
                if is_past_warmup(index, warmup) {
                    total_wall += timing.wall;
                }
            }
        }
    }

    Ok(MeasuredCell {
        operations_measured: operations,
        mean_wall_ns: mean_wall_nanos(total_wall, operations),
    })
}

fn io_as_sqlite_error(err: io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_ERROR), Some(err.to_string()))
}

/// One sanitized message for every [`crate::vfs::TimingError::Measurement`]
/// (protocol §13: the pilot's output "never contains `xsync_time`,
/// `xwrite_time`, `remainder`, or any ratio from them" — not even on a
/// failure path). `InconsistentTiming`'s own `Display` carries exactly
/// those numbers, so this name is reported instead of rendering it.
const INCONSISTENT_TIMING_MESSAGE: &str = "timing measurement was internally inconsistent (protocol §5/§13); rerun to collect a fresh sample";

fn timing_as_sqlite_error<E: std::fmt::Display>(
    err: crate::vfs::TimingError<E>,
) -> rusqlite::Error {
    let message = match err {
        // An ordinary SQLite error from the operation itself — never a
        // numeric timing split, so it is safe to report verbatim.
        crate::vfs::TimingError::Operation(inner) => inner.to_string(),
        crate::vfs::TimingError::Measurement(_) => INCONSISTENT_TIMING_MESSAGE.to_string(),
    };
    rusqlite::Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_ERROR), Some(message))
}

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

/// Samples [`monitor::capture`] on a fixed cadence from a background
/// thread until [`stop`](Self::stop) is called — the pilot's proof that
/// protocol §9's 200 ms sampling cadence is mechanically achievable over
/// the whole run. This is a coverage statistic only, never the gating
/// mechanism: an in-process thread inherits whatever CPU the pilot
/// itself runs on, so it cannot stand in for a genuinely separate,
/// independently pinned monitor process. Every attempt's actual
/// interference gate is [`crate::interference::sample_interference`], a real
/// `monitor-sample` child process pinned to the complement of
/// `{PINNED_CPU, SIBLING_CPU}` (protocol §9), called from
/// [`run_pilot_comparisons`].
pub struct MonitorSampler {
    stop_flag: Arc<AtomicBool>,
    handle: JoinHandle<Vec<io::Result<monitor::Sample>>>,
}

impl MonitorSampler {
    pub fn start(interval: Duration) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag_for_thread = Arc::clone(&stop_flag);
        let handle = thread::spawn(move || {
            let mut samples = Vec::new();
            while !flag_for_thread.load(Ordering::Relaxed) {
                samples.push(monitor::capture());
                thread::sleep(interval);
            }
            samples
        });
        MonitorSampler { stop_flag, handle }
    }

    /// Stops sampling and returns every sample captured, in order.
    pub fn stop(self) -> Vec<io::Result<monitor::Sample>> {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.handle.join().unwrap_or_default()
    }
}

/// Whether protocol §7's secondary `perf` breakdown ran. The pilot never
/// runs it itself (`perf` access is a separate operator grant, protocol
/// §12); this records why, instead of silently omitting the section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerfStatus {
    Skipped { reason: String },
}

/// Parses the `measure-cell` child's one-line stdout contract
/// (`mean_wall_ns=<value>`) back into its reported mean — pure, so a
/// malformed or missing line is tested without spawning anything; the
/// counterpart to `main.rs::run_measure_cell`'s `println!("mean_wall_ns={}", ...)`.
fn parse_mean_wall_ns(stdout: &str) -> Option<f64> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("mean_wall_ns="))
        .and_then(|value| value.trim().parse::<f64>().ok())
}

/// Spawns one `measure-cell` child process against `binary` and reads
/// back its reported mean wall time — protocol §9's "equivalent isolated
/// child processes pinned to CPU 2": the pinning itself happens inside
/// the child (`main.rs::run_measure_cell` calls
/// [`crate::affinity::pin_current_process`] before measuring), so this
/// one function spawns the instrumented binary and the passthrough one
/// identically, instead of measuring one in-process and the other as a
/// child. `pub` so an integration test (which alone can resolve the real
/// compiled binary's path) can drive it end to end;
/// [`parse_mean_wall_ns`] carries the tested parsing logic.
pub fn run_measure_cell_child(
    binary: &Path,
    workload: PilotWorkload,
    warmup: usize,
    operations: usize,
    seed: u64,
) -> io::Result<f64> {
    let output = Command::new(binary)
        .arg("measure-cell")
        .arg(workload.label())
        .arg(warmup.to_string())
        .arg(operations.to_string())
        .arg(seed.to_string())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "measure-cell for {workload:?} against {} exited with {}: {}",
            binary.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_mean_wall_ns(&stdout).ok_or_else(|| {
        io::Error::other(format!(
            "could not parse mean_wall_ns from {}: {stdout:?}",
            binary.display()
        ))
    })
}

/// Monitor sample coverage for the pilot's measured window (protocol
/// §13).
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
    pub instrumented_binary_sha256: String,
    pub passthrough_binary_sha256: String,
    pub cells: Vec<WorkloadComparison>,
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
            "monitor sample coverage: {}/{} samples ({:.1}%) at {:?} intervals over a {:?} window\n",
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
        out
    }

    /// Whether the pilot succeeded — the exit-code decision main.rs makes
    /// after rendering: every gated cell passed its overhead bound.
    pub fn all_gated_cells_pass(&self) -> bool {
        self.cells
            .iter()
            .all(|cell| cell.comparison.allows_pilot_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comparison::{CellComparison, ComparisonPair, OverheadSummary, Verdict};

    fn judged(verdict: Verdict) -> WorkloadComparison {
        let ratio = match verdict {
            Verdict::Pass => 0.01,
            Verdict::Fail => 0.05,
            Verdict::Inconclusive => 0.02,
        };
        WorkloadComparison {
            workload: PilotWorkload::PointUpdate,
            comparison: CellComparison::Judged {
                pairs: vec![ComparisonPair {
                    pair_index: 1,
                    first_build: crate::builds::Build::Instrumented,
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
            },
        }
    }

    fn not_gated_read() -> WorkloadComparison {
        WorkloadComparison {
            workload: PilotWorkload::PointRead,
            comparison: CellComparison::NotGated,
        }
    }

    fn sample_report(cells: Vec<WorkloadComparison>) -> PilotReport {
        PilotReport {
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

    mod kiln_e4_05 {
        use super::*;

        #[test]
        fn total_operation_count_is_warmup_plus_operations() {
            assert_eq!(total_operation_count(5, 10), 15);
            assert_eq!(total_operation_count(0, 0), 0);
        }

        #[test]
        fn is_past_warmup_is_true_at_and_after_the_boundary_only() {
            assert!(!is_past_warmup(4, 5));
            assert!(is_past_warmup(5, 5));
            assert!(is_past_warmup(6, 5));
        }

        #[test]
        fn mean_wall_nanos_divides_total_by_operation_count() {
            assert_eq!(mean_wall_nanos(Duration::from_nanos(100), 10), 10.0);
            // 105 / 10 = 10.5, distinct from 105 % 10 = 5 and 105 * 10 = 1050
            // — pins the division against both other arithmetic mutations.
            assert_eq!(mean_wall_nanos(Duration::from_nanos(105), 10), 10.5);
        }

        #[test]
        fn measure_cell_in_process_reports_a_positive_mean_wall_time_for_both_workloads() {
            // The shim's counters are process-wide globals (protocol §5), so
            // this test — which drives real commits through them for both
            // workloads — must not run concurrently with any other test that
            // resets or reads them (see crate::test_support and vfs.rs's own
            // tests).
            let _guard = crate::test_support::lock_vfs_counters();
            for workload in PilotWorkload::ALL {
                let measured =
                    measure_cell_in_process(workload, 5, 10, crate::driver::real_clock().as_secs())
                        .expect("measuring a small in-process cell must succeed");
                assert_eq!(measured.operations_measured, 10);
                assert!(
                    measured.mean_wall_ns > 0.0,
                    "a real SQLite operation's wall time must be positive, got {}",
                    measured.mean_wall_ns
                );
            }
        }
    }

    mod kiln_e4_07 {
        use super::*;

        #[test]
        fn run_measure_cell_child_fails_when_the_binary_cannot_be_spawned() {
            let result = run_measure_cell_child(
                Path::new("/nonexistent-kiln-e4-harness-binary"),
                PilotWorkload::PointRead,
                1,
                1,
                1,
            );
            assert!(result.is_err());
        }
    }

    mod kiln_e4_08 {
        use super::*;

        #[test]
        fn parse_mean_wall_ns_reads_the_measure_cell_contract() {
            assert_eq!(parse_mean_wall_ns("mean_wall_ns=1234.5\n"), Some(1234.5));
        }

        #[test]
        fn parse_mean_wall_ns_is_none_for_output_missing_the_line() {
            assert_eq!(parse_mean_wall_ns("something else entirely\n"), None);
        }
    }

    mod kiln_e4_11 {
        use super::*;

        #[test]
        fn only_the_write_workload_is_gated() {
            let gated: Vec<&str> = PilotWorkload::ALL
                .into_iter()
                .filter(|workload| workload.is_gated())
                .map(PilotWorkload::label)
                .collect();
            assert_eq!(gated, vec!["point-update"]);
        }

        #[test]
        fn the_rendered_report_names_a_read_only_workload_as_not_gated() {
            let rendered = sample_report(vec![judged(Verdict::Pass), not_gated_read()]).render();
            assert!(rendered.contains("point-read: not gated"));
            assert!(rendered.contains("point-update: gated"));
        }
    }

    mod kiln_e4_15 {
        use super::*;

        #[test]
        fn rendered_report_never_contains_a_verdict_derivable_field_name() {
            let rendered = sample_report(vec![judged(Verdict::Pass), not_gated_read()]).render();
            for forbidden in ["xsync", "xwrite", "remainder", "xSync", "xWrite"] {
                assert!(
                    !rendered.contains(forbidden),
                    "pilot output must never contain {forbidden:?} (protocol §13); got:\n{rendered}"
                );
            }
        }

        #[test]
        fn rendered_report_carries_hashes_duration_and_the_skipped_perf_reason() {
            let rendered = sample_report(vec![judged(Verdict::Pass)]).render();
            for expected in [
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

        #[test]
        fn timing_as_sqlite_error_never_renders_an_inconsistent_measurements_numbers() {
            let inconsistent = crate::vfs::InconsistentTiming {
                wall: Duration::from_nanos(10),
                xsync: Duration::from_nanos(8),
                xwrite: Duration::from_nanos(5),
            };
            let err: rusqlite::Error = timing_as_sqlite_error::<std::convert::Infallible>(
                crate::vfs::TimingError::Measurement(inconsistent),
            );
            let rendered = err.to_string();
            assert_eq!(
                rendered, INCONSISTENT_TIMING_MESSAGE,
                "a measurement error must render as the one sanitized message, never \
                 InconsistentTiming's own Display (protocol §13)"
            );
            for forbidden in ["xsync", "xwrite", "10ns", "8ns", "5ns"] {
                assert!(
                    !rendered.contains(forbidden),
                    "a sanitized measurement error must never leak {forbidden:?} (protocol §13); got {rendered:?}"
                );
            }
        }
    }

    mod kiln_e4_16 {
        use super::*;

        #[test]
        fn the_pilot_succeeds_when_every_gated_cell_passes_and_a_read_cell_is_not_gated() {
            assert!(
                sample_report(vec![judged(Verdict::Pass), not_gated_read()]).all_gated_cells_pass()
            );
        }

        #[test]
        fn the_pilot_fails_for_a_failing_or_inconclusive_gated_cell() {
            for verdict in [Verdict::Fail, Verdict::Inconclusive] {
                assert!(
                    !sample_report(vec![judged(verdict), not_gated_read()]).all_gated_cells_pass()
                );
            }
        }

        #[test]
        fn the_pilot_fails_for_a_gated_cell_with_insufficient_repetitions() {
            let insufficient = WorkloadComparison {
                workload: PilotWorkload::PointUpdate,
                comparison: CellComparison::InsufficientPairs {
                    pairs: Vec::new(),
                    required: 9,
                },
            };
            assert!(!sample_report(vec![insufficient]).all_gated_cells_pass());
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
            let rendered = sample_report(vec![judged(Verdict::Pass)]).render();
            assert!(rendered.contains("monitor sample coverage: 2/2 samples (100.0%)"));
        }

        #[test]
        fn monitor_sampler_collects_at_least_one_sample_over_a_window_longer_than_the_interval() {
            let sampler = MonitorSampler::start(Duration::from_millis(20));
            thread::sleep(Duration::from_millis(100));
            let samples = sampler.stop();
            assert!(
                !samples.is_empty(),
                "a 100ms window at a 20ms interval must collect at least one sample"
            );
        }
    }

    mod kiln_e4_18 {
        use super::*;

        #[test]
        fn pilot_workload_label_round_trips_through_parse() {
            for workload in PilotWorkload::ALL {
                assert_eq!(PilotWorkload::parse(workload.label()), Some(workload));
            }
            assert_eq!(PilotWorkload::parse("range-scan"), None);
        }

        #[test]
        fn pilot_workload_maps_to_its_own_frozen_workload() {
            assert_eq!(
                PilotWorkload::PointRead.as_workload(),
                workloads::Workload::PointRead
            );
            assert_eq!(
                PilotWorkload::PointUpdate.as_workload(),
                workloads::Workload::PointUpdate
            );
        }
    }
}
