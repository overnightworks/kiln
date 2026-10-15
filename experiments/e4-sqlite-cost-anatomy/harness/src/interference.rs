//! The interference gate's process side (protocol.md §9): judging a
//! monitor window against the frozen limits, and sampling the monitor in
//! a *separate* child process pinned away from the harness's own CPU.
//!
//! The pure reductions ([`judge_attempt`], parsing a sample line) are
//! unit-tested without a real process; spawning the actual child
//! ([`sample_interference`]) is the thin, untested adapter, exactly like
//! [`crate::preflight::run_real`] is for the preflight checks.

use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::monitor;

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
        return Err(io::Error::other(format!(
            "monitor-sample exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_sample_line(&stdout).ok_or_else(|| {
        io::Error::other(format!("could not parse monitor-sample output: {stdout:?}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine;

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
