//! Preflight (protocol.md §12): non-result-bearing checks that fail
//! closed before any frozen run. Each check is a pure function over
//! already-collected evidence, so a mismatch can be injected and proven
//! rejected without mutating the real process environment or
//! filesystem — [`run_real`] is the thin, untested adapter that collects
//! that evidence for an actual preflight.

use std::ffi::CStr;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use libsqlite3_sys as ffi;
use sha2::{Digest, Sha256};

use crate::machine;

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
}

impl PreflightReport {
    /// Fails closed: every check must pass.
    pub fn all_passed(&self) -> bool {
        [
            &self.rustc_version,
            &self.sqlite_version,
            &self.build_env,
            &self.cpu_siblings,
            &self.shim_vfs,
        ]
        .iter()
        .all(|c| c.is_pass())
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

/// SHA-256 of a file's bytes, as a lowercase hex string (protocol §12:
/// "Preflight hashes the harness binary... into the manifest; the
/// result-bearing run re-hashes and fails closed on a mismatch").
pub fn hash_file(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// SHA-256 of the currently running binary's own bytes.
pub fn hash_current_binary() -> io::Result<String> {
    hash_file(&std::env::current_exe()?)
}

fn pinned_toolchain_channel(repo_root: &Path) -> io::Result<String> {
    let contents = fs::read_to_string(repo_root.join("rust-toolchain.toml"))?;
    contents
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

/// This crate's directory, known at compile time — used only to find the
/// repository root's `rust-toolchain.toml`, never baked into a check's
/// pass/fail logic itself.
fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Runs every check against the real environment, filesystem, and
/// `rustc`/`sqlite3` the process is actually running under. This is the
/// thin adapter: each check it calls is independently unit-tested
/// against injected evidence instead.
pub fn run_real() -> io::Result<PreflightReport> {
    let rustc_output = Command::new("rustc").arg("--version").output()?;
    let rustc_version_output = String::from_utf8_lossy(&rustc_output.stdout).into_owned();
    // harness/ -> e4-sqlite-cost-anatomy/ -> experiments/ -> repo root.
    let repo_root = crate_dir()
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "repo root not found above crate")
        })?;
    let pinned_channel = pinned_toolchain_channel(repo_root)?;
    let rustc_version = check_rustc_version(&rustc_version_output, &pinned_channel);

    let sqlite_version_actual = unsafe {
        CStr::from_ptr(ffi::sqlite3_libversion())
            .to_str()
            .unwrap_or_default()
            .to_string()
    };
    let sqlite_version = check_sqlite_version(&sqlite_version_actual);

    let offenders = build_env_offenders(|name| std::env::var_os(name));
    let build_env = check_build_env(&offenders);

    let siblings_path = format!(
        "/sys/devices/system/cpu/cpu{}/topology/thread_siblings_list",
        machine::PINNED_CPU
    );
    let siblings_list = fs::read_to_string(siblings_path)?;
    let cpu_siblings = check_cpu_siblings(&siblings_list);

    let install_result = crate::vfs::install();
    let shim_vfs = check_shim_vfs(install_result, crate::vfs::is_registered());

    Ok(PreflightReport {
        rustc_version,
        sqlite_version,
        build_env,
        cpu_siblings,
        shim_vfs,
    })
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
    fn hash_file_is_deterministic_and_sensitive_to_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        fs::write(&a, b"same bytes").unwrap();
        fs::write(&b, b"same bytes").unwrap();
        assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());

        fs::write(&b, b"different bytes").unwrap();
        assert_ne!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[test]
    fn report_fails_closed_if_any_check_fails() {
        let report = PreflightReport {
            rustc_version: CheckResult::Pass,
            sqlite_version: CheckResult::Pass,
            build_env: CheckResult::Pass,
            cpu_siblings: CheckResult::Pass,
            shim_vfs: CheckResult::Fail("not registered".into()),
        };
        assert!(!report.all_passed());
    }

    #[test]
    fn report_passes_only_when_every_check_passes() {
        let report = PreflightReport {
            rustc_version: CheckResult::Pass,
            sqlite_version: CheckResult::Pass,
            build_env: CheckResult::Pass,
            cpu_siblings: CheckResult::Pass,
            shim_vfs: CheckResult::Pass,
        };
        assert!(report.all_passed());
    }
}
