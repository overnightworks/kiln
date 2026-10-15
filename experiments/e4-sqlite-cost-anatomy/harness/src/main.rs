//! The harness binary: `preflight` (issue #19); `measure-cell` and
//! `monitor-sample`, the pilot's/run's per-build, per-sample measurement
//! primitives (issue #28); `pilot` (the feasibility pilot, protocol.md
//! §13, issue #28), wired to the real run driver for its own repeated
//! comparisons; and `run`/`run-repetition` (the result-bearing run path,
//! issue #28), whose verdict computation (protocol §10's thresholds and
//! §11's output tables) is not wired up yet — this writes one cell's raw
//! per-repetition data only.

use std::path::{Path, PathBuf};
use std::time::Duration;

use kiln_e4_harness::comparison;
use kiln_e4_harness::pilot::{self, MonitorCoverageRow, PerfStatus, PilotReport};
use kiln_e4_harness::preflight::{self, BinaryHashCheck, CheckResult};
use kiln_e4_harness::{affinity, driver, machine, monitor, restore, run, vfs, workloads};
use rusqlite::{Connection, OpenFlags};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("preflight") => run_preflight(&args[2..]),
        Some("measure-cell") => run_measure_cell(&args[2..]),
        Some("monitor-sample") => run_monitor_sample(),
        Some("pilot") => run_pilot(&args[2..]),
        Some("run") => run_run(&args[2..]),
        Some("run-repetition") => run_repetition(&args[2..]),
        _ => {
            eprintln!(
                "usage: kiln-e4-harness <preflight|measure-cell|monitor-sample|pilot|run|run-repetition> [args]"
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
/// instrumented one or the passthrough one `pilot`/`run` spawned), then
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
/// independently pinned process, spawned by [`run::sample_interference`]
/// around every gated attempt in `pilot`/`run`.
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
        Ok(sample) => println!("{}", run::render_sample_line(&sample)),
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
        pilot_args.warmup,
        pilot_args.operations,
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

struct RunArgs {
    manifest_path: PathBuf,
    workload: pilot::PilotWorkload,
    size: workloads::Size,
    synchronous: workloads::Synchronous,
    output_dir: PathBuf,
}

fn parse_run_args(args: &[String]) -> RunArgs {
    let mut manifest_path = None;
    let mut workload = None;
    let mut size = None;
    let mut synchronous = None;
    let mut output_dir = None;

    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut next = || {
            iter.next().unwrap_or_else(|| {
                eprintln!("{flag} needs a value");
                std::process::exit(2);
            })
        };
        match flag.as_str() {
            "--manifest" => manifest_path = Some(PathBuf::from(next())),
            "--workload" => workload = Some(next().clone()),
            "--size" => size = Some(next().clone()),
            "--synchronous" => synchronous = Some(next().clone()),
            "--output-dir" => output_dir = Some(PathBuf::from(next())),
            other => {
                eprintln!("unknown run argument {other:?}");
                std::process::exit(2);
            }
        }
    }

    let Some(manifest_path) = manifest_path else {
        eprintln!("run needs --manifest <path>");
        std::process::exit(2);
    };
    let Some(workload) = workload else {
        eprintln!("run needs --workload <point-read|point-update>");
        std::process::exit(2);
    };
    let Some(size) = size else {
        eprintln!("run needs --size <small|medium>");
        std::process::exit(2);
    };
    let Some(synchronous) = synchronous else {
        eprintln!("run needs --synchronous <full|normal>");
        std::process::exit(2);
    };
    let Some(output_dir) = output_dir else {
        eprintln!("run needs --output-dir <dir>");
        std::process::exit(2);
    };

    let Some(workload) = pilot::PilotWorkload::parse(&workload) else {
        eprintln!("unknown workload {workload:?}; expected point-read or point-update");
        std::process::exit(2);
    };
    let Some(size) = workloads::Size::parse(&size) else {
        eprintln!("unknown size {size:?}; expected small or medium");
        std::process::exit(2);
    };
    let Some(synchronous) = workloads::Synchronous::parse(&synchronous) else {
        eprintln!("unknown synchronous {synchronous:?}; expected full or normal");
        std::process::exit(2);
    };

    RunArgs {
        manifest_path,
        workload,
        size,
        synchronous,
        output_dir,
    }
}

/// Builds protocol §2.1's pristine snapshot from scratch: a fresh
/// connection, configured and loaded at `size`'s row count, checkpointed
/// to an empty WAL, then copied into `pristine_dir` — the state every
/// repetition attempt restores from. Runs once per (size, synchronous);
/// `run_run` skips this when `pristine_dir` already holds a snapshot.
fn build_pristine_state(
    setup_dir: &Path,
    pristine_dir: &Path,
    size: workloads::Size,
    synchronous: workloads::Synchronous,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(setup_dir)?;
    let db_path = setup_dir.join("main.db");
    let wal_path = setup_dir.join("main.db-wal");
    let conn = Connection::open(&db_path)?;
    workloads::configure_connection(&conn, synchronous)?;
    workloads::create_schema(&conn)?;
    workloads::initial_load(&conn, size.row_count())?;
    drop(conn);
    restore::checkpoint_truncate(&db_path, &wal_path)?;
    restore::save_pristine(&db_path, &wal_path, pristine_dir)?;
    Ok(())
}

/// Runs the result-bearing run path's mechanics (protocol §8/§9) for one
/// (workload, size, synchronous) cell: [`machine::REPETITIONS_PER_CELL`]
/// repetitions, each an isolated, interference-gated child process
/// (`run-repetition`), via [`run::run_cell`]. Refuses to start without a
/// preflight manifest whose recorded binary hash matches this exact
/// binary. Writes raw per-repetition data to
/// `<output-dir>/raw-repetitions.tsv` and every rejection, immediately,
/// to `<output-dir>/rejected.log`. Does not compute protocol §10's
/// verdict (shares, thresholds) — that is a separate, not-yet-built
/// slice; this writes mechanics only, like the pilot.
fn run_run(args: &[String]) {
    let run_args = parse_run_args(args);

    let manifest_contents =
        std::fs::read_to_string(&run_args.manifest_path).unwrap_or_else(|err| {
            eprintln!(
                "could not read preflight manifest at {}: {err}",
                run_args.manifest_path.display()
            );
            std::process::exit(1);
        });
    let current_hash = preflight::hash_current_binary().unwrap_or_else(|err| {
        eprintln!("could not hash the running binary: {err}");
        std::process::exit(1);
    });
    match preflight::check_binary_hash(&manifest_contents, &current_hash) {
        BinaryHashCheck::Match => {}
        other => {
            eprintln!(
                "preflight manifest check failed ({other:?}); the result-bearing run refuses to \
                 start without a passing preflight manifest whose recorded binary hash matches \
                 this exact binary (protocol §12)"
            );
            std::process::exit(1);
        }
    }

    if let Err(err) = std::fs::create_dir_all(&run_args.output_dir) {
        eprintln!(
            "could not create output directory {}: {err}",
            run_args.output_dir.display()
        );
        std::process::exit(1);
    }

    let pristine_dir = run_args.output_dir.join("pristine");
    let rejected_log_path = run_args.output_dir.join("rejected.log");
    let raw_data_path = run_args.output_dir.join("raw-repetitions.tsv");

    if !pristine_dir.join("main.db").exists()
        && let Err(err) = build_pristine_state(
            &run_args.output_dir.join("setup"),
            &pristine_dir,
            run_args.size,
            run_args.synchronous,
        )
    {
        eprintln!("could not build the pristine snapshot: {err}");
        std::process::exit(1);
    }

    let self_binary = std::env::current_exe().unwrap_or_else(|err| {
        eprintln!("could not resolve the current binary's own path: {err}");
        std::process::exit(1);
    });

    let plan = driver::operation_plan(run_args.workload.as_workload());
    let work_dir = run_args.output_dir.join("work");
    let spec = run::CellSpec {
        workload: run_args.workload.label(),
        size: run_args.size.label(),
        synchronous: run_args.synchronous.label(),
        pristine_dir: &pristine_dir,
        work_dir: &work_dir,
        warmup: plan.warmup,
        operations: plan.timed,
    };

    let result = run::run_cell(&self_binary, &spec, &rejected_log_path).unwrap_or_else(|err| {
        eprintln!("the run failed: {err}");
        std::process::exit(1);
    });

    let mut raw = String::new();
    match &result.cell_outcomes[0] {
        driver::CellOutcome::Completed(successes) => {
            for (repetition_index, measurement) in successes.iter().enumerate() {
                raw.push_str(&format!(
                    "repetition={repetition_index}\t{}\n",
                    measurement.to_line()
                ));
            }
        }
        driver::CellOutcome::InsufficientRepetitions {
            succeeded,
            required,
        } => {
            println!(
                "cell inconclusive — insufficient repetitions ({succeeded}/{required} succeeded); see {}",
                rejected_log_path.display()
            );
        }
    }
    if let Err(err) = std::fs::write(&raw_data_path, &raw) {
        eprintln!(
            "could not write raw repetition data to {}: {err}",
            raw_data_path.display()
        );
        std::process::exit(1);
    }
    println!(
        "wrote {} accepted repetition(s) and {} rejected attempt(s) to {}",
        raw.lines().count(),
        result.rejected_attempts.len(),
        run_args.output_dir.display()
    );
}

/// `run-repetition <workload> <size> <synchronous> <pristine-dir> <work-dir>
/// <warmup> <operations> <seed>`: one repetition attempt's own process
/// (protocol §8.4), pinned to [`machine::PINNED_CPU`], restoring
/// `pristine-dir`'s snapshot into `work-dir` before running `warmup`
/// discarded operations and `operations` timed ones, and printing the
/// timed window's summed [`run::RepetitionMeasurement`] — the one
/// process [`run::run_repetition_child`] spawns and reads back.
fn run_repetition(args: &[String]) {
    let [
        workload,
        size,
        synchronous,
        pristine_dir,
        work_dir,
        warmup,
        operations,
        seed,
    ] = args
    else {
        eprintln!(
            "usage: kiln-e4-harness run-repetition <point-read|point-update> <small|medium> \
             <full|normal> <pristine-dir> <work-dir> <warmup> <operations> <seed>"
        );
        std::process::exit(2);
    };
    let Some(workload) = pilot::PilotWorkload::parse(workload) else {
        eprintln!("unknown workload {workload:?}; expected point-read or point-update");
        std::process::exit(2);
    };
    let Some(size) = workloads::Size::parse(size) else {
        eprintln!("unknown size {size:?}; expected small or medium");
        std::process::exit(2);
    };
    let Some(synchronous) = workloads::Synchronous::parse(synchronous) else {
        eprintln!("unknown synchronous {synchronous:?}; expected full or normal");
        std::process::exit(2);
    };
    let pristine_dir = PathBuf::from(pristine_dir);
    let work_dir = PathBuf::from(work_dir);
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

    if let Err(err) = std::fs::create_dir_all(&work_dir) {
        eprintln!(
            "could not create work directory {}: {err}",
            work_dir.display()
        );
        std::process::exit(1);
    }
    let db_path = work_dir.join("main.db");
    let wal_path = work_dir.join("main.db-wal");
    if let Err(err) = restore::restore_pristine(&pristine_dir, &db_path, &wal_path) {
        eprintln!(
            "could not restore pristine state from {} into {}: {err}",
            pristine_dir.display(),
            work_dir.display()
        );
        std::process::exit(1);
    }

    if let Err(err) = vfs::install() {
        eprintln!("could not install the shim VFS: {err}");
        std::process::exit(1);
    }
    let conn = match Connection::open_with_flags_and_vfs(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
        vfs::VFS_NAME,
    ) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("could not open {}: {err}", db_path.display());
            std::process::exit(1);
        }
    };
    if let Err(err) = workloads::configure_connection(&conn, synchronous) {
        eprintln!("could not configure the restored connection: {err}");
        std::process::exit(1);
    }

    let row_count = size.row_count();
    let total_ops = warmup + operations;
    let mut txn = match workloads::TransactionStatements::prepare(&conn) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("could not prepare BEGIN/COMMIT statements: {err}");
            std::process::exit(1);
        }
    };

    let sums = match workload {
        pilot::PilotWorkload::PointRead => {
            let mut stmts = match workloads::PointReadStatements::prepare(&conn) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!("could not prepare the point-read statement: {err}");
                    std::process::exit(1);
                }
            };
            let trace = kiln_e4_harness::generators::point_read_trace(seed, row_count, total_ops);
            accumulate_timed_operations(total_ops, warmup, |index| {
                let (_, timing) = vfs::time_operation(|| {
                    workloads::execute_point_read(&mut txn, &mut stmts, &trace[index])
                })
                .map_err(timing_error_message)?;
                txn.end_operation().map_err(|err| err.to_string())?;
                Ok(timing)
            })
        }
        pilot::PilotWorkload::PointUpdate => {
            let mut stmts = match workloads::PointUpdateStatements::prepare(&conn) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!("could not prepare the point-update statement: {err}");
                    std::process::exit(1);
                }
            };
            let trace = kiln_e4_harness::generators::point_update_trace(seed, row_count, total_ops);
            accumulate_timed_operations(total_ops, warmup, |index| {
                let (_, timing) = vfs::time_operation(|| {
                    workloads::execute_point_update(&mut txn, &mut stmts, &trace[index])
                })
                .map_err(timing_error_message)?;
                txn.end_operation().map_err(|err| err.to_string())?;
                Ok(timing)
            })
        }
    };

    println!(
        "{}",
        run::RepetitionMeasurement {
            xsync_ns: sums.0,
            xwrite_ns: sums.1,
            remainder_ns: sums.2,
            wall_ns: sums.3,
            cpu_ns: sums.4,
        }
        .to_line()
    );
}

fn timing_error_message<E: std::fmt::Display>(err: vfs::TimingError<E>) -> String {
    match err {
        vfs::TimingError::Operation(inner) => inner.to_string(),
        vfs::TimingError::Measurement(inner) => inner.to_string(),
    }
}

/// Runs `total_ops` operations through `run_one`, summing
/// [`vfs::OperationTiming`]'s five fields over every operation from
/// `warmup` onward (protocol §8.1/§8.2) — shared by every
/// `run-repetition` workload branch so the accumulation itself is
/// written once.
fn accumulate_timed_operations(
    total_ops: usize,
    warmup: usize,
    mut run_one: impl FnMut(usize) -> Result<vfs::OperationTiming, String>,
) -> (u64, u64, u64, u64, u64) {
    let mut sums = (0u64, 0u64, 0u64, 0u64, 0u64);
    for index in 0..total_ops {
        match run_one(index) {
            Ok(timing) => {
                if index >= warmup {
                    sums.0 += timing.xsync.as_nanos() as u64;
                    sums.1 += timing.xwrite.as_nanos() as u64;
                    sums.2 += timing.remainder.as_nanos() as u64;
                    sums.3 += timing.wall.as_nanos() as u64;
                    sums.4 += timing.cpu.as_nanos() as u64;
                }
            }
            Err(message) => {
                eprintln!("run-repetition operation {index} failed: {message}");
                std::process::exit(1);
            }
        }
    }
    sums
}
