//! The pilot's pair runs (protocol.md §5, §8, §9, §13): for every gated
//! cell, nine counterbalanced instrumented/passthrough pairs through the
//! run driver's interleaved order and retry budget, each half monitored
//! throughout, every sample persisted as evidence, and every rejected
//! attempt logged as it happens.
//!
//! The pair logic is written against [`MeasurementHost`], so its tests
//! script the host — quiet or noisy samples at chosen pairs — and never
//! depend on this machine's real load; [`run_pilot`] is the thin wiring
//! of the real [`ProcessHost`].

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::admission::{AdmittedBuilds, EvidenceFiles};
use crate::builds::Build;
use crate::cells::all_cells;
use crate::comparison::{CellComparison, CellRow, ComparisonMeasurement, first_build, judge_cell};
use crate::driver::{self, AttemptId, AttemptOutcome, OperationPlan};
use crate::generators;
use crate::host::{HalfRequest, HalfRun, MeasurementHost, ProcessHost, Snapshots};
use crate::interference::{self, TimedSample, judge_samples};
use crate::machine;
use crate::pilot::{MonitorCoverageRow, PerfStatus, PilotReport, expected_sample_count};
use crate::workloads::{Cell, Workload};

/// Monitor coverage across every measured window (protocol §13): the
/// samples §9 expects at its interval versus the intervals the monitor
/// actually covered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoverageTally {
    pub window: Duration,
    pub expected_samples: usize,
    pub actual_samples: usize,
}

impl CoverageTally {
    fn add_half(&mut self, half: &HalfRun, interval: Duration) {
        self.window += half.window();
        self.expected_samples += expected_sample_count(half.window(), interval);
        // n samples cover n - 1 intervals; the first only opens the first.
        self.actual_samples += half.samples.len().saturating_sub(1);
    }
}

/// What the pair runs produced.
#[derive(Debug)]
pub struct PilotComparisons {
    pub cells: Vec<CellRow>,
    pub rejected_attempts: usize,
    pub coverage: CoverageTally,
}

/// Runs one pair per attempt: checks the machine, then each half in its
/// counterbalanced order, judging every half's samples with `judge`
/// ([`judge_samples`] for the real pilot).
struct PairRunner<'a, H, J> {
    host: &'a H,
    judge: J,
    interval: Duration,
    samples_log: &'a Path,
    coverage: CoverageTally,
}

impl<H: MeasurementHost, J: Fn(&[TimedSample]) -> Option<String>> PairRunner<'_, H, J> {
    fn attempt(
        &mut self,
        cell: Cell,
        plan: OperationPlan,
        id: AttemptId,
    ) -> AttemptOutcome<ComparisonMeasurement> {
        match self.run_pair(cell, plan, id) {
            Ok(outcome) => outcome,
            Err(err) => AttemptOutcome::ExecutionFailed(format!(
                "measurement failed for {}: {err}",
                cell.label()
            )),
        }
    }

    fn run_pair(
        &mut self,
        cell: Cell,
        plan: OperationPlan,
        id: AttemptId,
    ) -> io::Result<AttemptOutcome<ComparisonMeasurement>> {
        let label = cell.label();
        let idle = self.host.observe_idle_machine()?;
        self.persist_samples(&label, id, "idle", &idle)?;
        if let Some(reason) = (self.judge)(&idle) {
            return Ok(AttemptOutcome::Rejected(format!(
                "{label}: machine not quiet before the pair: {reason}"
            )));
        }

        let seed = generators::seed_for(cell.index(), id.repetition_index as u64);
        let first = first_build(id.repetition_index);
        let (mut instrumented, mut passthrough) = (None, None);
        for build in [first, first.other()] {
            let half = self.host.run_half(&HalfRequest {
                cell,
                build,
                plan,
                repetition_index: id.repetition_index,
                seed,
            })?;
            self.coverage.add_half(&half, self.interval);
            self.persist_samples(&label, id, build.label(), &half.samples)?;
            if let Some(reason) = (self.judge)(&half.samples) {
                return Ok(AttemptOutcome::Rejected(format!(
                    "{label}: {} half: {reason}",
                    build.label()
                )));
            }
            match build {
                Build::Instrumented => instrumented = Some(half.mean_wall_ns),
                Build::Passthrough => passthrough = Some(half.mean_wall_ns),
            }
        }
        let (Some(instrumented), Some(passthrough)) = (instrumented, passthrough) else {
            return Err(io::Error::other("a pair ended without both halves"));
        };
        Ok(AttemptOutcome::Accepted(ComparisonMeasurement {
            operations_measured: plan.timed,
            instrumented_mean_wall_ns: instrumented,
            passthrough_mean_wall_ns: passthrough,
        }))
    }

    /// Appends every sample, with its cell, pair, attempt and phase, to
    /// the evidence log (protocol §13: samples written).
    fn persist_samples(
        &self,
        label: &str,
        id: AttemptId,
        phase: &str,
        samples: &[TimedSample],
    ) -> io::Result<()> {
        for sample in samples {
            driver::append_log_line(
                self.samples_log,
                &format!(
                    "cell={label}\tpair={}\tattempt={}\tphase={phase}\t{}",
                    id.repetition_index + 1,
                    id.attempt_index,
                    interference::render_sample_line(sample),
                ),
            )?;
        }
        Ok(())
    }
}

/// Runs the pairs of every cell in `gated_cells` (each must be a cell
/// the overhead bound gates) through [`driver::run_with_logging`] —
/// protocol §8/§9's interleaved order, retry budget and immediate
/// rejected-attempt log — and reports each cell judged, plus the
/// read-only cells of the pilot as not gated. Writes a terminal
/// `FAILED-INTERFERENCE` line for any pair that exhausted its budget; an
/// attempt that cannot execute is an error, not a log line. Each cell's
/// warm-up and timed counts come from `plan_for`: the pilot passes the
/// frozen protocol counts, and only a test of the mechanics passes
/// smaller ones. `judge` decides, from a window's samples, whether it was
/// interference-free: [`judge_samples`] for the pilot, and a fixed answer
/// for a test that must not depend on this machine's real load.
pub fn run_pilot_comparisons(
    host: &impl MeasurementHost,
    gated_cells: &[Cell],
    plan_for: impl Fn(Workload) -> OperationPlan,
    interval: Duration,
    judge: impl Fn(&[TimedSample]) -> Option<String>,
    evidence: &EvidenceFiles,
) -> io::Result<PilotComparisons> {
    if let Some(ungated) = gated_cells.iter().find(|cell| !cell.workload.is_gated()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not gated by protocol §5's overhead bound",
                ungated.label()
            ),
        ));
    }
    let mut runner = PairRunner {
        host,
        judge,
        interval,
        samples_log: &evidence.samples_log,
        coverage: CoverageTally::default(),
    };
    let run = run_driver(&mut runner, gated_cells, plan_for, &evidence.rejected_log)?;
    for (cell_index, repetition_index) in
        driver::exhausted_repetitions(&run.rejected_attempts, machine::MAX_RETRIES)
    {
        driver::append_log_line(
            &evidence.rejected_log,
            &driver::failed_interference_log_line(cell_index, repetition_index),
        )?;
    }
    Ok(PilotComparisons {
        cells: report_rows(gated_cells, &run.cell_outcomes),
        rejected_attempts: run.rejected_attempts.len(),
        coverage: runner.coverage,
    })
}

/// Hands every cell's pairs to the run driver, appending each rejected
/// attempt to `rejected_log` the moment it happens; an attempt that cannot
/// execute, or a log that cannot be written, ends the run with an error.
fn run_driver<H: MeasurementHost, J: Fn(&[TimedSample]) -> Option<String>>(
    runner: &mut PairRunner<'_, H, J>,
    gated_cells: &[Cell],
    plan_for: impl Fn(Workload) -> OperationPlan,
    rejected_log: &Path,
) -> io::Result<driver::DriverRun<ComparisonMeasurement>> {
    let mut rejected_log_error = None;
    let result = driver::run_with_logging(
        gated_cells.len(),
        machine::REPETITIONS_PER_CELL,
        machine::MAX_RETRIES,
        driver::real_clock,
        |id| {
            let cell = gated_cells[id.cell_index];
            runner.attempt(cell, plan_for(cell.workload), id)
        },
        |rejected| {
            if let Err(err) = driver::append_log_line(rejected_log, &rejected.to_log_line()) {
                rejected_log_error = Some(err);
            }
        },
    );
    match rejected_log_error {
        Some(err) => Err(err),
        None => result.map_err(io::Error::other),
    }
}

/// One row per cell the pilot reports: the gated cells judged from the
/// driver's outcomes, then every read-only cell as not gated.
fn report_rows(
    gated_cells: &[Cell],
    outcomes: &[driver::CellOutcome<ComparisonMeasurement>],
) -> Vec<CellRow> {
    let judged = gated_cells
        .iter()
        .zip(outcomes)
        .map(|(cell, outcome)| CellRow {
            cell: *cell,
            comparison: judge_cell(outcome),
        });
    let not_gated = all_cells()
        .into_iter()
        .filter(|cell| !cell.workload.is_gated())
        .map(|cell| CellRow {
            cell,
            comparison: CellComparison::NotGated,
        });
    judged.chain(not_gated).collect()
}

/// The pilot against a host: every gated cell's nine pairs, reported
/// (protocol §13). `admitted` names the vetted builds the report carries;
/// `judge` is as for [`run_pilot_comparisons`].
pub fn run_pilot_with(
    host: &impl MeasurementHost,
    admitted: AdmittedBuilds,
    evidence: &EvidenceFiles,
    interval: Duration,
    plan_for: impl Fn(Workload) -> OperationPlan,
    judge: impl Fn(&[TimedSample]) -> Option<String>,
) -> io::Result<PilotReport> {
    let gated_cells: Vec<Cell> = all_cells()
        .into_iter()
        .filter(|cell| cell.workload.is_gated())
        .collect();
    let run_start = Instant::now();
    let comparisons =
        run_pilot_comparisons(host, &gated_cells, plan_for, interval, judge, evidence)?;
    Ok(PilotReport {
        source_commit: admitted.source_commit,
        instrumented_binary_sha256: admitted.instrumented_sha256,
        passthrough_binary_sha256: admitted.passthrough_sha256,
        cells: comparisons.cells,
        run_duration: run_start.elapsed(),
        monitor: MonitorCoverageRow {
            interval,
            window: comparisons.coverage.window,
            expected_samples: comparisons.coverage.expected_samples,
            actual_samples: comparisons.coverage.actual_samples,
        },
        rejected_attempts: comparisons.rejected_attempts,
        perf: PerfStatus::Skipped {
            reason: "perf access not granted this session (protocol §7/§12)".to_string(),
        },
    })
}

/// The real pilot: loads and checkpoints both sizes once under a scratch
/// directory inside `output_dir` (the operator-chosen disk, never an
/// implicit temp directory), then runs [`run_pilot_with`] against real
/// child processes at the protocol's frozen operation counts. The scratch
/// directory is removed before returning.
pub fn run_pilot(
    instrumented: &Path,
    passthrough: &Path,
    admitted: AdmittedBuilds,
    evidence: &EvidenceFiles,
    output_dir: &Path,
    interval: Duration,
) -> io::Result<PilotReport> {
    let scratch = tempfile::Builder::new()
        .prefix("pilot-data-")
        .tempdir_in(output_dir)?;
    let host = ProcessHost {
        instrumented: instrumented.to_path_buf(),
        passthrough: passthrough.to_path_buf(),
        snapshots: Snapshots::prepare(scratch.path())?,
        scratch_dir: scratch.path().to_path_buf(),
        monitor_interval: interval,
    };
    run_pilot_with(
        &host,
        admitted,
        evidence,
        interval,
        driver::operation_plan,
        judge_samples,
    )
}
