//! The harness binary: `preflight`; `measure-cell`, `monitor-stream` and
//! `build-role`, the pilot's per-half and per-build primitives; and
//! `pilot` (the feasibility pilot, protocol.md §13). Every subcommand is
//! argument handling and process wiring around library code that carries
//! the tested logic.

use std::fmt::Display;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use kiln_e4_harness::builds::BuildReport;
use kiln_e4_harness::cli::{self, PilotArgs};
use kiln_e4_harness::measure_cell::serve_measure_cell;
use kiln_e4_harness::measure_protocol::MeasureCellRequest;
use kiln_e4_harness::preflight::{self, CheckResult};
use kiln_e4_harness::{admission, affinity, interference, machine, manifest, monitor, pilot_run};

const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("preflight") => run_preflight(&args[2..]),
        Some("measure-cell") => run_measure_cell(&args[2..]),
        Some("monitor-stream") => run_monitor_stream(&args[2..]),
        Some("build-role") => {
            println!("{}", BuildReport::of_this_binary().render_line());
            ExitCode::SUCCESS
        }
        Some("pilot") => run_pilot(&args[2..]),
        _ => fail(
            "usage: kiln-e4-harness <preflight|measure-cell|monitor-stream|build-role|pilot> [args]",
            USAGE_ERROR,
        ),
    }
}

/// Prints `message` to stderr and ends with `code`.
fn fail(message: impl Display, code: u8) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(code)
}

fn run_preflight(args: &[String]) -> ExitCode {
    let manifest_target = match cli::parse_preflight_args(args) {
        Ok(target) => target,
        Err(usage) => return fail(usage, USAGE_ERROR),
    };

    let report = preflight::run_real();
    for (name, check) in [
        ("rustc version", &report.rustc_version),
        ("sqlite version", &report.sqlite_version),
        ("build environment", &report.build_env),
        ("CPU siblings", &report.cpu_siblings),
        ("shim VFS", &report.shim_vfs),
        ("source commit", &report.source_commit),
    ] {
        match check {
            CheckResult::Pass => println!("PASS  {name}"),
            CheckResult::Fail(reason) => println!("FAIL  {name}: {reason}"),
        }
    }
    if !report.all_passed() {
        return fail("preflight failed closed; no frozen run may proceed", 1);
    }

    match manifest_target {
        Some((manifest_path, passthrough_binary)) => {
            write_preflight_manifest(&manifest_path, &passthrough_binary)
        }
        None => ExitCode::SUCCESS,
    }
}

fn write_preflight_manifest(manifest_path: &Path, passthrough_binary: &Path) -> ExitCode {
    let built = std::env::current_exe()
        .and_then(|instrumented| preflight::build_manifest(&instrumented, passthrough_binary));
    let manifest = match built {
        Ok(manifest) => manifest,
        Err(err) => return fail(format!("could not build manifest: {err}"), 1),
    };
    if let Err(err) = manifest::write_manifest(manifest_path, &manifest) {
        return fail(
            format!(
                "could not write manifest to {}: {err}",
                manifest_path.display()
            ),
            1,
        );
    }
    println!(
        "manifest written to {} (source commit {}, instrumented sha256 {}, passthrough sha256 {})",
        manifest_path.display(),
        manifest.source_commit,
        manifest.binary_sha256,
        manifest.passthrough_binary_sha256
    );
    ExitCode::SUCCESS
}

/// `measure-cell <see MeasureCellRequest::to_args>`: pins itself to
/// [`machine::PINNED_CPU`] (protocol §9: "equivalent isolated child
/// processes pinned to CPU 2" — true whether this binary is the
/// instrumented one or the passthrough one), then answers the parent's
/// ready/go conversation and reports the one number safe to read back.
fn run_measure_cell(args: &[String]) -> ExitCode {
    let request = match MeasureCellRequest::parse(args) {
        Ok(request) => request,
        Err(message) => return fail(format!("measure-cell: {message}"), USAGE_ERROR),
    };
    if let Err(err) = affinity::pin_current_process(&[machine::PINNED_CPU]) {
        return fail(
            format!("could not pin to CPU {}: {err}", machine::PINNED_CPU),
            1,
        );
    }
    match serve_measure_cell(&request, io::stdin().lock(), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => fail(format!("measurement failed: {err}"), 1),
    }
}

/// `monitor-stream <interval-ms>`: pins itself outside `{PINNED_CPU,
/// SIBLING_CPU}` (protocol §9) and writes one stamped sample line per
/// interval until its reader goes away — a genuinely separate,
/// independently pinned process, spawned around every measured half.
fn run_monitor_stream(args: &[String]) -> ExitCode {
    let [interval] = args else {
        return fail(
            "usage: kiln-e4-harness monitor-stream <interval-ms>",
            USAGE_ERROR,
        );
    };
    let interval = match cli::parse_interval_ms(interval) {
        Ok(interval) => interval,
        Err(message) => return fail(message, USAGE_ERROR),
    };
    if let Err(err) = affinity::pin_outside_measurement_cpus() {
        return fail(format!("could not pin the monitor: {err}"), 1);
    }
    match interference::stream_samples(
        interval,
        monitor::capture,
        interference::monotonic_ns,
        io::stdout().lock(),
    ) {
        Ok(never) => match never {},
        Err(err) => fail(format!("the monitor stopped: {err}"), 1),
    }
}

/// Runs the feasibility pilot (protocol.md §13): the nine counterbalanced
/// instrumented/passthrough pairs protocol §5 requires for each of the
/// twelve gated cells, every half in an isolated, CPU-pinned child under
/// a pinned monitor process (protocol §9), at the protocol's frozen
/// operation counts. It starts only from vetted builds
/// ([`admission::admit_builds`]) and into evidence files it creates
/// exclusively ([`admission::reserve_evidence_files`]), and writes the
/// mechanics-only report to `<output-dir>/pilot-report.txt`, every
/// rejection, immediately, to `<output-dir>/rejected.log` and every
/// monitor sample to `<output-dir>/monitor-samples.log`. Its exit status
/// is the report's.
fn run_pilot(args: &[String]) -> ExitCode {
    let pilot_args = match PilotArgs::parse(args) {
        Ok(pilot_args) => pilot_args,
        Err(message) => return fail(message, USAGE_ERROR),
    };
    // The orchestrating process stays off the measured core and its
    // sibling, like the monitor.
    if let Err(err) = affinity::pin_outside_measurement_cpus() {
        return fail(format!("could not pin the pilot: {err}"), 1);
    }
    let self_binary = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            return fail(
                format!("could not resolve the current binary's own path: {err}"),
                1,
            );
        }
    };
    let admitted = match admission::admit_builds(
        &self_binary,
        &pilot_args.passthrough_binary,
        &pilot_args.manifest,
    ) {
        Ok(admitted) => admitted,
        Err(refusal) => return fail(format!("the pilot refused to start: {refusal}"), 1),
    };
    let evidence = match admission::reserve_evidence_files(&pilot_args.output_dir) {
        Ok(evidence) => evidence,
        Err(err) => return fail(format!("the pilot refused to start: {err}"), 1),
    };

    let report = match pilot_run::run_pilot(
        &self_binary,
        &pilot_args.passthrough_binary,
        admitted,
        &evidence,
        &pilot_args.output_dir,
        pilot_args.monitor_interval,
    ) {
        Ok(report) => report,
        Err(err) => return fail(format!("the pilot's comparisons failed: {err}"), 1),
    };

    let rendered = report.render();
    print!("{rendered}");
    if let Err(err) = std::fs::write(&evidence.report, &rendered) {
        return fail(
            format!(
                "could not write pilot report to {}: {err}",
                evidence.report.display()
            ),
            1,
        );
    }
    ExitCode::from(report.exit_code())
}
