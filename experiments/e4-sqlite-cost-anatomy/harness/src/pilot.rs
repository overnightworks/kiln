//! The feasibility pilot (protocol.md §13): checks mechanics only, never
//! the verdict quantities protocol §10 judges. Its report carries
//! per-operation wall time (instrumented vs. the passthrough build, §5's
//! overhead bound), run duration, and monitor sample coverage — never
//! `xsync_time`, `xwrite_time`, `remainder`, or any ratio derived from
//! them. [`PilotReport`] enforces that by construction: its fields have
//! nowhere to put such a number.
//!
//! The pure reductions ([`overhead_fraction`], [`passes_overhead_bound`],
//! [`expected_sample_count`], [`coverage_ratio`]) are unit-tested without
//! SQLite or `/proc`; [`measure_cell_in_process`] and [`MonitorSampler`]
//! are the thin, untested real adapters, exactly like
//! [`crate::preflight::run_real`] is for the preflight checks.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use libsqlite3_sys as ffi;
use rusqlite::{Connection, OpenFlags};

use crate::generators;
use crate::monitor;
use crate::workloads::{self, PointReadStatements, PointUpdateStatements, Synchronous};

/// The two workloads the pilot drives — one read, one write path, the
/// smallest pair that still exercises both of the shim's timed methods
/// (`xWrite` never fires for a read-only commit; `xSync` fires for both
/// under `synchronous=FULL`). Protocol §13 names no required workload
/// subset for the pilot, only that it must not report §10's verdict
/// quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PilotWorkload {
    PointRead,
    PointUpdate,
}

impl PilotWorkload {
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
}

/// Protocol §3's small size, the only size the pilot needs: it checks
/// mechanics, not the cache-boundary comparison §10 judges.
pub const PILOT_ROW_COUNT: u64 = 10_000;

/// One in-process measurement: total wall time summed over every timed
/// operation, divided by the operation count. Carries no `xsync`/`xwrite`/
/// `remainder` field — there is nowhere in this type to put one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredCell {
    pub operations_measured: usize,
    pub mean_wall_ns: f64,
}

/// Runs `warmup` discarded operations then `operations` timed ones for one
/// workload, through the shim VFS (so the timed methods this binary's
/// feature selection controls are actually on the call path — protocol
/// §5), and reduces the timed operations' wall times to their mean.
/// Real SQLite I/O: the thin adapter, not unit-tested by mutation — the
/// pure reductions below carry the tested logic.
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
    workloads::configure_connection(&conn, Synchronous::Full)?;
    workloads::create_schema(&conn)?;
    workloads::initial_load(&conn, PILOT_ROW_COUNT)?;

    let mut txn = workloads::TransactionStatements::prepare(&conn)?;
    let total_ops = warmup + operations;

    let mut total_wall = Duration::ZERO;
    match workload {
        PilotWorkload::PointRead => {
            let mut stmts = PointReadStatements::prepare(&conn)?;
            let trace = generators::point_read_trace(seed, PILOT_ROW_COUNT, total_ops);
            for (index, params) in trace.iter().enumerate() {
                let (_, timing) = crate::vfs::time_operation(|| {
                    workloads::execute_point_read(&mut txn, &mut stmts, params)
                })
                .map_err(timing_as_sqlite_error)?;
                txn.end_operation()?;
                if index >= warmup {
                    total_wall += timing.wall;
                }
            }
        }
        PilotWorkload::PointUpdate => {
            let mut stmts = PointUpdateStatements::prepare(&conn)?;
            let trace = generators::point_update_trace(seed, PILOT_ROW_COUNT, total_ops);
            for (index, params) in trace.iter().enumerate() {
                let (_, timing) = crate::vfs::time_operation(|| {
                    workloads::execute_point_update(&mut txn, &mut stmts, params)
                })
                .map_err(timing_as_sqlite_error)?;
                txn.end_operation()?;
                if index >= warmup {
                    total_wall += timing.wall;
                }
            }
        }
    }

    Ok(MeasuredCell {
        operations_measured: operations,
        mean_wall_ns: total_wall.as_nanos() as f64 / operations as f64,
    })
}

fn io_as_sqlite_error(err: io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_ERROR), Some(err.to_string()))
}

fn timing_as_sqlite_error<E: std::fmt::Display>(
    err: crate::vfs::TimingError<E>,
) -> rusqlite::Error {
    let message = match err {
        crate::vfs::TimingError::Operation(inner) => inner.to_string(),
        crate::vfs::TimingError::Measurement(inner) => inner.to_string(),
    };
    rusqlite::Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_ERROR), Some(message))
}

/// Protocol §5's frozen overhead-acceptance bound: the instrumented
/// build's mean per-operation wall time must not exceed the passthrough
/// build's by more than this fraction.
pub const OVERHEAD_BOUND: f64 = 0.02;

/// `(instrumented - passthrough) / passthrough` — positive when the
/// instrumented build costs more, as the timer calls it carries always
/// should. `f64::INFINITY` for a non-positive passthrough mean: a zero or
/// negative baseline makes the ratio meaningless, and silently returning 0
/// would hide that instead of failing the bound check below.
pub fn overhead_fraction(instrumented_mean_ns: f64, passthrough_mean_ns: f64) -> f64 {
    if passthrough_mean_ns <= 0.0 {
        return f64::INFINITY;
    }
    (instrumented_mean_ns - passthrough_mean_ns) / passthrough_mean_ns
}

/// Protocol §5: "the instrumented build's per-repetition `Σwall/N`...
/// must not exceed the uninstrumented's by more than 2%."
pub fn passes_overhead_bound(fraction: f64) -> bool {
    fraction <= OVERHEAD_BOUND
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
/// protocol §9's 200 ms sampling cadence is mechanically achievable, not a
/// pinned, isolated process (deferred to the result-bearing run's own
/// item: this pilot measures sampling cadence, not interference
/// isolation).
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

/// One cell's measured row in the pilot report: labels and wall-time
/// aggregates only (protocol §13).
#[derive(Debug, Clone)]
pub struct PilotCellRow {
    pub workload_label: String,
    pub operations_measured: usize,
    pub instrumented_mean_wall_ns: f64,
    pub passthrough_mean_wall_ns: f64,
    pub overhead_fraction: f64,
    pub passes_bound: bool,
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
    pub cells: Vec<PilotCellRow>,
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
        out.push_str("\noverhead bound (§5, frozen at 2%):\n");
        for cell in &self.cells {
            out.push_str(&format!(
                "  {:<14} ops={:<6} instrumented_mean_ns={:>10.1} passthrough_mean_ns={:>10.1} overhead={:>6.2}% bound={}\n",
                cell.workload_label,
                cell.operations_measured,
                cell.instrumented_mean_wall_ns,
                cell.passthrough_mean_wall_ns,
                cell.overhead_fraction * 100.0,
                if cell.passes_bound { "PASS" } else { "FAIL" },
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overhead_fraction_is_zero_when_both_builds_measure_the_same_mean() {
        assert_eq!(overhead_fraction(1000.0, 1000.0), 0.0);
    }

    #[test]
    fn overhead_fraction_is_positive_when_the_instrumented_build_costs_more() {
        // 2% over the passthrough baseline.
        assert!((overhead_fraction(1020.0, 1000.0) - 0.02).abs() < 1e-9);
    }

    #[test]
    fn overhead_fraction_is_infinite_for_a_non_positive_passthrough_baseline() {
        assert_eq!(overhead_fraction(1000.0, 0.0), f64::INFINITY);
        assert_eq!(overhead_fraction(1000.0, -5.0), f64::INFINITY);
    }

    #[test]
    fn passes_overhead_bound_accepts_exactly_two_percent() {
        assert!(passes_overhead_bound(0.02));
        assert!(!passes_overhead_bound(0.020001));
    }

    #[test]
    fn passes_overhead_bound_rejects_a_larger_overhead() {
        assert!(!passes_overhead_bound(0.05));
    }

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
    fn coverage_ratio_is_one_when_every_expected_sample_was_written() {
        assert_eq!(coverage_ratio(5, 5), 1.0);
    }

    #[test]
    fn coverage_ratio_reports_under_coverage() {
        assert_eq!(coverage_ratio(3, 6), 0.5);
    }

    #[test]
    fn coverage_ratio_treats_zero_expected_as_full_coverage() {
        assert_eq!(coverage_ratio(0, 0), 1.0);
    }

    #[test]
    fn pilot_workload_label_round_trips_through_parse() {
        for workload in [PilotWorkload::PointRead, PilotWorkload::PointUpdate] {
            assert_eq!(PilotWorkload::parse(workload.label()), Some(workload));
        }
        assert_eq!(PilotWorkload::parse("not-a-workload"), None);
    }

    #[test]
    fn rendered_report_never_contains_a_verdict_derivable_field_name() {
        let report = PilotReport {
            instrumented_binary_sha256: "a".repeat(64),
            passthrough_binary_sha256: "b".repeat(64),
            cells: vec![PilotCellRow {
                workload_label: "point-read".to_string(),
                operations_measured: 2000,
                instrumented_mean_wall_ns: 1020.0,
                passthrough_mean_wall_ns: 1000.0,
                overhead_fraction: 0.02,
                passes_bound: true,
            }],
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
        };
        let rendered = report.render();
        for forbidden in ["xsync", "xwrite", "remainder", "xSync", "xWrite"] {
            assert!(
                !rendered.contains(forbidden),
                "pilot output must never contain {forbidden:?} (protocol §13); got:\n{rendered}"
            );
        }
    }

    #[test]
    fn measure_cell_in_process_reports_a_positive_mean_wall_time_for_both_workloads() {
        for workload in [PilotWorkload::PointRead, PilotWorkload::PointUpdate] {
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
