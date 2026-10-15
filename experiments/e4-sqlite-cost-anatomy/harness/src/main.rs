//! The harness binary: `preflight` (issue #19), `measure-cell` (the
//! pilot's per-build, per-workload measurement primitive, issue #28), and
//! `pilot` (the feasibility pilot, protocol.md §13, issue #28). The
//! result-bearing run itself is not wired up yet.

use std::path::PathBuf;
use std::time::Duration;

use kiln_e4_harness::pilot::{self, MonitorCoverageRow, PerfStatus, PilotCellRow, PilotReport};
use kiln_e4_harness::preflight::{self, CheckResult};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("preflight") => run_preflight(&args[2..]),
        Some("measure-cell") => run_measure_cell(&args[2..]),
        Some("pilot") => run_pilot(&args[2..]),
        _ => {
            eprintln!("usage: kiln-e4-harness <preflight|measure-cell|pilot> [args]");
            std::process::exit(2);
        }
    }
}

fn run_preflight(args: &[String]) {
    let report = preflight::run_real();

    for (name, check) in [
        ("rustc version", &report.rustc_version),
        ("sqlite version", &report.sqlite_version),
        ("build environment", &report.build_env),
        ("CPU siblings", &report.cpu_siblings),
        ("shim VFS", &report.shim_vfs),
    ] {
        match check {
            CheckResult::Pass => println!("PASS  {name}"),
            CheckResult::Fail(reason) => println!("FAIL  {name}: {reason}"),
        }
    }

    if !report.all_passed() {
        eprintln!("preflight failed closed; no frozen run may proceed");
        std::process::exit(1);
    }

    if let Some(manifest_path) = args.first() {
        match preflight::build_manifest() {
            Ok(manifest) => {
                if let Err(err) =
                    preflight::write_manifest(&PathBuf::from(manifest_path), &manifest)
                {
                    eprintln!("could not write manifest to {manifest_path}: {err}");
                    std::process::exit(1);
                }
                println!(
                    "manifest written to {manifest_path} (binary sha256: {})",
                    manifest.binary_sha256
                );
            }
            Err(err) => {
                eprintln!("could not build manifest: {err}");
                std::process::exit(1);
            }
        }
    }
}

/// `measure-cell <workload> <warmup> <operations> <seed>`: runs one
/// in-process measurement and prints only its mean wall time in
/// nanoseconds — the one number [`pilot::measure_cell_in_process`]
/// produces that is safe for another process to read back (protocol
/// §13's mechanics-only output).
fn run_measure_cell(args: &[String]) {
    let [workload, warmup, operations, seed] = args else {
        eprintln!("usage: kiln-e4-harness measure-cell <workload> <warmup> <operations> <seed>");
        std::process::exit(2);
    };
    let Some(workload) = pilot::PilotWorkload::parse(workload) else {
        eprintln!("unknown workload {workload:?}; expected point-read or point-update");
        std::process::exit(2);
    };
    let (warmup, operations, seed) = match (
        warmup.parse::<usize>(),
        operations.parse::<usize>(),
        seed.parse::<u64>(),
    ) {
        (Ok(w), Ok(o), Ok(s)) => (w, o, s),
        _ => {
            eprintln!("warmup, operations, and seed must all be non-negative integers");
            std::process::exit(2);
        }
    };

    match pilot::measure_cell_in_process(workload, warmup, operations, seed) {
        Ok(measured) => println!("mean_wall_ns={}", measured.mean_wall_ns),
        Err(err) => {
            eprintln!("measurement failed: {err}");
            std::process::exit(1);
        }
    }
}

struct PilotArgs {
    output_dir: PathBuf,
    passthrough_binary: PathBuf,
    warmup: usize,
    operations: usize,
    monitor_interval: Duration,
}

fn parse_pilot_args(args: &[String]) -> PilotArgs {
    let mut output_dir = None;
    let mut passthrough_binary = None;
    let mut warmup = 200;
    let mut operations = 2000;
    let mut monitor_interval_ms = 200;

    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut next = || {
            iter.next().unwrap_or_else(|| {
                eprintln!("{flag} needs a value");
                std::process::exit(2);
            })
        };
        match flag.as_str() {
            "--output-dir" => output_dir = Some(PathBuf::from(next())),
            "--passthrough-binary" => passthrough_binary = Some(PathBuf::from(next())),
            "--warmup" => warmup = next().parse().unwrap_or_else(|_| invalid_value(flag)),
            "--operations" => operations = next().parse().unwrap_or_else(|_| invalid_value(flag)),
            "--monitor-interval-ms" => {
                monitor_interval_ms = next().parse().unwrap_or_else(|_| invalid_value(flag));
            }
            other => {
                eprintln!("unknown pilot argument {other:?}");
                std::process::exit(2);
            }
        }
    }

    let Some(output_dir) = output_dir else {
        eprintln!("pilot needs --output-dir <dir>");
        std::process::exit(2);
    };
    let Some(passthrough_binary) = passthrough_binary else {
        eprintln!("pilot needs --passthrough-binary <path>");
        std::process::exit(2);
    };

    PilotArgs {
        output_dir,
        passthrough_binary,
        warmup,
        operations,
        monitor_interval: Duration::from_millis(monitor_interval_ms),
    }
}

fn invalid_value(flag: &str) -> ! {
    eprintln!("{flag}'s value must be a non-negative integer");
    std::process::exit(2);
}

/// Runs the feasibility pilot (protocol.md §13) against this binary
/// (instrumented, the one currently running) and a separately built
/// passthrough binary, for [`pilot::PilotWorkload::PointRead`] and
/// [`pilot::PilotWorkload::PointUpdate`] at the small size. Writes the
/// mechanics-only report to `<output-dir>/pilot-report.txt`.
fn run_pilot(args: &[String]) {
    let pilot_args = parse_pilot_args(args);

    if let Err(err) = std::fs::create_dir_all(&pilot_args.output_dir) {
        eprintln!(
            "could not create output directory {}: {err}",
            pilot_args.output_dir.display()
        );
        std::process::exit(1);
    }

    let instrumented_binary_sha256 = preflight::hash_current_binary().unwrap_or_else(|err| {
        eprintln!("could not hash the running binary: {err}");
        std::process::exit(1);
    });
    let passthrough_binary_sha256 = preflight::hash_file(&pilot_args.passthrough_binary)
        .unwrap_or_else(|err| {
            eprintln!(
                "could not hash the passthrough binary at {}: {err}",
                pilot_args.passthrough_binary.display()
            );
            std::process::exit(1);
        });

    let sampler = pilot::MonitorSampler::start(pilot_args.monitor_interval);
    let run_start = std::time::Instant::now();

    let mut cells = Vec::new();
    for workload in [
        pilot::PilotWorkload::PointRead,
        pilot::PilotWorkload::PointUpdate,
    ] {
        let seed = kiln_e4_harness::generators::seed_for(0, cells.len() as u64);

        let instrumented = pilot::measure_cell_in_process(
            workload,
            pilot_args.warmup,
            pilot_args.operations,
            seed,
        )
        .unwrap_or_else(|err| {
            eprintln!("instrumented measurement of {workload:?} failed: {err}");
            std::process::exit(1);
        });

        let passthrough_mean_ns = run_passthrough_measurement(
            &pilot_args.passthrough_binary,
            workload,
            pilot_args.warmup,
            pilot_args.operations,
            seed,
        );

        let overhead = pilot::overhead_fraction(instrumented.mean_wall_ns, passthrough_mean_ns);
        cells.push(PilotCellRow {
            workload_label: workload.label().to_string(),
            operations_measured: instrumented.operations_measured,
            instrumented_mean_wall_ns: instrumented.mean_wall_ns,
            passthrough_mean_wall_ns: passthrough_mean_ns,
            overhead_fraction: overhead,
            passes_bound: pilot::passes_overhead_bound(overhead),
        });
    }

    let run_duration = run_start.elapsed();
    let raw_samples = sampler.stop();
    let actual_samples = raw_samples.iter().filter(|s| s.is_ok()).count();
    let expected_samples = pilot::expected_sample_count(run_duration, pilot_args.monitor_interval);

    let report = PilotReport {
        instrumented_binary_sha256,
        passthrough_binary_sha256,
        cells,
        run_duration,
        monitor: MonitorCoverageRow {
            interval: pilot_args.monitor_interval,
            window: run_duration,
            expected_samples,
            actual_samples,
        },
        rejected_attempts: 0,
        perf: PerfStatus::Skipped {
            reason: "perf access not granted this session (protocol §7/§12)".to_string(),
        },
    };

    let rendered = report.render();
    print!("{rendered}");
    let report_path = pilot_args.output_dir.join("pilot-report.txt");
    if let Err(err) = std::fs::write(&report_path, &rendered) {
        eprintln!(
            "could not write pilot report to {}: {err}",
            report_path.display()
        );
        std::process::exit(1);
    }

    if !report.cells.iter().all(|c| c.passes_bound) {
        eprintln!("the 2% overhead bound (protocol §5) failed for at least one cell");
        std::process::exit(1);
    }
}

/// Spawns the passthrough binary's own `measure-cell` subcommand with the
/// same parameters and reads back its single reported mean — the pilot
/// never links the passthrough build in, so this is the only way to
/// compare the two (protocol §5: "a second, identical build with a
/// passthrough shim").
fn run_passthrough_measurement(
    passthrough_binary: &std::path::Path,
    workload: pilot::PilotWorkload,
    warmup: usize,
    operations: usize,
    seed: u64,
) -> f64 {
    let output = std::process::Command::new(passthrough_binary)
        .arg("measure-cell")
        .arg(workload.label())
        .arg(warmup.to_string())
        .arg(operations.to_string())
        .arg(seed.to_string())
        .output()
        .unwrap_or_else(|err| {
            eprintln!(
                "could not run the passthrough binary at {}: {err}",
                passthrough_binary.display()
            );
            std::process::exit(1);
        });
    if !output.status.success() {
        eprintln!(
            "passthrough measure-cell for {workload:?} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        std::process::exit(1);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("mean_wall_ns="))
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or_else(|| {
            eprintln!("could not parse mean_wall_ns from passthrough output: {stdout:?}");
            std::process::exit(1);
        })
}
