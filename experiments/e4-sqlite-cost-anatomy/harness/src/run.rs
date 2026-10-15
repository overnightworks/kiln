//! The result-bearing run path's mechanics (protocol.md §8/§9) — not its
//! verdict: §10's thresholds, shares, and output tables are a separate,
//! not-yet-built slice. This module drives one (workload, size,
//! synchronous) cell's frozen repetition count through
//! [`crate::driver::run_with_logging`], each repetition isolated in its
//! own child process of this harness binary (protocol §8.4), gated by
//! the interference monitor (protocol §9) sampled in a *separate* child
//! process pinned away from the harness's own CPU, and logs every
//! rejection — including an exhausted repetition's `FAILED-INTERFERENCE`
//! — append-only and immediately.
//!
//! Like [`crate::pilot`], the pure reductions here ([`judge_attempt`],
//! parsing a child's reported measurement) are unit-tested without a
//! real process; spawning the actual child processes
//! ([`sample_interference`], [`run_repetition_child`]) is the thin,
//! untested adapter, exactly like [`crate::preflight::run_real`] is for
//! the preflight checks.

use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::driver::{self, AttemptOutcome};
use crate::machine;
use crate::monitor;

/// One repetition's raw timing sums (protocol §5/§8.5), exactly as a
/// `run-repetition` child reports them — before any §10 aggregation
/// (medians, shares, confidence bounds), which stays out of this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepetitionMeasurement {
    pub xsync_ns: u64,
    pub xwrite_ns: u64,
    pub remainder_ns: u64,
    pub wall_ns: u64,
    pub cpu_ns: u64,
}

impl RepetitionMeasurement {
    /// One `key=value` line, tab-separated — the `run-repetition` child's
    /// stdout contract and this module's raw-data-file row format, so
    /// both the writer and [`parse_line`] agree on one owner.
    pub fn to_line(&self) -> String {
        format!(
            "xsync_ns={}\txwrite_ns={}\tremainder_ns={}\twall_ns={}\tcpu_ns={}",
            self.xsync_ns, self.xwrite_ns, self.remainder_ns, self.wall_ns, self.cpu_ns
        )
    }

    /// Parses [`to_line`]'s format back. Order-independent: each field is
    /// found by its key, not by position, so the line can carry extra
    /// whitespace-separated fields a future caller adds without breaking
    /// this one.
    pub fn parse_line(line: &str) -> Option<Self> {
        let field = |key: &str| {
            line.split('\t')
                .find_map(|part| part.strip_prefix(key))
                .and_then(|v| v.trim().parse::<u64>().ok())
        };
        Some(RepetitionMeasurement {
            xsync_ns: field("xsync_ns=")?,
            xwrite_ns: field("xwrite_ns=")?,
            remainder_ns: field("remainder_ns=")?,
            wall_ns: field("wall_ns=")?,
            cpu_ns: field("cpu_ns=")?,
        })
    }
}

/// Judges one attempt's bracketing interference samples against protocol
/// §9's frozen limits, reducing [`monitor::evaluate`]'s violation list to
/// a single human-readable rejection reason — `None` when the window was
/// clean.
pub fn judge_attempt(
    start: &monitor::Sample,
    end: &monitor::Sample,
    elapsed: Duration,
) -> Option<String> {
    let violations = monitor::evaluate(start, end, elapsed);
    if violations.is_empty() {
        None
    } else {
        Some(
            violations
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

/// Parses the `monitor-sample` child's one-line stdout contract back into
/// a [`monitor::Sample`] — the counterpart [`sample_interference`]
/// reads, kept pure and separately testable from the real process spawn.
pub fn parse_sample_line(line: &str) -> Option<monitor::Sample> {
    let field_u64 = |key: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(key))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let field_f64 = |key: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(key))
            .and_then(|v| v.parse::<f64>().ok())
    };
    Some(monitor::Sample {
        sibling_idle_ticks: field_u64("sibling_idle_ticks=")?,
        pinned_wait_ns: field_u64("pinned_wait_ns=")?,
        load_average_1m: field_f64("load_average_1m=")?,
    })
}

/// Renders one [`monitor::Sample`] as [`parse_sample_line`]'s format —
/// the `monitor-sample` child's own stdout, written by
/// `main.rs::run_monitor_sample`.
pub fn render_sample_line(sample: &monitor::Sample) -> String {
    format!(
        "sibling_idle_ticks={} pinned_wait_ns={} load_average_1m={}",
        sample.sibling_idle_ticks, sample.pinned_wait_ns, sample.load_average_1m
    )
}

fn child_io_error(what: &str, output: &std::process::Output) -> io::Error {
    io::Error::other(format!(
        "{what} exited with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Spawns one interference sample as an isolated child process of this
/// harness binary, pinned to the complement of
/// `{PINNED_CPU, SIBLING_CPU}` (protocol §9) by the child itself (the
/// `monitor-sample` subcommand) rather than by this parent — a genuinely
/// separate, independently pinned process, never a thread inside the
/// harness's own pinned process. The thin, untested adapter around
/// [`monitor::capture`]; [`parse_sample_line`] carries the tested logic.
pub fn sample_interference(harness_binary: &Path) -> io::Result<monitor::Sample> {
    let output = Command::new(harness_binary)
        .arg("monitor-sample")
        .output()?;
    if !output.status.success() {
        return Err(child_io_error("monitor-sample", &output));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_sample_line(&stdout).ok_or_else(|| {
        io::Error::other(format!("could not parse monitor-sample output: {stdout:?}"))
    })
}

/// One cell's fixed `run-repetition` parameters — every argument the
/// child needs except the per-attempt `seed` (protocol §2.1: each
/// attempt gets its own seed) — bundled so
/// [`run_repetition_child`], [`run_gated_attempt`], and [`run_cell`] stay
/// under clippy's argument-count limit instead of repeating eight
/// positional parameters each.
#[derive(Debug, Clone, Copy)]
pub struct CellSpec<'a> {
    pub workload: &'a str,
    pub size: &'a str,
    pub synchronous: &'a str,
    pub pristine_dir: &'a Path,
    pub work_dir: &'a Path,
    pub warmup: usize,
    pub operations: usize,
}

/// Spawns one repetition attempt as an isolated child process of this
/// harness binary (protocol §8.4: "each repetition... its own process"),
/// pinned to [`machine::PINNED_CPU`] by the child itself (the
/// `run-repetition` subcommand) — never merely called in-process, since
/// §9's monitor/harness CPU isolation only holds if the repetition's own
/// process is actually the one pinned there.
pub fn run_repetition_child(
    harness_binary: &Path,
    spec: &CellSpec,
    seed: u64,
) -> io::Result<RepetitionMeasurement> {
    let output = Command::new(harness_binary)
        .arg("run-repetition")
        .arg(spec.workload)
        .arg(spec.size)
        .arg(spec.synchronous)
        .arg(spec.pristine_dir)
        .arg(spec.work_dir)
        .arg(spec.warmup.to_string())
        .arg(spec.operations.to_string())
        .arg(seed.to_string())
        .output()?;
    if !output.status.success() {
        return Err(child_io_error("run-repetition", &output));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    RepetitionMeasurement::parse_line(&stdout).ok_or_else(|| {
        io::Error::other(format!("could not parse run-repetition output: {stdout:?}"))
    })
}

/// One gated attempt: samples interference before and after spawning the
/// repetition child, judges the bracketed window, and reduces to
/// [`AttemptOutcome`]. A child process failing to spawn, run, or report a
/// parseable measurement is itself a rejection — exactly the kind of
/// unexpected condition protocol §9's retry budget exists to absorb,
/// never a reason to abort the whole run.
pub fn run_gated_attempt(
    harness_binary: &Path,
    spec: &CellSpec,
    seed: u64,
) -> AttemptOutcome<RepetitionMeasurement> {
    let start = match sample_interference(harness_binary) {
        Ok(sample) => sample,
        Err(err) => {
            return AttemptOutcome::Rejected(format!(
                "could not sample interference before the attempt: {err}"
            ));
        }
    };
    let window_start = Instant::now();
    let measurement = run_repetition_child(harness_binary, spec, seed);
    let elapsed = window_start.elapsed();
    let end = match sample_interference(harness_binary) {
        Ok(sample) => sample,
        Err(err) => {
            return AttemptOutcome::Rejected(format!(
                "could not sample interference after the attempt: {err}"
            ));
        }
    };
    if let Some(reason) = judge_attempt(&start, &end, elapsed) {
        return AttemptOutcome::Rejected(reason);
    }
    match measurement {
        Ok(value) => AttemptOutcome::Accepted(value),
        Err(err) => AttemptOutcome::Rejected(format!("repetition child failed: {err}")),
    }
}

/// Runs one cell's full frozen repetition budget (protocol §8.4:
/// [`machine::REPETITIONS_PER_CELL`] repetitions, [`machine::MAX_RETRIES`]
/// retries each) through [`driver::run_with_logging`], writing every
/// rejection immediately to `rejected_log_path` — including, once the run
/// finishes, a terminal `FAILED-INTERFERENCE` line for any repetition
/// that exhausted its retry budget (protocol §9).
pub fn run_cell(
    harness_binary: &Path,
    spec: &CellSpec,
    rejected_log_path: &Path,
) -> io::Result<driver::DriverRun<RepetitionMeasurement>> {
    let mut log_err = None;
    let result = driver::run_with_logging(
        1,
        machine::REPETITIONS_PER_CELL,
        machine::MAX_RETRIES,
        driver::real_clock,
        |id| {
            run_gated_attempt(
                harness_binary,
                spec,
                crate::generators::seed_for(0, id.repetition_index as u64),
            )
        },
        |rejected| {
            if let Err(err) = driver::append_log_line(rejected_log_path, &rejected.to_log_line()) {
                log_err = Some(err);
            }
        },
    );
    if let Some(err) = log_err {
        return Err(err);
    }
    for (cell_index, repetition_index) in
        driver::exhausted_repetitions(&result.rejected_attempts, machine::MAX_RETRIES)
    {
        driver::append_log_line(
            rejected_log_path,
            &driver::failed_interference_log_line(cell_index, repetition_index),
        )?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod kiln_e4_06 {
        use super::*;

        #[test]
        fn judge_attempt_accepts_a_fully_idle_quiet_window() {
            let interval = machine::SAMPLE_INTERVAL;
            let expected_ticks = interval.as_millis() as u64 * machine::USER_HZ / 1000;
            let start = monitor::Sample {
                sibling_idle_ticks: 0,
                pinned_wait_ns: 0,
                load_average_1m: 1.0,
            };
            let end = monitor::Sample {
                sibling_idle_ticks: expected_ticks,
                pinned_wait_ns: 0,
                load_average_1m: 1.0,
            };
            assert_eq!(judge_attempt(&start, &end, interval), None);
        }

        #[test]
        fn judge_attempt_rejects_with_a_reason_naming_the_violation() {
            let start = monitor::Sample {
                sibling_idle_ticks: 0,
                pinned_wait_ns: 0,
                load_average_1m: machine::MAX_LOAD_AVERAGE,
            };
            let reason = judge_attempt(&start, &start, Duration::from_millis(0))
                .expect("a load average at the gate must be rejected");
            assert!(reason.contains("LoadAverageTooHigh"));
        }
    }

    mod kiln_e4_08 {
        use super::*;

        #[test]
        fn repetition_measurement_round_trips_through_its_own_line_format() {
            let measurement = RepetitionMeasurement {
                xsync_ns: 123,
                xwrite_ns: 456,
                remainder_ns: 789,
                wall_ns: 1368,
                cpu_ns: 42,
            };
            let line = measurement.to_line();
            assert_eq!(RepetitionMeasurement::parse_line(&line), Some(measurement));
        }

        #[test]
        fn repetition_measurement_parse_line_rejects_a_line_missing_a_field() {
            assert_eq!(
                RepetitionMeasurement::parse_line("xsync_ns=1\txwrite_ns=2"),
                None
            );
        }

        #[test]
        fn sample_line_round_trips_through_render_and_parse() {
            let sample = monitor::Sample {
                sibling_idle_ticks: 100,
                pinned_wait_ns: 5000,
                load_average_1m: 2.5,
            };
            let line = render_sample_line(&sample);
            assert_eq!(parse_sample_line(&line), Some(sample));
        }

        #[test]
        fn parse_sample_line_rejects_a_line_missing_a_field() {
            assert_eq!(parse_sample_line("sibling_idle_ticks=1"), None);
        }
    }
}
