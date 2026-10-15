//! Exercises the pilot's real child-process adapters
//! ([`run_measure_cell_child`], [`run_pilot_comparisons`]) against the
//! actual compiled harness binary — never a fake stand-in — so a
//! mutation to their spawn/status/parse handling has real process
//! behavior to catch. Every invocation uses tiny `warmup`/`operations`
//! counts, never protocol §8's frozen counts, so this stays a fast
//! correctness check, not a benchmark or a feasibility-pilot run.

use std::path::Path;
use std::sync::OnceLock;

use kiln_e4_harness::comparison::{CellComparison, WorkloadComparison, run_pilot_comparisons};
use kiln_e4_harness::driver::OperationPlan;
use kiln_e4_harness::machine::REPETITIONS_PER_CELL;
use kiln_e4_harness::pilot::{PilotWorkload, run_measure_cell_child};

fn harness_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
}

const TINY_PLAN: OperationPlan = OperationPlan {
    warmup: 1,
    timed: 1,
};

/// Both builds' roles point at the same real binary: this proves the
/// orchestration wiring (driver, interference gate, child spawns) runs
/// end to end, not the §5 overhead comparison itself (which needs a
/// genuinely separate passthrough build and is covered by the night run).
fn run_tiny_pilot(rejected_log: &Path) -> Vec<WorkloadComparison> {
    run_pilot_comparisons(
        harness_binary(),
        harness_binary(),
        |_| TINY_PLAN,
        rejected_log,
    )
    .expect("run_pilot_comparisons must succeed against the real compiled binary")
    .0
}

/// One shared tiny pilot for the tests that only read its rows: nine
/// interference-gated pairs are the slow part of this file.
fn shared_tiny_pilot() -> &'static [WorkloadComparison] {
    static CELLS: OnceLock<Vec<WorkloadComparison>> = OnceLock::new();
    CELLS.get_or_init(|| {
        let output_dir = tempfile::tempdir().expect("tempdir");
        run_tiny_pilot(&output_dir.path().join("rejected.log"))
    })
}

mod kiln_e4_07 {
    use super::*;

    #[test]
    fn a_measurement_child_reports_a_real_mean_wall_time() {
        let mean = run_measure_cell_child(harness_binary(), PilotWorkload::PointRead, 2, 3, 42)
            .expect("measure-cell must succeed against the real compiled binary");
        // A real SQLite point-read's wall time runs to thousands of
        // nanoseconds; this threshold rules out a stubbed 1.0/0.0/-1.0.
        assert!(
            mean > 100.0,
            "expected a real measured mean well above a stub constant, got {mean}"
        );
    }

    #[test]
    fn a_comparison_whose_passthrough_build_cannot_start_is_an_error_not_interference() {
        let output_dir = tempfile::tempdir().expect("tempdir");
        let rejected_log = output_dir.path().join("rejected.log");

        let result = run_pilot_comparisons(
            harness_binary(),
            Path::new("/nonexistent-kiln-e4-passthrough-binary"),
            |_| TINY_PLAN,
            &rejected_log,
        );

        let message = result
            .expect_err("an unstartable build must stop the pilot")
            .to_string();
        assert!(
            message.contains("measurement child failed"),
            "got: {message}"
        );
        assert!(
            !rejected_log.exists(),
            "an execution failure is not interference and must not reach the rejected-attempt log"
        );
    }

    #[test]
    fn a_measurement_child_finishes_well_under_five_seconds() {
        let start = std::time::Instant::now();
        run_measure_cell_child(harness_binary(), PilotWorkload::PointUpdate, 1, 1, 7)
            .expect("measure-cell must succeed against the real compiled binary");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }
}

mod kiln_e4_04 {
    use super::*;

    #[test]
    fn the_rejected_attempt_log_is_never_overwritten() {
        let output_dir = tempfile::tempdir().expect("tempdir");
        let rejected_log = output_dir.path().join("rejected.log");
        std::fs::write(&rejected_log, "preexisting line\n").expect("seed the log");

        run_tiny_pilot(&rejected_log);

        let contents = std::fs::read_to_string(&rejected_log).expect("read rejected log");
        assert!(
            contents.starts_with("preexisting line\n"),
            "the rejected-attempt log must never be overwritten (protocol §9), got:\n{contents}"
        );
    }
}

mod kiln_e4_11 {
    use super::*;

    #[test]
    fn the_pilot_reports_the_write_workload_gated_and_the_read_workload_not_gated() {
        let cells = shared_tiny_pilot();

        let read = cells
            .iter()
            .find(|cell| cell.workload == PilotWorkload::PointRead)
            .expect("a row for the read workload");
        assert_eq!(read.comparison, CellComparison::NotGated);
        let write = cells
            .iter()
            .find(|cell| cell.workload == PilotWorkload::PointUpdate)
            .expect("a row for the write workload");
        assert_ne!(write.comparison, CellComparison::NotGated);
        assert_eq!(cells.len(), 2);
    }
}

mod kiln_e4_13 {
    use super::*;

    #[test]
    fn a_judged_write_cell_holds_exactly_the_protocols_nine_pairs() {
        let cells = shared_tiny_pilot();

        // The machine's real interference gate may reject every attempt of
        // a pair; then the cell is insufficient and has no pairs to count.
        if let Some(CellComparison::Judged { pairs, .. }) = cells
            .iter()
            .find(|cell| cell.workload == PilotWorkload::PointUpdate)
            .map(|cell| &cell.comparison)
        {
            assert_eq!(pairs.len(), REPETITIONS_PER_CELL);
        }
    }
}
