//! The harness binary: `preflight`; `measure-cell` and `monitor-sample`,
//! the pilot's per-build, per-sample measurement primitives; and `pilot`
//! (the feasibility pilot, protocol.md §13), wired to the real run driver
//! for its repeated comparisons.

use std::path::PathBuf;
use std::time::Duration;

use kiln_e4_harness::comparison;
use kiln_e4_harness::pilot::{self, MonitorCoverageRow, PerfStatus, PilotReport};
use kiln_e4_harness::preflight::{self, CheckResult};
use kiln_e4_harness::{affinity, interference, machine, monitor};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("preflight") => run_preflight(&args[2..]),
        Some("measure-cell") => run_measure_cell(&args[2..]),
        Some("monitor-sample") => run_monitor_sample(),
        Some("pilot") => run_pilot(&args[2..]),
        _ => {
            eprintln!(
                "usage: kiln-e4-harness <preflight|measure-cell|monitor-sample|pilot> [args]"
            );
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

/// `measure-cell <workload> <warmup> <operations> <seed>`: pins itself to
/// [`machine::PINNED_CPU`] (protocol §9: "equivalent isolated child
/// processes pinned to CPU 2" — true whether this binary is the
/// instrumented one or the passthrough one `pilot` spawned), then
/// runs one in-process measurement and prints only its mean wall time in
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

    pin_or_exit(&[machine::PINNED_CPU]);

    match pilot::measure_cell_in_process(workload, warmup, operations, seed) {
        Ok(measured) => println!("mean_wall_ns={}", measured.mean_wall_ns),
        Err(err) => {
            eprintln!("measurement failed: {err}");
            std::process::exit(1);
        }
    }
}

/// `monitor-sample`: pins itself to the complement of
/// `{PINNED_CPU, SIBLING_CPU}` (protocol §9: "never merely unpinned")
/// and prints one [`monitor::Sample`] — a genuinely separate,
/// independently pinned process, spawned by [`interference::sample_interference`]
/// around every gated attempt in `pilot`.
fn run_monitor_sample() {
    let total_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(machine::SIBLING_CPU + 1);
    let pinned_cpus =
        affinity::complement(total_cpus, &[machine::PINNED_CPU, machine::SIBLING_CPU]);
    if pinned_cpus.is_empty() {
        eprintln!(
            "no CPUs left to pin the monitor to once {} and {} are excluded (host reports only {total_cpus} logical CPUs)",
            machine::PINNED_CPU,
            machine::SIBLING_CPU
        );
        std::process::exit(1);
    }
    pin_or_exit(&pinned_cpus);

    match monitor::capture() {
        Ok(sample) => println!("{}", interference::render_sample_line(&sample)),
        Err(err) => {
            eprintln!("could not capture an interference sample: {err}");
            std::process::exit(1);
        }
    }
}

fn pin_or_exit(cpus: &[usize]) {
    if let Err(err) = affinity::pin_current_process(cpus) {
        eprintln!("could not pin this process to {cpus:?}: {err}");
        std::process::exit(1);
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
    let mut warmup = machine::WARMUP_OPERATIONS;
    let mut operations = machine::OPERATION_COUNT_AB_D;
    let mut monitor_interval_ms = machine::SAMPLE_INTERVAL.as_millis() as u64;

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

/// Runs the feasibility pilot (protocol.md §13): the nine counterbalanced
/// instrumented/passthrough pairs protocol §5 requires for each gated
/// workload ([`pilot::PilotWorkload::is_gated`]) at the small size,
/// every attempt gated by the real interference monitor and run in
/// isolated, CPU-pinned child processes (protocol §9) via
/// [`comparison::run_pilot_comparisons`]; read-only workloads are
/// reported as not gated. Writes the mechanics-only report to
/// `<output-dir>/pilot-report.txt` and every rejection, immediately, to
/// `<output-dir>/rejected.log`.
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

    let self_binary = std::env::current_exe().unwrap_or_else(|err| {
        eprintln!("could not resolve the current binary's own path: {err}");
        std::process::exit(1);
    });

    let sampler = pilot::MonitorSampler::start(pilot_args.monitor_interval);
    let run_start = std::time::Instant::now();

    let rejected_log_path = pilot_args.output_dir.join("rejected.log");
    let (cells, rejected_attempts) = comparison::run_pilot_comparisons(
        &self_binary,
        &pilot_args.passthrough_binary,
        |_| kiln_e4_harness::driver::OperationPlan {
            warmup: pilot_args.warmup,
            timed: pilot_args.operations,
        },
        &rejected_log_path,
    )
    .unwrap_or_else(|err| {
        eprintln!("the pilot's comparisons failed: {err}");
        std::process::exit(1);
    });

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
        rejected_attempts,
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

    if !report.all_gated_cells_pass() {
        eprintln!(
            "a gated cell did not pass the 2% overhead bound (protocol §5): it failed, \
             was inconclusive, or had insufficient repetitions"
        );
        std::process::exit(1);
    }
}
