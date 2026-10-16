//! Preflight (protocol.md §12): non-result-bearing checks that fail
//! closed before any frozen run. Each check is a pure function over
//! already-collected evidence, so a mismatch can be injected and proven
//! rejected without mutating the real process environment or
//! filesystem — [`run_real`] is the thin, untested adapter that collects
//! that evidence for an actual preflight.

use std::ffi::CStr;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use libsqlite3_sys as ffi;

use crate::build_record::BuildRecord;
use crate::builds::BuildEvidence;
use crate::cli::ManifestTarget;
use crate::machine;
use crate::manifest::{self, Manifest};

/// One check's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    Pass,
    Fail(String),
}

impl CheckResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, CheckResult::Pass)
    }
}

/// Every preflight check's outcome (protocol §12's list, minus the
/// `perf`-gated symbolization check — see §7, out of this slice).
#[derive(Debug, Clone)]
pub struct PreflightReport {
    pub rustc_version: CheckResult,
    pub sqlite_version: CheckResult,
    pub build_env: CheckResult,
    pub cpu_siblings: CheckResult,
    pub shim_vfs: CheckResult,
    pub source_commit: CheckResult,
}

impl PreflightReport {
    fn named_checks(&self) -> [(&'static str, &CheckResult); 6] {
        [
            ("rustc version", &self.rustc_version),
            ("sqlite version", &self.sqlite_version),
            ("build environment", &self.build_env),
            ("CPU siblings", &self.cpu_siblings),
            ("shim VFS", &self.shim_vfs),
            ("source commit", &self.source_commit),
        ]
    }

    /// Fails closed: every check must pass.
    pub fn all_passed(&self) -> bool {
        self.named_checks().iter().all(|(_, check)| check.is_pass())
    }
}

/// `rustc --version`'s second word (its semver) must match the pinned
/// channel (protocol §12).
pub fn check_rustc_version(rustc_version_output: &str, pinned_channel: &str) -> CheckResult {
    match rustc_version_output.split_whitespace().nth(1) {
        Some(actual) if actual == pinned_channel => CheckResult::Pass,
        Some(actual) => CheckResult::Fail(format!(
            "rustc reports {actual}, pinned toolchain is {pinned_channel}"
        )),
        None => CheckResult::Fail(format!(
            "could not parse a version from rustc --version output {rustc_version_output:?}"
        )),
    }
}

/// `sqlite3_libversion()` must return the pinned vendored version
/// (protocol §4/§12).
pub fn check_sqlite_version(actual: &str) -> CheckResult {
    if actual == machine::PINNED_SQLITE_VERSION {
        CheckResult::Pass
    } else {
        CheckResult::Fail(format!(
            "sqlite3_libversion() reports {actual}, pinned build is {}",
            machine::PINNED_SQLITE_VERSION
        ))
    }
}

/// None of [`machine::UNSET_BUILD_ENV_VARS`] may be set — any of them
/// can silently change what the bundled SQLite build measures (protocol
/// §12). Takes the already-collected list of offending names so the
/// check itself never touches real process environment.
pub fn check_build_env(offending_vars: &[String]) -> CheckResult {
    if offending_vars.is_empty() {
        CheckResult::Pass
    } else {
        CheckResult::Fail(format!(
            "must be unset but set: {}",
            offending_vars.join(", ")
        ))
    }
}

/// Collects which of [`machine::UNSET_BUILD_ENV_VARS`] are set, via an
/// injectable reader so a test never touches real process environment.
/// Reads as `OsString` (never `String`): a present value that is not
/// valid UTF-8 is still set and still able to change the bundled SQLite
/// build, so it must still be caught (see [`run_real`], which reads via
/// `std::env::var_os`).
pub fn build_env_offenders(read_var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Vec<String> {
    machine::UNSET_BUILD_ENV_VARS
        .iter()
        .filter(|name| read_var(name).is_some())
        .map(|s| s.to_string())
        .collect()
}

/// `thread_siblings_list` for [`machine::PINNED_CPU`] must contain both
/// the pinned CPU and its sibling (protocol §9/§12).
pub fn check_cpu_siblings(thread_siblings_list: &str) -> CheckResult {
    let ids: std::collections::HashSet<usize> = thread_siblings_list
        .trim()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    if ids.contains(&machine::PINNED_CPU) && ids.contains(&machine::SIBLING_CPU) {
        CheckResult::Pass
    } else {
        CheckResult::Fail(format!(
            "thread_siblings_list {thread_siblings_list:?} does not list both CPU {} and CPU {}",
            machine::PINNED_CPU,
            machine::SIBLING_CPU
        ))
    }
}

/// Reads the CPU topology fact via an injectable reader and checks it, so a
/// host that cannot produce the fact (missing `/sys` path, smaller runner)
/// yields a `Fail` instead of aborting every other check (protocol §12 fails
/// closed per check, not per preflight run).
pub fn cpu_siblings_check(read_siblings_list: impl FnOnce() -> io::Result<String>) -> CheckResult {
    match read_siblings_list() {
        Ok(list) => check_cpu_siblings(&list),
        Err(err) => CheckResult::Fail(format!("could not read CPU topology: {err}")),
    }
}

/// Reads `rustc --version` and the pinned toolchain channel via injectable
/// readers and checks them, so either read failing yields a `Fail` instead
/// of aborting every other check.
pub fn rustc_version_check(
    read_rustc_version_output: impl FnOnce() -> io::Result<String>,
    read_pinned_channel: impl FnOnce() -> io::Result<String>,
) -> CheckResult {
    let rustc_version_output = match read_rustc_version_output() {
        Ok(output) => output,
        Err(err) => return CheckResult::Fail(format!("could not run rustc --version: {err}")),
    };
    let pinned_channel = match read_pinned_channel() {
        Ok(channel) => channel,
        Err(err) => {
            return CheckResult::Fail(format!("could not read pinned toolchain channel: {err}"));
        }
    };
    check_rustc_version(&rustc_version_output, &pinned_channel)
}

/// The shim VFS must register and `sqlite3_vfs_find` must see it under
/// its name (protocol §12).
pub fn check_shim_vfs(install_result: Result<(), String>, is_registered: bool) -> CheckResult {
    match install_result {
        Err(reason) => CheckResult::Fail(format!("shim VFS failed to register: {reason}")),
        Ok(()) if !is_registered => {
            CheckResult::Fail("sqlite3_vfs_find could not find the registered shim".to_string())
        }
        Ok(()) => CheckResult::Pass,
    }
}

/// The binary must come from a known, committed state of the source: a
/// result needs a commit it can be reproduced from (protocol §12).
pub fn check_source_commit(commit: &str) -> CheckResult {
    if commit == "unknown" {
        CheckResult::Fail("this binary was built outside a git checkout".to_string())
    } else if commit.ends_with("-dirty") {
        CheckResult::Fail(format!(
            "this binary was built from uncommitted changes on top of {commit}"
        ))
    } else {
        CheckResult::Pass
    }
}

/// The repository's `rust-toolchain.toml` as of the commit this binary was
/// compiled from, embedded so the pin is the reviewed commit's even when
/// the build ran in a checkout that no longer exists.
const PINNED_TOOLCHAIN_FILE: &str = include_str!("../../../../rust-toolchain.toml");

fn pinned_toolchain_channel(toolchain_file: &str) -> io::Result<String> {
    toolchain_file
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("channel")?;
            let rest = rest.trim_start().strip_prefix('=')?.trim();
            let value = rest.strip_prefix('"')?.strip_suffix('"')?;
            Some(value.to_string())
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "no channel line in rust-toolchain.toml",
            )
        })
}

/// Runs every check against the real environment, filesystem, and
/// `rustc`/`sqlite3` the process is actually running under. This is the
/// thin adapter: each check it calls is independently unit-tested against
/// injected evidence instead. Every check runs and reports regardless of
/// whether another check's evidence could be collected (protocol §12: each
/// check fails closed on its own, never by aborting the whole report).
pub fn run_real() -> PreflightReport {
    let rustc_version = rustc_version_check(
        || {
            let output = Command::new("rustc").arg("--version").output()?;
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        },
        || pinned_toolchain_channel(PINNED_TOOLCHAIN_FILE),
    );

    let sqlite_version_actual = unsafe {
        CStr::from_ptr(ffi::sqlite3_libversion())
            .to_str()
            .unwrap_or_default()
            .to_string()
    };
    let sqlite_version = check_sqlite_version(&sqlite_version_actual);

    let offenders = build_env_offenders(|name| std::env::var_os(name));
    let build_env = check_build_env(&offenders);

    let cpu_siblings = cpu_siblings_check(|| {
        let siblings_path = format!(
            "/sys/devices/system/cpu/cpu{}/topology/thread_siblings_list",
            machine::PINNED_CPU
        );
        fs::read_to_string(siblings_path)
    });

    let install_result = crate::vfs::install();
    let shim_vfs = check_shim_vfs(install_result, crate::vfs::is_registered());

    PreflightReport {
        rustc_version,
        sqlite_version,
        build_env,
        cpu_siblings,
        shim_vfs,
        source_commit: check_source_commit(crate::builds::SOURCE_COMMIT),
    }
}

/// Prints each check's outcome to `out`; when every check passed and
/// there is a `target`, records the manifest there, for the binary at
/// `instrumented_binary` and the target's passthrough binary, both held
/// against the target's build record. `Err` is the message of a preflight
/// that failed closed.
pub fn conclude(
    report: &PreflightReport,
    target: Option<&ManifestTarget>,
    instrumented_binary: &Path,
    mut out: impl Write,
) -> Result<(), String> {
    for (name, check) in report.named_checks() {
        let line = match check {
            CheckResult::Pass => format!("PASS  {name}"),
            CheckResult::Fail(reason) => format!("FAIL  {name}: {reason}"),
        };
        writeln!(out, "{line}").map_err(|err| format!("could not report {name}: {err}"))?;
    }
    if !report.all_passed() {
        return Err("preflight failed closed; no frozen run may proceed".to_string());
    }
    let Some(target) = target else {
        return Ok(());
    };
    let record = BuildRecord::read_file(&target.build_record)
        .map_err(|err| format!("could not read the build record: {err}"))?;
    let manifest = build_manifest(instrumented_binary, &target.passthrough_binary, &record)
        .map_err(|err| format!("could not build manifest: {err}"))?;
    manifest::write_manifest(&target.manifest_path, &manifest).map_err(|err| {
        format!(
            "could not write manifest to {}: {err}",
            target.manifest_path.display()
        )
    })?;
    writeln!(
        out,
        "manifest written to {} (source commit {}, instrumented sha256 {}, passthrough sha256 {})",
        target.manifest_path.display(),
        manifest.source_commit,
        manifest.binary_sha256,
        manifest.passthrough_binary_sha256
    )
    .map_err(|err| format!("could not report the manifest: {err}"))
}

/// What the process exits with when the preflight concluded as `conclusion`
/// says: success only for a preflight that passed and recorded what was
/// asked of it.
pub fn exit_status(conclusion: &Result<(), String>) -> u8 {
    match conclusion {
        Ok(()) => 0,
        Err(_) => FAILED_CLOSED_EXIT_STATUS,
    }
}

const FAILED_CLOSED_EXIT_STATUS: u8 = 1;

/// Builds the real manifest (protocol §12): the source commit and the
/// SHA-256 of the instrumented binary and of the passthrough binary it
/// will be compared with, `rustc --version`'s raw output, and
/// `sqlite3_libversion()`. Called only once every preflight check has
/// passed — a manifest recording a binary preflight has already rejected
/// is not the record §12 means. Refuses binaries that are not the two
/// builds `build_record` made from the reviewed commit.
pub fn build_manifest(
    instrumented_binary: &Path,
    passthrough_binary: &Path,
    build_record: &BuildRecord,
) -> io::Result<Manifest> {
    let instrumented = BuildEvidence::collect(instrumented_binary)?;
    let passthrough = BuildEvidence::collect(passthrough_binary)?;
    build_record
        .check_builds(&instrumented, &passthrough)
        .map_err(io::Error::other)?;
    let rustc_version_output =
        String::from_utf8_lossy(&Command::new("rustc").arg("--version").output()?.stdout)
            .into_owned();
    let sqlite_version = unsafe {
        CStr::from_ptr(ffi::sqlite3_libversion())
            .to_str()
            .unwrap_or_default()
            .to_string()
    };
    Manifest::record(
        &instrumented,
        &passthrough,
        rustc_version_output,
        sqlite_version,
    )
    .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustc_version_passes_when_it_matches_the_pin() {
        assert_eq!(
            check_rustc_version("rustc 1.99.1 (58cf8f9c3 2026-10-13)", "1.99.1"),
            CheckResult::Pass
        );
    }

    #[test]
    fn rustc_version_rejects_a_mismatch() {
        let result = check_rustc_version("rustc 1.98.0 (deadbeef 2026-01-01)", "1.99.1");
        assert!(!result.is_pass());
    }

    #[test]
    fn rustc_version_rejects_unparsable_output() {
        let result = check_rustc_version("not a version string", "1.99.1");
        assert!(!result.is_pass());
    }

    #[test]
    fn sqlite_version_passes_on_the_pinned_build() {
        assert_eq!(
            check_sqlite_version(machine::PINNED_SQLITE_VERSION),
            CheckResult::Pass
        );
    }

    #[test]
    fn sqlite_version_rejects_a_mismatch() {
        assert!(!check_sqlite_version("3.0.0").is_pass());
    }

    #[test]
    fn build_env_passes_when_nothing_is_set() {
        let offenders = build_env_offenders(|_| None);
        assert_eq!(check_build_env(&offenders), CheckResult::Pass);
    }

    #[test]
    fn build_env_rejects_any_offending_variable_set() {
        let offenders = build_env_offenders(|name| {
            if name == "CFLAGS" {
                Some(std::ffi::OsString::from("-O0"))
            } else {
                None
            }
        });
        let result = check_build_env(&offenders);
        assert!(!result.is_pass());
        assert!(matches!(result, CheckResult::Fail(msg) if msg.contains("CFLAGS")));
    }

    // A present-but-not-valid-UTF-8 value's detection is pinned against
    // the real `std::env::var_os`-based reader `run_real` actually uses,
    // not an injected closure — see
    // `tests/preflight_environment.rs`'s
    // `preflight_reports_a_present_non_utf8_libsqlite3_flags_as_an_offending_build_env_var`,
    // which drives the real `preflight` binary with the variable set
    // only on that child process.

    #[test]
    fn cpu_siblings_passes_for_the_pinned_pair_in_either_order() {
        assert_eq!(check_cpu_siblings("2,8"), CheckResult::Pass);
        assert_eq!(check_cpu_siblings("8,2"), CheckResult::Pass);
    }

    #[test]
    fn cpu_siblings_rejects_a_different_pair() {
        assert!(!check_cpu_siblings("2,9").is_pass());
    }

    #[test]
    fn shim_vfs_rejects_an_install_failure() {
        let result = check_shim_vfs(Err("sqlite3_vfs_register failed with code 1".into()), false);
        assert!(!result.is_pass());
    }

    #[test]
    fn shim_vfs_rejects_a_successful_install_sqlite_still_cannot_find() {
        let result = check_shim_vfs(Ok(()), false);
        assert!(!result.is_pass());
    }

    #[test]
    fn shim_vfs_passes_once_installed_and_findable() {
        assert_eq!(check_shim_vfs(Ok(()), true), CheckResult::Pass);
    }

    #[test]
    fn the_pinned_toolchain_channel_is_read_from_rust_toolchain_toml() {
        let file = "[toolchain]\nchannel = \"1.99.1\"\ncomponents = [\"rustfmt\"]\n";

        assert_eq!(pinned_toolchain_channel(file).unwrap(), "1.99.1");
    }

    #[test]
    fn a_toolchain_file_without_a_channel_line_is_an_error() {
        assert!(pinned_toolchain_channel("[toolchain]\n").is_err());
        assert!(pinned_toolchain_channel("").is_err());
    }

    #[test]
    fn the_embedded_toolchain_pin_names_a_channel() {
        assert!(pinned_toolchain_channel(PINNED_TOOLCHAIN_FILE).is_ok());
    }

    #[test]
    fn report_fails_closed_if_any_check_fails() {
        let report = PreflightReport {
            rustc_version: CheckResult::Pass,
            sqlite_version: CheckResult::Pass,
            build_env: CheckResult::Pass,
            cpu_siblings: CheckResult::Pass,
            shim_vfs: CheckResult::Fail("not registered".into()),
            source_commit: CheckResult::Pass,
        };
        assert!(!report.all_passed());
    }

    #[test]
    fn rustc_version_check_fails_closed_when_rustc_cannot_be_run() {
        let result = rustc_version_check(
            || Err(io::Error::new(io::ErrorKind::NotFound, "rustc not found")),
            || Ok("1.99.1".to_string()),
        );
        assert!(!result.is_pass());
    }

    #[test]
    fn rustc_version_check_fails_closed_when_the_pinned_channel_cannot_be_read() {
        let result = rustc_version_check(
            || Ok("rustc 1.99.1 (58cf8f9c3 2026-10-13)".to_string()),
            || {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "rust-toolchain.toml missing",
                ))
            },
        );
        assert!(!result.is_pass());
    }

    #[test]
    fn cpu_siblings_check_fails_closed_when_the_topology_fact_is_unreadable() {
        // A host without the pinned CPU's /sys topology path (a smaller CI
        // runner, for instance) must still yield a FAIL line for this one
        // check — not abort collecting the rest of the report (this is the
        // GitHub Actions preflight regression: a bare `?` inside `run_real`
        // on this read used to skip the whole report).
        let result =
            cpu_siblings_check(|| Err(io::Error::new(io::ErrorKind::NotFound, "no such path")));
        assert!(!result.is_pass());
    }

    #[test]
    fn an_unreadable_host_fact_does_not_suppress_the_other_checks() {
        let report = PreflightReport {
            rustc_version: rustc_version_check(
                || Err(io::Error::new(io::ErrorKind::NotFound, "rustc not found")),
                || Ok("1.99.1".to_string()),
            ),
            sqlite_version: check_sqlite_version(machine::PINNED_SQLITE_VERSION),
            build_env: check_build_env(&[]),
            cpu_siblings: cpu_siblings_check(|| {
                Err(io::Error::new(io::ErrorKind::NotFound, "no such path"))
            }),
            shim_vfs: check_shim_vfs(Ok(()), true),
            source_commit: CheckResult::Pass,
        };
        assert!(!report.rustc_version.is_pass());
        assert!(!report.cpu_siblings.is_pass());
        assert!(report.sqlite_version.is_pass());
        assert!(report.build_env.is_pass());
        assert!(report.shim_vfs.is_pass());
    }

    #[test]
    fn report_passes_only_when_every_check_passes() {
        let report = PreflightReport {
            rustc_version: CheckResult::Pass,
            sqlite_version: CheckResult::Pass,
            build_env: CheckResult::Pass,
            cpu_siblings: CheckResult::Pass,
            shim_vfs: CheckResult::Pass,
            source_commit: CheckResult::Pass,
        };
        assert!(report.all_passed());
    }

    mod kiln_e4_09 {
        use super::*;

        #[test]
        fn source_commit_passes_only_for_a_committed_known_state() {
            assert_eq!(check_source_commit(&"a".repeat(40)), CheckResult::Pass);
            for refused in ["unknown", &format!("{}-dirty", "a".repeat(40))] {
                assert!(!check_source_commit(refused).is_pass(), "{refused}");
            }
        }
    }
}
