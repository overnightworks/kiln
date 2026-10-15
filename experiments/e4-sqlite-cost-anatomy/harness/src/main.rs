//! The harness binary. Only `preflight` is wired up in this slice (issue
//! #19); the pilot and the result-bearing run are issue #13's.

use kiln_e4_harness::preflight::{self, CheckResult};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("preflight") => run_preflight(),
        _ => {
            eprintln!("usage: kiln-e4-harness preflight");
            std::process::exit(2);
        }
    }
}

fn run_preflight() {
    let report = match preflight::run_real() {
        Ok(report) => report,
        Err(err) => {
            eprintln!("preflight could not run: {err}");
            std::process::exit(1);
        }
    };

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
}
