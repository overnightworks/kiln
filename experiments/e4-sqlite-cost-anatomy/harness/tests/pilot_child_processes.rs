//! Exercises the pilot's real process adapters ([`ProcessHost`]: the
//! measurement child and the pinned monitor process, protocol §9) against
//! the actual compiled harness binary — never a fake stand-in — so a
//! mutation to their spawn, pin, handshake and parse handling has real
//! process behaviour to catch. Every invocation uses tiny `warmup`/
//! `operations` counts, never protocol §8's frozen counts, and only the
//! small size, so this stays a fast correctness check, not a benchmark or
//! a feasibility-pilot run. What the machine's real load does to a pair is
//! never asserted here: the pair logic is tested against scripted hosts
//! (`src/pilot_run.rs`).

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use kiln_e4_harness::admission::{EvidenceFiles, reserve_evidence_files};
use kiln_e4_harness::builds::Build;
use kiln_e4_harness::comparison::CellComparison;
use kiln_e4_harness::driver::OperationPlan;
use kiln_e4_harness::host::{HalfRequest, HalfRun, MeasurementHost, ProcessHost, Snapshots};
use kiln_e4_harness::interference::TimedSample;
use kiln_e4_harness::machine;
use kiln_e4_harness::measure_protocol::MeasureCellRequest;
use kiln_e4_harness::pilot_run::run_pilot_comparisons;
use kiln_e4_harness::restore::prepare_pristine;
use kiln_e4_harness::workloads::{Cell, Size, Synchronous, Workload};

const TINY_PLAN: OperationPlan = OperationPlan {
    warmup: 2,
    timed: 5,
};
const FAST_INTERVAL: Duration = Duration::from_millis(50);

/// A judge that sees no interference whatever the samples say, so these
/// tests never depend on how loaded this machine happens to be.
fn machine_is_always_quiet(_samples: &[TimedSample]) -> Option<String> {
    None
}

fn harness_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
}

/// One small pristine snapshot shared by every test in this file.
fn small_snapshot() -> &'static Path {
    static SNAPSHOT: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &SNAPSHOT
        .get_or_init(|| {
            let dir = tempfile::tempdir().expect("tempdir");
            let pristine = dir.path().join("small").join("pristine");
            prepare_pristine(&pristine, Size::Small.row_count()).expect("prepare snapshot");
            (dir, pristine)
        })
        .1
}

struct Fixture {
    host: ProcessHost,
    _scratch: tempfile::TempDir,
}

fn fixture_with(passthrough: &Path, small_snapshot_dir: &Path) -> Fixture {
    let scratch = tempfile::tempdir().expect("tempdir");
    let host = ProcessHost {
        instrumented: harness_binary().to_path_buf(),
        passthrough: passthrough.to_path_buf(),
        snapshots: Snapshots {
            small: small_snapshot_dir.to_path_buf(),
            medium: small_snapshot_dir.to_path_buf(),
        },
        scratch_dir: scratch.path().to_path_buf(),
        monitor_interval: FAST_INTERVAL,
    };
    Fixture {
        host,
        _scratch: scratch,
    }
}

fn fixture() -> Fixture {
    fixture_with(harness_binary(), small_snapshot())
}

fn small_cell(workload: Workload) -> Cell {
    Cell {
        workload,
        size: Size::Small,
        synchronous: Synchronous::Full,
    }
}

fn half_request(workload: Workload, build: Build, plan: OperationPlan) -> HalfRequest {
    HalfRequest {
        cell: small_cell(workload),
        build,
        plan,
        repetition_index: 0,
        seed: 42,
    }
}

fn run_half(workload: Workload, plan: OperationPlan) -> HalfRun {
    fixture()
        .host
        .run_half(&half_request(workload, Build::Instrumented, plan))
        .expect("a half must run against the real compiled binary")
}

/// CPU ids in a `Cpus_allowed_list` value such as `0-1,3,9-11`.
fn cpus_in(list: &str) -> Vec<usize> {
    list.trim()
        .split(',')
        .flat_map(|part| match part.split_once('-') {
            Some((low, high)) => low.parse().unwrap()..=high.parse().unwrap(),
            None => part.parse().unwrap()..=part.parse().unwrap(),
        })
        .collect()
}

fn allowed_cpus_of(child: &Child) -> Vec<usize> {
    let status = fs::read_to_string(format!("/proc/{}/status", child.id())).expect("status");
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .expect("Cpus_allowed_list");
    cpus_in(list)
}

/// An executable shell script standing in for a harness binary.
fn stand_in(directory: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write stand-in");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// How many running processes were started with exactly these arguments
/// (after the program name).
fn processes_started_with(arguments: &[&str]) -> usize {
    fs::read_dir("/proc")
        .expect("/proc")
        .filter_map(|entry| fs::read(entry.ok()?.path().join("cmdline")).ok())
        .filter(|cmdline| {
            let parts: Vec<&[u8]> = cmdline
                .split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
                .collect();
            parts.len() >= arguments.len()
                && parts[parts.len() - arguments.len()..]
                    .iter()
                    .zip(arguments)
                    .all(|(part, argument)| *part == argument.as_bytes())
        })
        .count()
}

fn first_line_of(child: &mut Child) -> String {
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().expect("piped stdout"))
        .read_line(&mut line)
        .expect("read the child's first line");
    line
}

mod kiln_e4_07 {
    use super::*;

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn every_workload_runs_in_a_child_and_reports_a_real_mean_wall_time() {
        for workload in Workload::ALL {
            let run = run_half(workload, TINY_PLAN);
            // A real SQLite operation's wall time runs to thousands of
            // nanoseconds; this threshold rules out a stubbed 1.0/0.0/-1.0.
            assert!(
                run.mean_wall_ns > 100.0,
                "{workload:?}: expected a real measured mean, got {}",
                run.mean_wall_ns
            );
        }
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_half_runs_the_passthrough_build_through_the_same_child_protocol() {
        let run = fixture()
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect("the passthrough path of the host");

        assert!(run.mean_wall_ns > 100.0);
    }

    #[test]
    fn a_half_whose_binary_cannot_start_is_an_error() {
        let fixture = fixture_with(
            Path::new("/nonexistent-kiln-e4-passthrough-binary"),
            small_snapshot(),
        );

        let result = fixture.host.run_half(&half_request(
            Workload::PointUpdate,
            Build::Passthrough,
            TINY_PLAN,
        ));

        assert!(result.is_err());
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_half_whose_child_fails_is_an_error_quoting_what_the_child_said() {
        let missing_snapshot = tempfile::tempdir().expect("tempdir");
        let fixture = fixture_with(harness_binary(), &missing_snapshot.path().join("absent"));

        let message = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Instrumented,
                TINY_PLAN,
            ))
            .expect_err("a child that cannot restore its snapshot must fail the half")
            .to_string();

        assert!(
            message.contains("measure-cell for point-update"),
            "{message}"
        );
        assert!(message.contains("measurement failed"), "{message}");
    }

    #[test]
    fn a_child_that_never_says_ready_fails_the_half() {
        let dir = tempfile::tempdir().expect("tempdir");
        let child = stand_in(dir.path(), "silent", "echo hello");
        let fixture = fixture_with(&child, small_snapshot());

        let message = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect_err("a child that does not say ready cannot be measured")
            .to_string();

        assert!(message.contains("did not say ready"), "{message}");
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_child_that_exits_unsuccessfully_after_ready_fails_the_half_and_leaves_no_monitor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let child = stand_in(dir.path(), "crashing", "echo ready\nread go\nexit 3");
        let mut fixture = fixture_with(&child, small_snapshot());
        fixture.host.monitor_interval = Duration::from_millis(52);

        let message = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect_err("a child that crashes fails the half")
            .to_string();

        assert!(message.contains("exited with"), "{message}");
        assert_eq!(processes_started_with(&["monitor-stream", "52"]), 0);
    }

    #[test]
    fn a_waiting_child_is_stopped_when_its_monitor_cannot_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The duration is this test process's own: `sleep` keeps it through
        // exec, so no other process can carry the same arguments.
        let own_duration = format!("3100.{}", std::process::id());
        let waiting_child = stand_in(
            dir.path(),
            "waiting",
            &format!("echo ready\nexec sleep {own_duration}"),
        );
        let dead_monitor = stand_in(dir.path(), "dead-monitor", "exit 0");
        let mut fixture = fixture_with(&waiting_child, small_snapshot());
        fixture.host.instrumented = dead_monitor;

        let message = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect_err("a half without a monitor must not run")
            .to_string();

        assert!(message.contains("monitor"), "{message}");
        assert_eq!(
            processes_started_with(&["sleep", &own_duration]),
            0,
            "the child left waiting for go must not outlive the failed half"
        );
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_measurement_child_finishes_well_under_five_seconds() {
        let start = std::time::Instant::now();
        run_half(Workload::PointUpdate, TINY_PLAN);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_measurement_child_pins_itself_to_cpu_2_before_it_measures() {
        let scratch = tempfile::tempdir().expect("tempdir");
        let request = MeasureCellRequest {
            workload: Workload::PointRead,
            synchronous: Synchronous::Full,
            row_count: Size::Small.row_count(),
            plan: TINY_PLAN,
            seed: 1,
            pristine_dir: small_snapshot().to_path_buf(),
            scratch_dir: scratch.path().to_path_buf(),
        };
        let mut child = Command::new(harness_binary())
            .arg("measure-cell")
            .args(request.to_args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn the measurement child");

        let announced = first_line_of(&mut child);
        let allowed = allowed_cpus_of(&child);
        // The child waits for `go`; closing its stdin ends it unmeasured.
        drop(child.stdin.take());
        child.wait().expect("child exits");

        assert_eq!(announced.trim(), "ready");
        assert_eq!(allowed, vec![machine::PINNED_CPU]);
    }
}

mod kiln_e4_05 {
    use super::*;

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_half_ends_its_window_at_the_childs_done_stamp_not_when_the_child_exits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slow_to_clean_up = stand_in(
            dir.path(),
            "slow-to-clean-up",
            "echo ready\nread go\necho 'done monotonic_ns=7'\nread release\nsleep 1\necho mean_wall_ns=5",
        );
        let fixture = fixture_with(&slow_to_clean_up, small_snapshot());

        let run = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect("a child that follows the protocol is measured");

        assert_eq!(run.finished_ns, 7);
        assert_eq!(run.mean_wall_ns, 5.0);
        assert!(
            run.samples.last().expect("a sample after the window").at_ns > 7,
            "the monitor still samples once past the stamp"
        );
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_child_is_held_at_its_done_stamp_until_the_monitor_has_covered_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let released_at = dir.path().join("released_at");
        let busy_after_release = stand_in(
            dir.path(),
            "busy-after-release",
            &format!(
                "now() {{ perl -MTime::HiRes=clock_gettime,CLOCK_MONOTONIC -e 'printf \"%d\\n\", clock_gettime(CLOCK_MONOTONIC) * 1e9'; }}\n\
                 echo ready\nread go\necho \"done monotonic_ns=$(now)\"\n\
                 read release\nnow > '{released}'\n\
                 perl -e '$until = time + 1; 1 while time < $until'\necho mean_wall_ns=5",
                released = released_at.display()
            ),
        );
        let fixture = fixture_with(&busy_after_release, small_snapshot());

        let run = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect("a child that follows the protocol is measured");

        let released_ns: u64 = fs::read_to_string(&released_at)
            .expect("the child was released")
            .trim()
            .parse()
            .expect("an instant");
        let boundary = run.samples.last().expect("a boundary sample").at_ns;
        assert!(
            boundary > run.finished_ns,
            "the last sample covers the stamp"
        );
        assert!(
            boundary < released_ns,
            "no sample is taken once the child is free to check and clean up"
        );
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_child_that_reports_a_mean_without_a_done_stamp_fails_the_half() {
        let dir = tempfile::tempdir().expect("tempdir");
        let no_stamp = stand_in(
            dir.path(),
            "no-stamp",
            "echo ready\nread go\necho mean_wall_ns=5",
        );
        let fixture = fixture_with(&no_stamp, small_snapshot());

        let message = fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Passthrough,
                TINY_PLAN,
            ))
            .expect_err("a window with no end stamp cannot be closed")
            .to_string();

        assert!(message.contains("wrote no done line"), "{message}");
    }
}

mod kiln_e4_24 {
    use super::*;

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn the_monitor_samples_across_the_whole_window_of_a_half() {
        let plan = OperationPlan {
            warmup: 20,
            timed: 200,
        };
        let run = run_half(Workload::Insert, plan);

        let first = run.samples.first().expect("a sample");
        let last = run.samples.last().expect("a sample");
        assert!(
            first.at_ns <= run.started_ns,
            "the first sample precedes the window"
        );
        assert!(
            last.at_ns > run.finished_ns,
            "the last sample follows the window"
        );
        assert!(run.samples.len() >= 2);
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn consecutive_samples_are_at_least_one_interval_apart_and_in_order() {
        let plan = OperationPlan {
            warmup: 20,
            timed: 200,
        };
        let run = run_half(Workload::Insert, plan);

        for pair in run.samples.windows(2) {
            let gap = Duration::from_nanos(pair[1].at_ns - pair[0].at_ns);
            assert!(gap >= FAST_INTERVAL, "samples only {gap:?} apart");
        }
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn the_idle_machine_is_observed_for_one_interval_between_two_samples() {
        let samples = fixture()
            .host
            .observe_idle_machine()
            .expect("observe the idle machine");

        assert_eq!(samples.len(), 2);
        let gap = Duration::from_nanos(samples[1].at_ns - samples[0].at_ns);
        assert!(gap >= FAST_INTERVAL, "{gap:?}");
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn no_monitor_process_outlives_the_half_or_the_idle_observation_it_sampled() {
        let mut fixture = fixture();
        fixture.host.monitor_interval = Duration::from_millis(51);

        fixture
            .host
            .observe_idle_machine()
            .expect("observe the idle machine");
        fixture
            .host
            .run_half(&half_request(
                Workload::PointUpdate,
                Build::Instrumented,
                TINY_PLAN,
            ))
            .expect("a half");

        assert_eq!(processes_started_with(&["monitor-stream", "51"]), 0);
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn the_monitor_process_is_pinned_away_from_the_measured_core_and_its_sibling() {
        let mut monitor = Command::new(harness_binary())
            .args(["monitor-stream", "50"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn the monitor");

        let first_sample = first_line_of(&mut monitor);
        let allowed = allowed_cpus_of(&monitor);
        monitor.kill().expect("stop the monitor");
        monitor.wait().expect("reap the monitor");

        assert!(
            first_sample.contains("sibling_idle_ticks="),
            "{first_sample}"
        );
        assert!(!allowed.is_empty());
        for measured in [machine::PINNED_CPU, machine::SIBLING_CPU] {
            assert!(
                !allowed.contains(&measured),
                "the monitor may run on {allowed:?}, which includes CPU {measured}"
            );
        }
    }

    #[test]
    fn the_monitor_refuses_a_missing_or_zero_interval() {
        for arguments in [vec!["monitor-stream"], vec!["monitor-stream", "0"]] {
            let output = Command::new(harness_binary())
                .args(&arguments)
                .output()
                .expect("spawn");
            assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        }
    }
}

mod kiln_e4_04 {
    use super::*;

    fn evidence() -> (tempfile::TempDir, EvidenceFiles) {
        let dir = tempfile::tempdir().expect("tempdir");
        let files = reserve_evidence_files(&dir.path().join("evidence")).expect("evidence");
        (dir, files)
    }

    fn update_cell() -> Cell {
        small_cell(Workload::PointUpdate)
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn a_comparison_whose_passthrough_build_cannot_start_is_an_error_not_interference() {
        let (_dir, evidence) = evidence();
        let fixture = fixture_with(
            Path::new("/nonexistent-kiln-e4-passthrough-binary"),
            small_snapshot(),
        );

        let result = run_pilot_comparisons(
            &fixture.host,
            &[update_cell()],
            |_| TINY_PLAN,
            FAST_INTERVAL,
            machine_is_always_quiet,
            &evidence,
        );

        let message = result
            .expect_err("an unstartable build must stop the pilot")
            .to_string();
        assert!(
            message.contains("measurement failed for point-update/small/full")
                && message.contains(
                    "could not start the passthrough build at /nonexistent-kiln-e4-passthrough-binary",
                )
                && message.contains("No such file or directory"),
            "got: {message}"
        );
        assert!(
            fs::read_to_string(&evidence.rejected_log)
                .unwrap()
                .is_empty(),
            "an execution failure is not interference and must not reach the rejected-attempt log"
        );
    }

    #[test]
    #[ignore = "needs the measuring machine's CPU topology (CPU 2, its sibling 8, a further CPU), protocol §9; run-pilot-night.sh runs it there"]
    fn real_children_and_a_real_monitor_feed_the_pair_logic_end_to_end() {
        let (_dir, evidence) = evidence();
        fs::write(&evidence.rejected_log, "preexisting line\n").expect("seed the log");

        let comparisons = run_pilot_comparisons(
            &fixture().host,
            &[update_cell()],
            |_| TINY_PLAN,
            FAST_INTERVAL,
            machine_is_always_quiet,
            &evidence,
        )
        .expect("the real hosts must run the pairs");

        let row = comparisons
            .cells
            .iter()
            .find(|row| row.cell == update_cell())
            .expect("a row for the cell");
        let CellComparison::Judged { pairs, .. } = &row.comparison else {
            panic!("with no interference rejected, the cell completes its nine pairs");
        };
        assert_eq!(pairs.len(), machine::REPETITIONS_PER_CELL);
        assert_eq!(comparisons.rejected_attempts, 0);
        assert!(
            !fs::read_to_string(&evidence.samples_log)
                .unwrap()
                .is_empty()
        );
        assert!(
            fs::read_to_string(&evidence.rejected_log)
                .unwrap()
                .starts_with("preexisting line\n"),
            "the rejected-attempt log must never be overwritten (protocol §9)"
        );
        assert!(comparisons.coverage.actual_samples > 0);
    }
}
