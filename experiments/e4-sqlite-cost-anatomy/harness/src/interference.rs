//! The interference gate (protocol.md §9): a pinned monitor process
//! samples every 200 ms for as long as a measured half runs; consecutive
//! samples are judged against the frozen limits, and one sample outside
//! them rejects the half. Every sample is kept as evidence.
//!
//! The reductions here are pure and unit-tested without a process
//! ([`judge_samples`], the sample line, [`stream_samples`] against an
//! injected capture, clock and writer); the real monitor process is
//! spawned by [`crate::host`].

use std::convert::Infallible;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use crate::monitor;

/// One reading of the three signals, stamped with the system-wide
/// `CLOCK_MONOTONIC` time (nanoseconds) it was taken at, so a parent can
/// compare it with its own clock reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedSample {
    pub at_ns: u64,
    pub sample: monitor::Sample,
}

/// `CLOCK_MONOTONIC` in nanoseconds: the same clock in every process of
/// the machine, which the per-operation `wall` timings use too.
pub fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, correctly sized out-parameter, and
    // CLOCK_MONOTONIC exists on every Linux kernel this harness targets.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    assert_eq!(rc, 0, "clock_gettime(CLOCK_MONOTONIC) failed");
    nanoseconds(ts.tv_sec as u64, ts.tv_nsec as u64)
}

fn nanoseconds(seconds: u64, nanoseconds: u64) -> u64 {
    seconds * 1_000_000_000 + nanoseconds
}

/// Judges the samples of one window against protocol §9's frozen limits,
/// each consecutive pair over the time that actually passed between them,
/// plus the first sample's load average: `None` when every sample was
/// clean, otherwise one human-readable rejection reason. Fewer than two
/// samples leave nothing to judge a window by, which fails closed.
pub fn judge_samples(samples: &[TimedSample]) -> Option<String> {
    let [first, ..] = samples else {
        return Some("the monitor produced no samples".to_string());
    };
    if samples.len() < 2 {
        return Some("the monitor produced fewer than two samples".to_string());
    }
    // The first sample has no predecessor to measure a window against;
    // judged against itself over no time it can only show the load limit.
    let mut windows = vec![(first.sample, first.sample, Duration::ZERO)];
    windows.extend(samples.windows(2).map(|pair| {
        let elapsed = Duration::from_nanos(pair[1].at_ns.saturating_sub(pair[0].at_ns));
        (pair[0].sample, pair[1].sample, elapsed)
    }));
    let reasons: Vec<String> = windows
        .iter()
        .enumerate()
        .flat_map(|(position, (start, end, elapsed))| {
            monitor::evaluate(start, end, *elapsed)
                .into_iter()
                .map(move |violation| format!("sample {position}: {violation:?}"))
        })
        .collect();
    (!reasons.is_empty()).then(|| reasons.join("; "))
}

/// Parses the monitor process's one-line stdout contract back into a
/// [`TimedSample`]; a line missing a field is rejected rather than
/// defaulted.
pub fn parse_sample_line(line: &str) -> Option<TimedSample> {
    let field = |key: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(key))
    };
    Some(TimedSample {
        at_ns: field("at_ns=")?.parse().ok()?,
        sample: monitor::Sample {
            sibling_idle_ticks: field("sibling_idle_ticks=")?.parse().ok()?,
            pinned_wait_ns: field("pinned_wait_ns=")?.parse().ok()?,
            load_average_1m: field("load_average_1m=")?.parse().ok()?,
        },
    })
}

/// Renders one sample as [`parse_sample_line`]'s format — the monitor
/// process's own stdout, and the line the evidence log keeps.
pub fn render_sample_line(timed: &TimedSample) -> String {
    format!(
        "at_ns={} sibling_idle_ticks={} pinned_wait_ns={} load_average_1m={}",
        timed.at_ns,
        timed.sample.sibling_idle_ticks,
        timed.sample.pinned_wait_ns,
        timed.sample.load_average_1m
    )
}

/// The monitor process's loop: one sample line per `interval`, forever.
/// Only a failure ends it — the reader hanging up (a write error) or a
/// sample that cannot be captured — and that error is returned.
pub fn stream_samples(
    interval: Duration,
    mut capture: impl FnMut() -> io::Result<monitor::Sample>,
    mut now_ns: impl FnMut() -> u64,
    mut out: impl Write,
) -> io::Result<Infallible> {
    loop {
        let at_ns = now_ns();
        let sample = capture()?;
        writeln!(
            out,
            "{}",
            render_sample_line(&TimedSample { at_ns, sample })
        )?;
        out.flush()?;
        thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine;
    use crate::test_support::quiet_samples;

    const INTERVAL_NS: u64 = 200_000_000;

    fn quiet(index: u64) -> TimedSample {
        quiet_samples(index + 1)[index as usize]
    }

    fn quiet_run(samples: u64) -> Vec<TimedSample> {
        quiet_samples(samples)
    }

    mod kiln_e4_06 {
        use super::*;

        #[test]
        fn a_fully_idle_quiet_run_is_accepted() {
            assert_eq!(judge_samples(&quiet_run(6)), None);
        }

        #[test]
        fn a_sample_with_sibling_activity_rejects_the_run_naming_it() {
            let mut run = quiet_run(6);
            // From sample 3 on, CPU 8 missed 5 idle ticks.
            for timed in &mut run[3..] {
                timed.sample.sibling_idle_ticks -= 5;
            }

            let reason = judge_samples(&run).expect("sibling activity must be rejected");

            assert!(reason.contains("SiblingNotIdle"), "{reason}");
            assert!(reason.contains("sample 3"), "{reason}");
        }

        #[test]
        fn more_than_a_millisecond_of_pinned_cpu_wait_in_one_interval_rejects_the_run() {
            let mut run = quiet_run(4);
            for timed in &mut run[2..] {
                timed.sample.pinned_wait_ns = machine::MAX_SCHEDSTAT_WAIT_NS + 1;
            }

            let reason = judge_samples(&run).expect("a wait above the limit must be rejected");

            assert!(reason.contains("PinnedCpuWaited"), "{reason}");
        }

        #[test]
        fn a_wait_of_exactly_the_limit_is_still_clean() {
            let mut run = quiet_run(4);
            for timed in &mut run[2..] {
                timed.sample.pinned_wait_ns = machine::MAX_SCHEDSTAT_WAIT_NS;
            }

            assert_eq!(judge_samples(&run), None);
        }

        #[test]
        fn a_load_average_at_the_gate_rejects_the_run_whichever_sample_shows_it() {
            for position in [0, 1, 3] {
                let mut run = quiet_run(4);
                run[position].sample.load_average_1m = machine::MAX_LOAD_AVERAGE;

                let reason = judge_samples(&run)
                    .unwrap_or_else(|| panic!("load at the gate in sample {position}"));

                assert!(reason.contains("LoadAverageTooHigh"), "{reason}");
            }
        }

        #[test]
        fn a_longer_interval_expects_proportionally_more_idle_time() {
            let mut run = quiet_run(3);
            // Sample 2 arrives 100 ms late with CPU 8 idle for the extra
            // time too: still clean. Without the extra idle ticks the same
            // delay would not be.
            run[2].at_ns += 100_000_000;
            run[2].sample.sibling_idle_ticks += 10;
            assert_eq!(judge_samples(&run), None);

            run[2].sample.sibling_idle_ticks -= 10;
            assert!(judge_samples(&run).is_some());
        }

        #[test]
        fn fewer_than_two_samples_cannot_vouch_for_a_window() {
            assert!(judge_samples(&[]).is_some());
            assert!(judge_samples(&quiet_run(1)).is_some());
        }

        #[test]
        fn two_quiet_samples_are_a_complete_window() {
            assert_eq!(judge_samples(&quiet_run(2)), None);
        }
    }

    mod kiln_e4_08 {
        use super::*;

        #[test]
        fn sample_line_round_trips_through_render_and_parse() {
            let timed = TimedSample {
                at_ns: 123_456_789,
                sample: monitor::Sample {
                    sibling_idle_ticks: 100,
                    pinned_wait_ns: 5000,
                    load_average_1m: 2.5,
                },
            };
            assert_eq!(parse_sample_line(&render_sample_line(&timed)), Some(timed));
        }

        #[test]
        fn parse_sample_line_rejects_a_line_missing_a_field() {
            let line = render_sample_line(&quiet(1));
            for key in [
                "at_ns=",
                "sibling_idle_ticks=",
                "pinned_wait_ns=",
                "load_average_1m=",
            ] {
                let without: Vec<&str> = line
                    .split_whitespace()
                    .filter(|part| !part.starts_with(key))
                    .collect();
                assert_eq!(parse_sample_line(&without.join(" ")), None, "without {key}");
            }
        }

        #[test]
        fn parse_sample_line_rejects_a_malformed_value() {
            assert_eq!(
                parse_sample_line(
                    "at_ns=soon sibling_idle_ticks=1 pinned_wait_ns=1 load_average_1m=1.0"
                ),
                None
            );
        }
    }

    mod kiln_e4_24 {
        use super::*;

        /// A writer that hangs up after accepting `lines` complete lines.
        struct HangsUpAfter {
            lines: usize,
            written: Vec<u8>,
        }

        impl Write for HangsUpAfter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let newlines_so_far = self.written.iter().filter(|b| **b == b'\n').count();
                if newlines_so_far >= self.lines {
                    return Err(io::Error::from(io::ErrorKind::BrokenPipe));
                }
                self.written.extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        #[test]
        fn the_monitor_writes_one_stamped_sample_per_interval_until_its_reader_hangs_up() {
            let mut clock = 0;
            let mut out = HangsUpAfter {
                lines: 3,
                written: Vec::new(),
            };

            let ended = stream_samples(
                Duration::ZERO,
                || Ok(quiet(1).sample),
                || {
                    clock += INTERVAL_NS;
                    clock
                },
                &mut out,
            )
            .unwrap_err();

            assert_eq!(ended.kind(), io::ErrorKind::BrokenPipe);
            let written = String::from_utf8(out.written).unwrap();
            let stamps: Vec<u64> = written
                .lines()
                .map(|line| parse_sample_line(line).expect("a parseable line").at_ns)
                .collect();
            assert_eq!(stamps, vec![INTERVAL_NS, 2 * INTERVAL_NS, 3 * INTERVAL_NS]);
        }

        #[test]
        fn the_monitor_stops_with_the_error_of_a_sample_it_cannot_capture() {
            let mut out = Vec::new();

            let ended = stream_samples(
                Duration::ZERO,
                || Err(io::Error::from(io::ErrorKind::NotFound)),
                || 0,
                &mut out,
            )
            .unwrap_err();

            assert_eq!(ended.kind(), io::ErrorKind::NotFound);
            assert!(out.is_empty());
        }

        #[test]
        fn a_timespec_is_its_seconds_in_nanoseconds_plus_its_nanoseconds() {
            assert_eq!(nanoseconds(0, 0), 0);
            assert_eq!(nanoseconds(3, 500), 3_000_000_500);
        }

        #[test]
        fn the_monotonic_clock_counts_nanoseconds_and_never_runs_backwards() {
            let first = monotonic_ns();
            thread::sleep(Duration::from_millis(5));
            let later = monotonic_ns();
            assert!(
                later - first >= 5_000_000,
                "5 ms later the clock read only {} ns more",
                later - first
            );
        }
    }
}
