//! The interference monitor (protocol.md §9): samples `/proc/stat`'s
//! idle field for the sibling CPU, `/proc/schedstat`'s field 8 for the
//! pinned CPU, and `/proc/loadavg`'s 1-minute average, then judges a
//! window against the frozen limits. Parsing is pure and tested against
//! fixture text; only [`capture`] touches the real `/proc` files.

use std::fs;
use std::io;
use std::time::Duration;

use crate::machine;

/// One instant's reading of the three signals (protocol §9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// `/proc/stat`'s idle field for [`machine::SIBLING_CPU`], in
    /// USER_HZ ticks.
    pub sibling_idle_ticks: u64,
    /// `/proc/schedstat` field 8 for [`machine::PINNED_CPU`], in
    /// nanoseconds.
    pub pinned_wait_ns: u64,
    /// `/proc/loadavg`'s 1-minute average.
    pub load_average_1m: f64,
}

/// A window's verdict (protocol §9): any violation fails the repetition
/// and is appended to the rejected-attempt log by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Violation {
    /// The sibling CPU showed non-idle time during the window.
    SiblingNotIdle { idle_ticks_short: u64 },
    /// The pinned CPU's scheduler wait grew past the 1 ms limit.
    PinnedCpuWaited { delta_ns: u64 },
    /// `/proc/loadavg`'s 1-minute average reached the gate.
    LoadAverageTooHigh { observed: f64 },
}

/// Judges one sampling window against protocol §9's three limits.
/// `interval` is the wall time between `start` and `end`'s capture —
/// nominally [`machine::SAMPLE_INTERVAL`], passed explicitly so a test
/// (or a window that ran long under load) can use its own measured
/// duration.
pub fn evaluate(start: &Sample, end: &Sample, interval: Duration) -> Vec<Violation> {
    let mut violations = Vec::new();

    let expected_idle_ticks = interval.as_millis() as u64 * machine::USER_HZ / 1000;
    let idle_delta = end
        .sibling_idle_ticks
        .saturating_sub(start.sibling_idle_ticks);
    if idle_delta < expected_idle_ticks {
        violations.push(Violation::SiblingNotIdle {
            idle_ticks_short: expected_idle_ticks - idle_delta,
        });
    }

    let wait_delta = end.pinned_wait_ns.saturating_sub(start.pinned_wait_ns);
    if wait_delta > machine::MAX_SCHEDSTAT_WAIT_NS {
        violations.push(Violation::PinnedCpuWaited {
            delta_ns: wait_delta,
        });
    }

    if end.load_average_1m >= machine::MAX_LOAD_AVERAGE {
        violations.push(Violation::LoadAverageTooHigh {
            observed: end.load_average_1m,
        });
    }

    violations
}

/// Extracts `/proc/stat`'s idle field (the 4th whitespace-separated
/// token after the `cpu<N>` label) for one CPU.
pub fn parse_stat_idle_ticks(contents: &str, cpu: usize) -> Option<u64> {
    let prefix = format!("cpu{cpu} ");
    contents.lines().find_map(|line| {
        let rest = line.strip_prefix(&prefix)?;
        rest.split_whitespace().nth(3)?.parse().ok()
    })
}

/// Extracts one 1-indexed numbered field from `/proc/schedstat`'s
/// `cpu<N>` line (protocol §9 names field 8).
pub fn parse_schedstat_field(contents: &str, cpu: usize, field: usize) -> Option<u64> {
    let prefix = format!("cpu{cpu} ");
    contents.lines().find_map(|line| {
        let rest = line.strip_prefix(&prefix)?;
        rest.split_whitespace().nth(field - 1)?.parse().ok()
    })
}

/// Extracts `/proc/loadavg`'s first (1-minute) average.
pub fn parse_loadavg_1m(contents: &str) -> Option<f64> {
    contents.split_whitespace().next()?.parse().ok()
}

/// Reads the real `/proc` files and parses one [`Sample`].
pub fn capture() -> io::Result<Sample> {
    let stat = fs::read_to_string("/proc/stat")?;
    let schedstat = fs::read_to_string("/proc/schedstat")?;
    let loadavg = fs::read_to_string("/proc/loadavg")?;

    let missing =
        |what: &str| io::Error::new(io::ErrorKind::InvalidData, format!("missing {what}"));
    Ok(Sample {
        sibling_idle_ticks: parse_stat_idle_ticks(&stat, machine::SIBLING_CPU)
            .ok_or_else(|| missing("/proc/stat idle field"))?,
        pinned_wait_ns: parse_schedstat_field(&schedstat, machine::PINNED_CPU, 8)
            .ok_or_else(|| missing("/proc/schedstat field 8"))?,
        load_average_1m: parse_loadavg_1m(&loadavg).ok_or_else(|| missing("/proc/loadavg"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT_FIXTURE: &str = "\
cpu  100 0 200 9000 0 0 0 0 0 0
cpu0 50 0 100 4500 0 0 0 0 0 0
cpu8 50 0 100 4500 0 0 0 0 0 0
";

    const SCHEDSTAT_FIXTURE: &str = "\
version 15
timestamp 123456
cpu0 0 0 0 0 0 0 1000 2000 42
cpu2 0 0 0 0 0 0 1000 123456789 42
";

    const LOADAVG_FIXTURE: &str = "0.52 0.58 0.59 2/733 12345\n";

    #[test]
    fn parses_stat_idle_ticks_for_the_named_cpu() {
        assert_eq!(parse_stat_idle_ticks(STAT_FIXTURE, 8), Some(4500));
        assert_eq!(parse_stat_idle_ticks(STAT_FIXTURE, 1), None);
    }

    #[test]
    fn parses_schedstat_field_eight_for_the_named_cpu() {
        assert_eq!(
            parse_schedstat_field(SCHEDSTAT_FIXTURE, 2, 8),
            Some(123456789)
        );
        assert_eq!(parse_schedstat_field(SCHEDSTAT_FIXTURE, 9, 8), None);
    }

    #[test]
    fn parses_the_one_minute_load_average() {
        assert_eq!(parse_loadavg_1m(LOADAVG_FIXTURE), Some(0.52));
    }

    #[test]
    fn a_fully_idle_quiet_window_has_no_violations() {
        let interval = machine::SAMPLE_INTERVAL;
        let expected_ticks = interval.as_millis() as u64 * machine::USER_HZ / 1000;
        let start = Sample {
            sibling_idle_ticks: 1_000,
            pinned_wait_ns: 50_000,
            load_average_1m: 2.0,
        };
        let end = Sample {
            sibling_idle_ticks: 1_000 + expected_ticks,
            pinned_wait_ns: 50_500,
            load_average_1m: 2.1,
        };
        assert_eq!(evaluate(&start, &end, interval), Vec::new());
    }

    #[test]
    fn any_non_idle_time_on_the_sibling_cpu_is_a_violation() {
        let interval = machine::SAMPLE_INTERVAL;
        let expected_ticks = interval.as_millis() as u64 * machine::USER_HZ / 1000;
        let start = Sample {
            sibling_idle_ticks: 0,
            pinned_wait_ns: 0,
            load_average_1m: 1.0,
        };
        let end = Sample {
            // One tick short of fully idle: something else ran.
            sibling_idle_ticks: expected_ticks - 1,
            pinned_wait_ns: 0,
            load_average_1m: 1.0,
        };
        assert_eq!(
            evaluate(&start, &end, interval),
            vec![Violation::SiblingNotIdle {
                idle_ticks_short: 1
            }]
        );
    }

    #[test]
    fn a_pinned_cpu_wait_over_one_millisecond_is_a_violation() {
        let interval = machine::SAMPLE_INTERVAL;
        let expected_ticks = interval.as_millis() as u64 * machine::USER_HZ / 1000;
        let start = Sample {
            sibling_idle_ticks: 20,
            pinned_wait_ns: 0,
            load_average_1m: 1.0,
        };
        let end = Sample {
            sibling_idle_ticks: 20 + expected_ticks,
            pinned_wait_ns: machine::MAX_SCHEDSTAT_WAIT_NS + 1,
            load_average_1m: 1.0,
        };
        assert_eq!(
            evaluate(&start, &end, interval),
            vec![Violation::PinnedCpuWaited {
                delta_ns: machine::MAX_SCHEDSTAT_WAIT_NS + 1
            }]
        );
    }

    #[test]
    fn a_wait_at_exactly_the_limit_is_not_a_violation() {
        let interval = machine::SAMPLE_INTERVAL;
        let expected_ticks = interval.as_millis() as u64 * machine::USER_HZ / 1000;
        let start = Sample {
            sibling_idle_ticks: 20,
            pinned_wait_ns: 0,
            load_average_1m: 1.0,
        };
        let end = Sample {
            sibling_idle_ticks: 20 + expected_ticks,
            pinned_wait_ns: machine::MAX_SCHEDSTAT_WAIT_NS,
            load_average_1m: 1.0,
        };
        assert_eq!(evaluate(&start, &end, interval), Vec::new());
    }

    #[test]
    fn a_load_average_at_or_above_the_gate_is_a_violation() {
        let sample_at_gate = Sample {
            sibling_idle_ticks: 20,
            pinned_wait_ns: 0,
            load_average_1m: machine::MAX_LOAD_AVERAGE,
        };
        assert_eq!(
            evaluate(&sample_at_gate, &sample_at_gate, Duration::from_millis(0)),
            vec![Violation::LoadAverageTooHigh {
                observed: machine::MAX_LOAD_AVERAGE
            }]
        );
    }

    #[test]
    fn several_violations_in_one_window_are_all_reported() {
        let start = Sample {
            sibling_idle_ticks: 0,
            pinned_wait_ns: 0,
            load_average_1m: 20.0,
        };
        let end = Sample {
            sibling_idle_ticks: 0,
            pinned_wait_ns: machine::MAX_SCHEDSTAT_WAIT_NS + 1,
            load_average_1m: 20.0,
        };
        let violations = evaluate(&start, &end, machine::SAMPLE_INTERVAL);
        assert_eq!(violations.len(), 3);
    }
}
