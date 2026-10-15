//! The run driver (protocol.md §8/§9): sequences a set of cells into the
//! frozen interleaved order, runs each repetition's retry budget to either
//! a success or `FAILED-INTERFERENCE`, and reduces one cell's repetition
//! outcomes to whether it ran to completion. Pure and deterministic — no
//! SQLite, no process spawning, no real clock — so the mechanics are
//! tested without a live benchmark; the real per-repetition work (restore,
//! warm-up, timed window, interference sampling) is the caller's injected
//! `attempt` closure, read by [`run`] exactly like [`crate::preflight`]'s
//! checks are read by `run_real`.

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// One cell's repetition/attempt coordinates (protocol §8/§9), carried
/// alongside every rejected attempt and every log line so a reader can
/// locate it without re-deriving it from position in a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptId {
    pub cell_index: usize,
    pub repetition_index: usize,
    pub attempt_index: usize,
}

/// What one attempt of a repetition reports back to [`run`]: either it
/// produced a usable measurement, or it was discarded for a named reason
/// (protocol §9: an interference sample failing the window) and must be
/// retried from warm-up with the same input trace.
#[derive(Debug, Clone)]
pub enum AttemptOutcome<T> {
    Accepted(T),
    Rejected(String),
}

/// One repetition's final outcome after its retry budget (protocol §9:
/// "up to 3 retries (4 attempts)").
#[derive(Debug, Clone)]
pub enum RepetitionOutcome<T> {
    Succeeded(T),
    /// Every attempt in the retry budget was rejected: "marked
    /// `FAILED-INTERFERENCE` and logged" (protocol §9).
    FailedInterference,
}

/// One cell's outcome once every one of its repetitions has run (protocol
/// §9: "A cell's verdict... requires all 9 repetitions to succeed; one
/// exhausted repetition makes that cell `inconclusive — insufficient
/// repetitions`... instead of stopping the run").
#[derive(Debug, Clone)]
pub enum CellOutcome<T> {
    Completed(Vec<T>),
    InsufficientRepetitions { succeeded: usize, required: usize },
}

/// A discarded attempt, as protocol §9's rejected-attempt log records it:
/// "timestamp, cell, attempt, reason".
#[derive(Debug, Clone)]
pub struct RejectedAttempt {
    pub id: AttemptId,
    pub unix_time: std::time::Duration,
    pub reason: String,
}

impl RejectedAttempt {
    /// One tab-separated log line, machine-parseable and still readable —
    /// `data/rejected.log`'s format (protocol §9), never overwritten, only
    /// appended to (see [`append_log_line`]).
    pub fn to_log_line(&self) -> String {
        format!(
            "{}\tcell={}\trepetition={}\tattempt={}\treason={}",
            self.unix_time.as_secs_f64(),
            self.id.cell_index,
            self.id.repetition_index,
            self.id.attempt_index,
            self.reason,
        )
    }
}

/// Protocol §8.6's cell order: "repetition 1 of every cell, then
/// repetition 2, and so on" — repetition-major, cell-minor, so a load
/// spike biases every cell equally rather than concentrating on whichever
/// cell happens to run last.
pub fn interleaved_order(cell_count: usize, repetitions_per_cell: usize) -> Vec<(usize, usize)> {
    let mut order = Vec::with_capacity(cell_count * repetitions_per_cell);
    for repetition_index in 0..repetitions_per_cell {
        for cell_index in 0..cell_count {
            order.push((cell_index, repetition_index));
        }
    }
    order
}

/// Runs one repetition's retry budget (protocol §9): calls `attempt` for
/// attempt indices `0..=max_retries`, stopping at the first
/// [`AttemptOutcome::Accepted`]; every [`AttemptOutcome::Rejected`] is
/// reported to `on_rejected` before the next attempt (or, on the last
/// attempt, before returning [`RepetitionOutcome::FailedInterference`]).
pub fn run_repetition<T>(
    cell_index: usize,
    repetition_index: usize,
    max_retries: usize,
    now: impl Fn() -> std::time::Duration,
    mut attempt: impl FnMut(usize) -> AttemptOutcome<T>,
    mut on_rejected: impl FnMut(RejectedAttempt),
) -> RepetitionOutcome<T> {
    for attempt_index in 0..=max_retries {
        match attempt(attempt_index) {
            AttemptOutcome::Accepted(value) => return RepetitionOutcome::Succeeded(value),
            AttemptOutcome::Rejected(reason) => {
                on_rejected(RejectedAttempt {
                    id: AttemptId {
                        cell_index,
                        repetition_index,
                        attempt_index,
                    },
                    unix_time: now(),
                    reason,
                });
            }
        }
    }
    RepetitionOutcome::FailedInterference
}

/// The whole driver's result: every cell's outcome, in cell order, plus
/// every rejected attempt across the whole run, in the order it happened
/// (protocol §8.6's interleaved order, so earlier entries are from earlier
/// repetition indices, not earlier cells).
#[derive(Debug, Clone)]
pub struct DriverRun<T> {
    pub cell_outcomes: Vec<CellOutcome<T>>,
    pub rejected_attempts: Vec<RejectedAttempt>,
}

/// Runs every cell's every repetition in protocol §8.6's interleaved
/// order, each with its own retry budget (§9), and reduces each cell's
/// collected repetition outcomes to a [`CellOutcome`] — "the remaining
/// cells still run to completion" even once one repetition has already
/// exhausted its retries, so a late cell is never skipped because an
/// earlier one failed.
pub fn run<T>(
    cell_count: usize,
    repetitions_per_cell: usize,
    max_retries: usize,
    now: impl Fn() -> std::time::Duration,
    mut attempt: impl FnMut(AttemptId) -> AttemptOutcome<T>,
) -> DriverRun<T> {
    let mut per_cell_successes: Vec<Vec<T>> = (0..cell_count).map(|_| Vec::new()).collect();
    let mut rejected_attempts = Vec::new();

    for (cell_index, repetition_index) in interleaved_order(cell_count, repetitions_per_cell) {
        let outcome = run_repetition(
            cell_index,
            repetition_index,
            max_retries,
            &now,
            |attempt_index| {
                attempt(AttemptId {
                    cell_index,
                    repetition_index,
                    attempt_index,
                })
            },
            |rejected| rejected_attempts.push(rejected),
        );
        if let RepetitionOutcome::Succeeded(value) = outcome {
            per_cell_successes[cell_index].push(value);
        }
    }

    let cell_outcomes = per_cell_successes
        .into_iter()
        .map(|successes| {
            if successes.len() == repetitions_per_cell {
                CellOutcome::Completed(successes)
            } else {
                CellOutcome::InsufficientRepetitions {
                    succeeded: successes.len(),
                    required: repetitions_per_cell,
                }
            }
        })
        .collect();

    DriverRun {
        cell_outcomes,
        rejected_attempts,
    }
}

/// The real clock, for a real driver run: seconds since the Unix epoch, as
/// a plain [`std::time::Duration`] so [`run`]/[`run_repetition`] stay
/// clock-agnostic and testable against an injected one.
pub fn real_clock() -> std::time::Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// Appends one line to the rejected-attempt log, creating it if absent —
/// "never overwritten" (protocol §9): every call opens in append mode, so
/// an existing log's earlier lines are never touched.
pub fn append_log_line(path: &Path, line: &str) -> io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// Warm-up and timed operation counts for one workload (protocol §8.1/
/// §8.2), read from the frozen constants in [`crate::machine`] rather than
/// re-deriving them, so a driver caller has one place to ask instead of
/// matching on [`crate::workloads::Workload`] itself.
pub fn operation_plan(workload: crate::workloads::Workload) -> OperationPlan {
    use crate::machine;
    use crate::workloads::Workload;
    let timed = match workload {
        Workload::Insert => machine::OPERATION_COUNT_C,
        Workload::PointRead | Workload::PointUpdate | Workload::Transfer => {
            machine::OPERATION_COUNT_AB_D
        }
    };
    OperationPlan {
        warmup: machine::WARMUP_OPERATIONS,
        timed,
    }
}

/// One workload's warm-up/timed split (protocol §8.1/§8.2): warm-up
/// operations run first and are discarded from timing and the shim's
/// counters; the timed count then runs back-to-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationPlan {
    pub warmup: usize,
    pub timed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fixed_clock(seconds: u64) -> impl Fn() -> Duration {
        move || Duration::from_secs(seconds)
    }

    #[test]
    fn interleaved_order_runs_every_cells_repetition_zero_before_any_repetition_one() {
        let order = interleaved_order(3, 2);
        assert_eq!(order, vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]);
    }

    #[test]
    fn interleaved_order_is_empty_for_zero_cells_or_zero_repetitions() {
        assert_eq!(interleaved_order(0, 5), Vec::new());
        assert_eq!(interleaved_order(5, 0), Vec::new());
    }

    #[test]
    fn a_repetition_succeeds_on_its_first_attempt_without_any_rejection() {
        let mut rejections = Vec::new();
        let outcome = run_repetition(
            0,
            0,
            3,
            fixed_clock(10),
            |_attempt_index| AttemptOutcome::Accepted(42),
            |r| rejections.push(r),
        );
        assert!(matches!(outcome, RepetitionOutcome::Succeeded(42)));
        assert!(rejections.is_empty());
    }

    #[test]
    fn a_repetition_retries_after_a_rejection_and_then_succeeds() {
        let mut rejections = Vec::new();
        let mut calls = 0;
        let outcome = run_repetition(
            1,
            2,
            3,
            fixed_clock(10),
            |attempt_index| {
                calls += 1;
                if attempt_index == 0 {
                    AttemptOutcome::Rejected("load average too high".to_string())
                } else {
                    AttemptOutcome::Accepted(attempt_index)
                }
            },
            |r| rejections.push(r),
        );
        assert!(matches!(outcome, RepetitionOutcome::Succeeded(1)));
        assert_eq!(calls, 2);
        assert_eq!(rejections.len(), 1);
        assert_eq!(rejections[0].id.cell_index, 1);
        assert_eq!(rejections[0].id.repetition_index, 2);
        assert_eq!(rejections[0].id.attempt_index, 0);
        assert_eq!(rejections[0].reason, "load average too high");
    }

    #[test]
    fn a_repetition_that_exhausts_every_retry_fails_interference() {
        let mut rejections = Vec::new();
        let outcome: RepetitionOutcome<()> = run_repetition(
            0,
            0,
            3,
            fixed_clock(10),
            |_attempt_index| AttemptOutcome::Rejected("sibling not idle".to_string()),
            |r| rejections.push(r),
        );
        assert!(matches!(outcome, RepetitionOutcome::FailedInterference));
        // "up to 3 retries (4 attempts)" — attempts 0..=3.
        assert_eq!(rejections.len(), 4);
        assert_eq!(
            rejections
                .iter()
                .map(|r| r.id.attempt_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn a_cell_with_every_repetition_succeeding_is_completed() {
        let result: DriverRun<usize> = run(2, 3, 3, fixed_clock(0), |id| {
            AttemptOutcome::Accepted(id.repetition_index)
        });
        assert_eq!(result.rejected_attempts.len(), 0);
        assert_eq!(result.cell_outcomes.len(), 2);
        for outcome in &result.cell_outcomes {
            match outcome {
                CellOutcome::Completed(successes) => assert_eq!(successes, &vec![0, 1, 2]),
                CellOutcome::InsufficientRepetitions { .. } => panic!("expected Completed"),
            }
        }
    }

    #[test]
    fn a_cell_whose_one_repetition_exhausts_its_retries_is_insufficient_but_the_run_continues() {
        // Cell 0's repetition 1 (of 3) always rejects; every other
        // (cell, repetition) pair always accepts — proving the driver
        // keeps running the remaining cells and repetitions rather than
        // stopping the whole run (protocol §9: "the remaining cells still
        // run to completion").
        let result: DriverRun<usize> = run(2, 3, 3, fixed_clock(0), |id| {
            if id.cell_index == 0 && id.repetition_index == 1 {
                AttemptOutcome::Rejected("pinned cpu waited".to_string())
            } else {
                AttemptOutcome::Accepted(id.repetition_index)
            }
        });

        match &result.cell_outcomes[0] {
            CellOutcome::InsufficientRepetitions {
                succeeded,
                required,
            } => {
                assert_eq!(*succeeded, 2);
                assert_eq!(*required, 3);
            }
            CellOutcome::Completed(_) => panic!("expected cell 0 to be insufficient"),
        }
        match &result.cell_outcomes[1] {
            CellOutcome::Completed(successes) => assert_eq!(successes, &vec![0, 1, 2]),
            CellOutcome::InsufficientRepetitions { .. } => panic!("expected cell 1 completed"),
        }
        // 4 attempts (1 original + 3 retries), all rejected, for exactly
        // the one failing repetition.
        assert_eq!(result.rejected_attempts.len(), 4);
        assert!(
            result
                .rejected_attempts
                .iter()
                .all(|r| r.id.cell_index == 0 && r.id.repetition_index == 1)
        );
    }

    #[test]
    fn rejected_attempt_log_line_carries_timestamp_cell_attempt_and_reason() {
        let rejected = RejectedAttempt {
            id: AttemptId {
                cell_index: 4,
                repetition_index: 2,
                attempt_index: 1,
            },
            unix_time: Duration::from_secs(1_700_000_000),
            reason: "load average too high".to_string(),
        };
        let line = rejected.to_log_line();
        assert!(line.contains("1700000000"));
        assert!(line.contains("cell=4"));
        assert!(line.contains("repetition=2"));
        assert!(line.contains("attempt=1"));
        assert!(line.contains("reason=load average too high"));
    }

    #[test]
    fn append_log_line_creates_the_file_and_never_overwrites_earlier_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rejected.log");

        append_log_line(&path, "first line").unwrap();
        append_log_line(&path, "second line").unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "first line\nsecond line\n");
    }

    #[test]
    fn real_clock_reports_a_plausible_unix_time() {
        let now = real_clock();
        // Any time after this file was written; proves it reads the real
        // clock rather than returning a stub zero duration.
        assert!(now.as_secs() > 1_760_000_000);
    }

    #[test]
    fn operation_plan_uses_the_smaller_insert_count_only_for_insert() {
        use crate::machine;
        use crate::workloads::Workload;

        assert_eq!(
            operation_plan(Workload::Insert),
            OperationPlan {
                warmup: machine::WARMUP_OPERATIONS,
                timed: machine::OPERATION_COUNT_C,
            }
        );
        for workload in [
            Workload::PointRead,
            Workload::PointUpdate,
            Workload::Transfer,
        ] {
            assert_eq!(
                operation_plan(workload),
                OperationPlan {
                    warmup: machine::WARMUP_OPERATIONS,
                    timed: machine::OPERATION_COUNT_AB_D,
                }
            );
        }
    }
}
