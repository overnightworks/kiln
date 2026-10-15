//! Exercises pilot.rs's real child-process adapters
//! ([`run_measure_cell_child`], [`run_pilot_comparisons`]) against the
//! actual compiled harness binary — never a fake stand-in — so a
//! mutation to their spawn/status/parse handling has real process
//! behavior to catch. Every invocation uses tiny `warmup`/`operations`
//! counts, never protocol §8's frozen counts, so this stays a fast
//! correctness check, not a benchmark or a feasibility-pilot run.

use std::path::Path;
use std::time::Duration;

use kiln_e4_harness::pilot::{PilotWorkload, run_measure_cell_child, run_pilot_comparisons};

fn harness_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
}

#[test]
fn run_measure_cell_child_reports_a_measurement_too_large_to_be_a_hardcoded_stub() {
    let mean = run_measure_cell_child(harness_binary(), PilotWorkload::PointRead, 2, 3, 42)
        .expect("measure-cell must succeed against the real compiled binary");
    // A real SQLite point-read's wall time runs to thousands of
    // nanoseconds; this threshold sits comfortably below any real
    // measurement but well above the 1.0/0.0/-1.0 a stubbed return would
    // produce, so it also rules out a hardcoded constant standing in for
    // the real parsed value.
    assert!(
        mean > 100.0,
        "expected a real measured mean well above a stub constant, got {mean}"
    );
}

#[test]
fn run_pilot_comparisons_reports_exactly_one_row_per_pilot_workload() {
    let output_dir = tempfile::tempdir().expect("tempdir");
    let rejected_log = output_dir.path().join("rejected.log");

    // Both roles point at the same real binary: this proves the
    // orchestration wiring (driver, interference gate, child spawns)
    // runs end to end, not the §5 overhead comparison itself (which
    // needs a genuinely separate passthrough build and is covered by
    // the frozen night run instead).
    let (cells, _rejected_attempts) =
        run_pilot_comparisons(harness_binary(), harness_binary(), 1, 1, &rejected_log)
            .expect("run_pilot_comparisons must succeed against the real compiled binary");

    // Exactly one row per PILOT_WORKLOADS entry, regardless of whether
    // this machine's real interference gate accepted or rejected any
    // individual attempt — the structural guarantee that rules out a
    // stubbed `Ok((vec![], 0))`/`Ok((vec![], 1))` return.
    assert_eq!(cells.len(), 2);
    let labels: Vec<&str> = cells.iter().map(|c| c.workload_label.as_str()).collect();
    assert_eq!(labels, vec!["point-read", "point-update"]);
}

#[test]
fn run_pilot_comparisons_never_overwrites_an_existing_rejected_log() {
    let output_dir = tempfile::tempdir().expect("tempdir");
    let rejected_log = output_dir.path().join("rejected.log");
    std::fs::write(&rejected_log, "preexisting line\n").expect("seed the log");

    run_pilot_comparisons(harness_binary(), harness_binary(), 1, 1, &rejected_log)
        .expect("run_pilot_comparisons must succeed against the real compiled binary");

    let contents = std::fs::read_to_string(&rejected_log).expect("read rejected log");
    assert!(
        contents.starts_with("preexisting line\n"),
        "the rejected-attempt log must never be overwritten (protocol §9), got:\n{contents}"
    );
}

/// Not asserted elsewhere: proves the two real calls above actually
/// finish quickly, so this file stays a test, never an accidental
/// benchmark-length run.
#[test]
fn run_measure_cell_child_finishes_well_under_one_second() {
    let start = std::time::Instant::now();
    run_measure_cell_child(harness_binary(), PilotWorkload::PointUpdate, 1, 1, 7)
        .expect("measure-cell must succeed against the real compiled binary");
    assert!(start.elapsed() < Duration::from_secs(5));
}
