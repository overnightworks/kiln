//! The text protocol between the pilot and a `measure-cell` child
//! (protocol.md §9, §13): the arguments that describe one measured half,
//! and the lines the two sides write each other — `ready`, `go`, the
//! `done` stamp that closes the monitored window, the `release` that
//! lets the child leave it, and the one result line. Everything crosses the process boundary as plain text and is
//! read back exactly or rejected (KILN-E4-08); the parsing is pure, so a
//! malformed field is tested without spawning anything.

use std::path::PathBuf;

use crate::driver::OperationPlan;
use crate::workloads::{Synchronous, Workload};

/// Everything one measurement child needs, and nothing it must derive:
/// the workload and setting, the row count of the snapshot it restores
/// (its size, protocol §3), the warm-up and timed counts, the seed of the
/// attempt's input trace (§2.1), the pristine snapshot to restore, and a
/// directory to keep its working copy in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasureCellRequest {
    pub workload: Workload,
    pub synchronous: Synchronous,
    pub row_count: u64,
    pub plan: OperationPlan,
    pub seed: u64,
    pub pristine_dir: PathBuf,
    pub scratch_dir: PathBuf,
}

impl MeasureCellRequest {
    /// The `measure-cell` subcommand's arguments.
    pub fn to_args(&self) -> Vec<String> {
        vec![
            self.workload.label().to_string(),
            self.synchronous.label().to_string(),
            self.row_count.to_string(),
            self.plan.warmup.to_string(),
            self.plan.timed.to_string(),
            self.seed.to_string(),
            self.pristine_dir.display().to_string(),
            self.scratch_dir.display().to_string(),
        ]
    }

    /// Reads [`to_args`](Self::to_args)'s arguments back; the error names
    /// what was wrong.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let [
            workload,
            synchronous,
            row_count,
            warmup,
            timed,
            seed,
            pristine_dir,
            scratch_dir,
        ] = args
        else {
            return Err(format!("expected 8 arguments, got {}", args.len()));
        };
        let number = |name: &str, value: &str| {
            value
                .parse::<u64>()
                .map_err(|_| format!("{name} must be a non-negative integer, got {value:?}"))
        };
        Ok(MeasureCellRequest {
            workload: Workload::parse(workload)
                .ok_or_else(|| format!("unknown workload {workload:?}"))?,
            synchronous: Synchronous::parse(synchronous)
                .ok_or_else(|| format!("unknown synchronous setting {synchronous:?}"))?,
            row_count: number("row count", row_count)?,
            plan: OperationPlan {
                warmup: number("warm-up", warmup)? as usize,
                timed: number("operations", timed)? as usize,
            },
            seed: number("seed", seed)?,
            pristine_dir: PathBuf::from(pristine_dir),
            scratch_dir: PathBuf::from(scratch_dir),
        })
    }
}

/// What the child writes once its snapshot is restored and its
/// statements prepared.
pub const READY_LINE: &str = "ready";
/// What the parent writes to let the child start measuring.
pub const GO_LINE: &str = "go";
/// Prefix of the line that closes the monitored window: the child's
/// `CLOCK_MONOTONIC` instant right after its last operation.
pub const DONE_PREFIX: &str = "done monotonic_ns=";
/// What the parent writes once the monitor's boundary sample covers the
/// `done` stamp: only then may the child check its repetition and clean up,
/// so none of that can reach a judged sample.
pub const RELEASE_LINE: &str = "release";
/// Prefix of the child's one result line.
pub const MEAN_WALL_PREFIX: &str = "mean_wall_ns=";

/// Reads the instant out of the child's `done` line — pure, so a
/// malformed line is tested without spawning anything.
pub fn parse_done_ns(line: &str) -> Option<u64> {
    line.trim().strip_prefix(DONE_PREFIX)?.parse().ok()
}

/// Parses the child's result line out of its stdout — pure, so a
/// malformed or missing line is tested without spawning anything.
pub fn parse_mean_wall_ns(stdout: &str) -> Option<f64> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(MEAN_WALL_PREFIX))
        .and_then(|value| value.trim().parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod kiln_e4_08 {
        use super::*;

        fn request() -> MeasureCellRequest {
            MeasureCellRequest {
                workload: Workload::Transfer,
                synchronous: Synchronous::Normal,
                row_count: 10_000_000,
                plan: OperationPlan {
                    warmup: 200,
                    timed: 2000,
                },
                seed: 12_345,
                pristine_dir: PathBuf::from("/data/medium/pristine"),
                scratch_dir: PathBuf::from("/data/scratch"),
            }
        }

        #[test]
        fn a_request_round_trips_through_its_arguments() {
            assert_eq!(
                MeasureCellRequest::parse(&request().to_args()),
                Ok(request())
            );
        }

        #[test]
        fn a_request_with_a_missing_or_malformed_argument_is_rejected() {
            let mut missing = request().to_args();
            missing.pop();
            assert!(MeasureCellRequest::parse(&missing).is_err());
            for (position, bad) in [(0, "range-scan"), (1, "off"), (2, "many"), (5, "-1")] {
                let mut args = request().to_args();
                args[position] = bad.to_string();
                assert!(MeasureCellRequest::parse(&args).is_err(), "{bad}");
            }
        }

        #[test]
        fn a_done_line_reads_back_as_its_instant_and_nothing_else_does() {
            assert_eq!(parse_done_ns("done monotonic_ns=1234\n"), Some(1234));
            for line in ["done", "done monotonic_ns=soon", "mean_wall_ns=7", ""] {
                assert_eq!(parse_done_ns(line), None, "{line:?}");
            }
        }

        #[test]
        fn parse_mean_wall_ns_reads_the_measure_cell_contract() {
            assert_eq!(parse_mean_wall_ns("mean_wall_ns=1234.5\n"), Some(1234.5));
            assert_eq!(parse_mean_wall_ns("ready\nmean_wall_ns=7\n"), Some(7.0));
        }

        #[test]
        fn parse_mean_wall_ns_is_none_for_output_missing_the_line() {
            assert_eq!(parse_mean_wall_ns("something else entirely\n"), None);
            assert_eq!(parse_mean_wall_ns("mean_wall_ns=fast\n"), None);
        }
    }
}
