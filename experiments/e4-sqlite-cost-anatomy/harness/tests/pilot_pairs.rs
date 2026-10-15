//! The pilot's pair logic (protocol.md §5, §8, §9, §13) against a
//! scripted [`MeasurementHost`]: which halves run in which order, when a
//! pair is rejected and retried, what an exhausted pair leaves in the
//! report, and what is persisted. The script says what each half's
//! samples look like, so none of it depends on this machine's real load;
//! the real child and monitor processes are exercised in
//! `pilot_child_processes.rs`.

use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use kiln_e4_harness::admission::{AdmittedBuilds, EvidenceFiles, reserve_evidence_files};
use kiln_e4_harness::builds::Build;
use kiln_e4_harness::cells::all_cells;
use kiln_e4_harness::comparison::{CellComparison, CellRow, Verdict, first_build};
use kiln_e4_harness::driver::OperationPlan;
use kiln_e4_harness::generators;
use kiln_e4_harness::host::{HalfRequest, HalfRun, MeasurementHost};
use kiln_e4_harness::interference::{TimedSample, judge_samples};
use kiln_e4_harness::machine;
use kiln_e4_harness::monitor::Sample;
use kiln_e4_harness::pilot::PilotReport;
use kiln_e4_harness::pilot_run::{run_pilot_comparisons, run_pilot_with};
use kiln_e4_harness::workloads::{Cell, Workload};

/// `count` monitor samples of a machine that stays quiet: one per
/// sampling interval, CPU 8 idle throughout, no scheduler wait, low load.
fn quiet_samples(count: u64) -> Vec<TimedSample> {
    let interval_ns = machine::SAMPLE_INTERVAL.as_nanos() as u64;
    let idle_ticks_per_interval =
        machine::SAMPLE_INTERVAL.as_millis() as u64 * machine::USER_HZ / 1000;
    (0..count)
        .map(|index| TimedSample {
            at_ns: index * interval_ns,
            sample: Sample {
                sibling_idle_ticks: index * idle_ticks_per_interval,
                pinned_wait_ns: 0,
                load_average_1m: 1.0,
            },
        })
        .collect()
}

/// A host that plays a script instead of running anything: halves
/// report a 1 % instrumentation overhead over `window_samples`
/// monitor samples, quiet except where the script says otherwise.
struct ScriptedHost {
    /// (cell, repetition index, build) halves that always show
    /// sibling-CPU activity.
    noisy_halves: Vec<(Cell, usize, Build)>,
    /// How many leading idle observations show a loaded machine.
    noisy_idle_observations: RefCell<usize>,
    /// Cells whose halves cannot be run at all.
    broken_cell: Option<Cell>,
    samples_per_half: u64,
    half_requests: RefCell<Vec<HalfRequest>>,
}

impl ScriptedHost {
    fn quiet() -> Self {
        ScriptedHost {
            noisy_halves: Vec::new(),
            noisy_idle_observations: RefCell::new(0),
            broken_cell: None,
            samples_per_half: 6,
            half_requests: RefCell::new(Vec::new()),
        }
    }
}

impl MeasurementHost for ScriptedHost {
    fn observe_idle_machine(&self) -> io::Result<Vec<TimedSample>> {
        let mut noisy = self.noisy_idle_observations.borrow_mut();
        let mut samples = quiet_samples(3);
        if *noisy > 0 {
            *noisy -= 1;
            samples[2].sample.load_average_1m = machine::MAX_LOAD_AVERAGE;
        }
        Ok(samples)
    }

    fn run_half(&self, request: &HalfRequest) -> io::Result<HalfRun> {
        self.half_requests.borrow_mut().push(request.clone());
        if self.broken_cell == Some(request.cell) {
            return Err(io::Error::other("the child could not be started"));
        }
        let mut samples = quiet_samples(self.samples_per_half);
        if self
            .noisy_halves
            .contains(&(request.cell, request.repetition_index, request.build))
        {
            // CPU 8 missed 5 idle ticks from the second sample on.
            for timed in &mut samples[1..] {
                timed.sample.sibling_idle_ticks -= 5;
            }
        }
        let mean_wall_ns = match request.build {
            Build::Instrumented => 1010.0,
            Build::Passthrough => 1000.0,
        };
        Ok(HalfRun {
            mean_wall_ns,
            started_ns: 0,
            finished_ns: 1_000_000_000,
            samples,
        })
    }
}

const INTERVAL: Duration = Duration::from_millis(200);
const TINY_PLAN: OperationPlan = OperationPlan {
    warmup: 1,
    timed: 2,
};

fn admitted() -> AdmittedBuilds {
    AdmittedBuilds {
        source_commit: "c0ffee".to_string(),
        instrumented_sha256: "a".repeat(64),
        passthrough_sha256: "b".repeat(64),
    }
}

struct Run {
    _dir: tempfile::TempDir,
    evidence: EvidenceFiles,
}

fn run() -> Run {
    let dir = tempfile::tempdir().expect("tempdir");
    let evidence = reserve_evidence_files(&dir.path().join("evidence")).expect("evidence");
    Run {
        _dir: dir,
        evidence,
    }
}

fn pilot(host: &ScriptedHost, run: &Run) -> io::Result<PilotReport> {
    run_pilot_with(
        host,
        admitted(),
        &run.evidence,
        INTERVAL,
        |_| TINY_PLAN,
        judge_samples,
    )
}

fn row<'a>(report: &'a PilotReport, label: &str) -> &'a CellRow {
    report
        .cells
        .iter()
        .find(|row| row.cell.label() == label)
        .unwrap_or_else(|| panic!("no row for {label}"))
}

fn gated_cells() -> Vec<Cell> {
    all_cells()
        .into_iter()
        .filter(|cell| cell.workload.is_gated())
        .collect()
}

fn lines_of(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

mod kiln_e4_11 {
    use super::*;

    #[test]
    fn the_pilot_reports_exactly_the_sixteen_cells_once_each() {
        let report = pilot(&ScriptedHost::quiet(), &run()).expect("a quiet pilot");

        let mut reported: Vec<String> = report.cells.iter().map(|r| r.cell.label()).collect();
        let mut expected: Vec<String> = all_cells().iter().map(Cell::label).collect();
        reported.sort();
        expected.sort();
        assert_eq!(reported, expected);
    }

    #[test]
    fn the_twelve_gated_cells_are_compared_and_the_four_read_cells_are_not() {
        let report = pilot(&ScriptedHost::quiet(), &run()).expect("a quiet pilot");

        let not_gated: Vec<&CellRow> = report
            .cells
            .iter()
            .filter(|row| row.comparison.is_not_gated())
            .collect();
        assert_eq!(not_gated.len(), 4);
        assert!(
            not_gated
                .iter()
                .all(|row| row.cell.workload == Workload::PointRead)
        );
        let compared = report.cells.len() - not_gated.len();
        assert_eq!(compared, 12);
    }

    #[test]
    fn a_cell_that_is_not_gated_cannot_be_run_as_a_gated_one() {
        let read_cell = all_cells()
            .into_iter()
            .find(|cell| cell.workload == Workload::PointRead)
            .unwrap();
        let run = run();

        let err = run_pilot_comparisons(
            &ScriptedHost::quiet(),
            &[read_cell],
            |_| TINY_PLAN,
            INTERVAL,
            judge_samples,
            &run.evidence,
        )
        .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}

mod kiln_e4_13 {
    use super::*;

    #[test]
    fn on_a_quiet_machine_every_gated_cell_completes_nine_pairs_and_the_pilot_passes() {
        let run = run();
        let report = pilot(&ScriptedHost::quiet(), &run).expect("a quiet pilot");

        for gated in gated_cells() {
            let CellComparison::Judged { pairs, summary } =
                &row(&report, &gated.label()).comparison
            else {
                panic!("{} must be judged", gated.label());
            };
            let indices: Vec<usize> = pairs.iter().map(|pair| pair.pair_index).collect();
            assert_eq!(
                indices,
                (1..=9).collect::<Vec<usize>>(),
                "{}",
                gated.label()
            );
            assert_eq!(summary.verdict, Verdict::Pass);
        }
        assert_eq!(report.rejected_attempts, 0);
        assert_eq!(report.exit_code(), 0);
        assert!(lines_of(&run.evidence.rejected_log).is_empty());
    }

    #[test]
    fn every_pair_reports_the_overhead_of_the_scripted_means() {
        let report = pilot(&ScriptedHost::quiet(), &run()).expect("a quiet pilot");

        let CellComparison::Judged { pairs, .. } =
            &row(&report, "point-update/small/full").comparison
        else {
            panic!("judged");
        };
        assert!(
            pairs
                .iter()
                .all(|pair| (pair.overhead_fraction - 0.01).abs() < 1e-9)
        );
    }
}

mod kiln_e4_12 {
    use super::*;

    #[test]
    fn both_halves_of_a_pair_run_the_same_trace_in_the_counterbalanced_order() {
        let host = ScriptedHost::quiet();
        pilot(&host, &run()).expect("a quiet pilot");

        let requests = host.half_requests.borrow();
        let first_cell = gated_cells()[0];
        let of_first_cell: Vec<&HalfRequest> = requests
            .iter()
            .filter(|request| request.cell == first_cell)
            .collect();
        assert_eq!(of_first_cell.len(), 18);
        for (pair, halves) in of_first_cell.chunks(2).enumerate() {
            let expected_first = first_build(pair);
            assert_eq!(halves[0].build, expected_first, "pair {}", pair + 1);
            assert_eq!(halves[1].build, expected_first.other(), "pair {}", pair + 1);
            assert_eq!(halves[0].seed, halves[1].seed);
            assert_eq!(halves[0].repetition_index, pair);
            assert_eq!(
                halves[0].seed,
                generators::seed_for(first_cell.index(), pair as u64)
            );
        }
    }

    #[test]
    fn pairs_run_in_the_drivers_interleaved_order_across_cells() {
        let host = ScriptedHost::quiet();
        pilot(&host, &run()).expect("a quiet pilot");

        let requests = host.half_requests.borrow();
        let first_pair_of_each_cell: Vec<(Cell, usize)> = requests
            .iter()
            .step_by(2)
            .take(12)
            .map(|request| (request.cell, request.repetition_index))
            .collect();
        let expected: Vec<(Cell, usize)> =
            gated_cells().into_iter().map(|cell| (cell, 0)).collect();
        assert_eq!(first_pair_of_each_cell, expected);
    }
}

mod kiln_e4_14 {
    use super::*;

    /// Pair 9 (repetition 8) of the first gated cell never gets a
    /// quiet first half (the instrumented build runs first in even
    /// repetitions).
    fn host_where_the_ninth_pair_of_one_cell_is_always_noisy() -> (ScriptedHost, Cell) {
        let noisy_cell = gated_cells()[0];
        let mut host = ScriptedHost::quiet();
        host.noisy_halves.push((noisy_cell, 8, Build::Instrumented));
        (host, noisy_cell)
    }

    #[test]
    fn a_cell_whose_ninth_pair_exhausts_its_retries_is_insufficient_with_its_eight_pairs_kept() {
        let (host, noisy_cell) = host_where_the_ninth_pair_of_one_cell_is_always_noisy();
        let run = run();
        let report = pilot(&host, &run).expect("the run continues past an exhausted pair");

        let CellComparison::InsufficientPairs { pairs, required } =
            &row(&report, &noisy_cell.label()).comparison
        else {
            panic!("the cell with an exhausted pair is insufficient");
        };
        let kept: Vec<usize> = pairs.iter().map(|pair| pair.pair_index).collect();
        assert_eq!(kept, (1..=8).collect::<Vec<usize>>());
        assert_eq!(*required, 9);
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn the_other_cells_still_run_to_completion_around_an_exhausted_pair() {
        let (host, noisy_cell) = host_where_the_ninth_pair_of_one_cell_is_always_noisy();
        let report = pilot(&host, &run()).expect("the run continues");

        for gated in gated_cells().into_iter().filter(|cell| *cell != noisy_cell) {
            assert!(
                matches!(
                    row(&report, &gated.label()).comparison,
                    CellComparison::Judged { .. }
                ),
                "{}",
                gated.label()
            );
        }
    }

    #[test]
    fn each_of_the_four_attempts_is_rejected_after_one_half_and_logged_with_its_reason() {
        let (host, noisy_cell) = host_where_the_ninth_pair_of_one_cell_is_always_noisy();
        let run = run();
        let report = pilot(&host, &run).expect("the run continues");

        assert_eq!(report.rejected_attempts, 4);
        let log = lines_of(&run.evidence.rejected_log);
        let rejections: Vec<&String> = log.iter().filter(|l| l.contains("reason=")).collect();
        assert_eq!(rejections.len(), 4);
        for line in rejections {
            assert!(line.contains(&noisy_cell.label()), "{line}");
            assert!(line.contains("SiblingNotIdle"), "{line}");
        }
        assert!(
            log.iter()
                .any(|line| line.starts_with("FAILED-INTERFERENCE")),
            "{log:?}"
        );
        let halves_run_for_the_noisy_pair = host
            .half_requests
            .borrow()
            .iter()
            .filter(|request| request.cell == noisy_cell && request.repetition_index == 8)
            .count();
        assert_eq!(
            halves_run_for_the_noisy_pair, 4,
            "a rejected half discards the pair: its second half never runs"
        );
    }

    #[test]
    fn a_noisy_second_half_rejects_the_pair_after_the_first_half_ran() {
        let noisy_cell = gated_cells()[0];
        let mut host = ScriptedHost::quiet();
        // Repetition 0 runs the instrumented build first, so the noisy
        // passthrough half is the second one.
        host.noisy_halves.push((noisy_cell, 0, Build::Passthrough));
        let run = run();

        let report = pilot(&host, &run).expect("the run continues");

        assert!(matches!(
            row(&report, &noisy_cell.label()).comparison,
            CellComparison::InsufficientPairs { .. }
        ));
        let log = lines_of(&run.evidence.rejected_log).join("\n");
        assert!(log.contains("passthrough half"), "{log}");
        let halves_of_the_noisy_pair = host
            .half_requests
            .borrow()
            .iter()
            .filter(|request| request.cell == noisy_cell && request.repetition_index == 0)
            .count();
        assert_eq!(
            halves_of_the_noisy_pair, 8,
            "four attempts of two halves each"
        );
    }
}

mod kiln_e4_25 {
    use super::*;

    #[test]
    fn a_noisy_machine_before_a_pair_rejects_that_attempt_and_the_retry_succeeds() {
        let host = ScriptedHost::quiet();
        *host.noisy_idle_observations.borrow_mut() = 2;
        let run = run();

        let report = pilot(&host, &run).expect("the retry budget absorbs two noisy starts");

        assert_eq!(report.rejected_attempts, 2);
        let log = lines_of(&run.evidence.rejected_log).join("\n");
        assert_eq!(
            log.matches("machine not quiet before the pair").count(),
            2,
            "{log}"
        );
        assert!(log.contains("LoadAverageTooHigh"), "{log}");
        assert!(
            row(&report, &gated_cells()[0].label())
                .comparison
                .allows_pilot_success(),
            "the first cell's first pair is retried and completes"
        );
    }

    #[test]
    fn no_half_starts_while_the_machine_is_not_quiet() {
        let host = ScriptedHost::quiet();
        *host.noisy_idle_observations.borrow_mut() = 1;

        pilot(&host, &run()).expect("one noisy start is retried");

        let first_request = host.half_requests.borrow()[0].clone();
        assert_eq!(first_request.repetition_index, 0);
        assert_eq!(
            host.half_requests
                .borrow()
                .iter()
                .filter(
                    |request| request.cell == first_request.cell && request.repetition_index == 0
                )
                .count(),
            2,
            "only the retried attempt ran its two halves"
        );
    }
}

mod kiln_e4_24 {
    use super::*;

    #[test]
    fn every_sample_of_every_phase_is_persisted_with_its_cell_pair_attempt_and_phase() {
        let run = run();
        let host = ScriptedHost::quiet();
        pilot(&host, &run).expect("a quiet pilot");

        let log = lines_of(&run.evidence.samples_log);
        let pairs = 12 * 9;
        // Per pair: 3 idle samples + 6 for each of the two halves.
        assert_eq!(log.len(), pairs * (3 + 6 + 6));
        let first = &log[0];
        for expected in [
            "cell=point-update/small/full",
            "pair=1",
            "attempt=0",
            "phase=idle",
            "at_ns=",
            "sibling_idle_ticks=",
        ] {
            assert!(first.contains(expected), "{expected} missing from {first}");
        }
        assert_eq!(
            log.iter()
                .filter(|line| line.contains("phase=instrumented"))
                .count(),
            pairs * 6
        );
        assert_eq!(
            log.iter()
                .filter(|line| line.contains("phase=passthrough"))
                .count(),
            pairs * 6
        );
    }

    #[test]
    fn the_samples_of_a_rejected_half_are_persisted_too() {
        let noisy_cell = gated_cells()[0];
        let mut host = ScriptedHost::quiet();
        host.noisy_halves.push((noisy_cell, 0, Build::Instrumented));
        let run = run();

        pilot(&host, &run).expect("the run continues");

        let log = lines_of(&run.evidence.samples_log);
        let noisy_first_attempt = log
            .iter()
            .filter(|line| {
                line.contains(&noisy_cell.label())
                    && line.contains("pair=1\t")
                    && line.contains("attempt=0")
                    && line.contains("phase=instrumented")
            })
            .count();
        assert_eq!(noisy_first_attempt, 6);
    }

    #[test]
    fn coverage_counts_the_intervals_the_monitor_covered_against_those_it_should_have() {
        let report = pilot(&ScriptedHost::quiet(), &run()).expect("a quiet pilot");

        // 216 halves, each a 1 s window at 200 ms (5 expected) covered
        // by 6 samples (5 intervals).
        assert_eq!(report.monitor.expected_samples, 216 * 5);
        assert_eq!(report.monitor.actual_samples, 216 * 5);
        assert_eq!(report.monitor.window, Duration::from_secs(216));
        assert_eq!(report.monitor.interval, INTERVAL);
    }

    #[test]
    fn a_monitor_that_missed_samples_shows_as_under_coverage() {
        let mut host = ScriptedHost::quiet();
        host.samples_per_half = 4;

        let report = pilot(&host, &run()).expect("a quiet pilot");

        assert_eq!(report.monitor.expected_samples, 216 * 5);
        assert_eq!(report.monitor.actual_samples, 216 * 3);
        assert!(
            report
                .render()
                .contains("monitor sample coverage: 648/1080 samples (60.0%)")
        );
    }
}

mod kiln_e4_07 {
    use super::*;

    #[test]
    fn a_half_that_cannot_run_stops_the_pilot_and_is_never_logged_as_interference() {
        let broken_cell = gated_cells()[3];
        let mut host = ScriptedHost::quiet();
        host.broken_cell = Some(broken_cell);
        let run = run();

        let err = pilot(&host, &run).expect_err("an unstartable child stops the pilot");

        assert!(err.to_string().contains(&broken_cell.label()), "{err}");
        assert!(err.to_string().contains("could not be started"), "{err}");
        assert!(lines_of(&run.evidence.rejected_log).is_empty());
    }
}
