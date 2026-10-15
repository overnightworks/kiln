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

/// The preflight manifest (protocol §12): "Preflight hashes the harness
/// binary (SHA-256) into the manifest; the result-bearing run re-hashes
/// and fails closed on a mismatch — preflight's build must be the exact
/// bytes producing the result, not a rebuild in between."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub binary_sha256: String,
    pub rustc_version_output: String,
    pub sqlite_version: String,
}

impl Manifest {
    /// Renders the manifest as simple `key = value` lines — the same
    /// hand-rolled format `rust-toolchain.toml` already uses here (see
    /// [`pinned_toolchain_channel`]), so one small, stable record pulls in
    /// no serialization dependency.
    pub fn render(&self) -> String {
        format!(
            "binary_sha256 = \"{}\"\nrustc_version = \"{}\"\nsqlite_version = \"{}\"\n",
            self.binary_sha256,
            self.rustc_version_output.trim(),
            self.sqlite_version,
        )
    }
}

/// Reads one quoted `key = "value"` line's value out of a rendered
/// manifest. Shared by every manifest field reader so each one stays a
/// one-line call instead of repeating the same quote-stripping.
fn read_quoted_field(manifest_contents: &str, key: &str) -> Option<String> {
    manifest_contents.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(key)?;
        let rest = rest.trim_start().strip_prefix('=')?.trim();
        let value = rest.strip_prefix('"')?.strip_suffix('"')?;
        Some(value.to_string())
    })
}

/// Reads a rendered manifest's `binary_sha256` field back out.
pub fn read_binary_sha256(manifest_contents: &str) -> Option<String> {
    read_quoted_field(manifest_contents, "binary_sha256")
}

/// Whether the manifest's recorded binary hash still matches the hash of
/// the binary running right now (protocol §12's re-hash-and-fail-closed
/// rule, also reused by §13's amendment delta check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryHashCheck {
    Match,
    Mismatch { manifest: String, current: String },
    ManifestUnreadable,
}

/// Checks a freshly computed binary hash against a manifest's recorded
/// one. Pure: both hashes are already-collected evidence, so the decision
/// is tested without hashing a real file.
pub fn check_binary_hash(manifest_contents: &str, current_hash: &str) -> BinaryHashCheck {
    match read_binary_sha256(manifest_contents) {
        Some(manifest_hash) if manifest_hash == current_hash => BinaryHashCheck::Match,
        Some(manifest_hash) => BinaryHashCheck::Mismatch {
            manifest: manifest_hash,
            current: current_hash.to_string(),
        },
        None => BinaryHashCheck::ManifestUnreadable,
    }
}

/// Writes the manifest to `path`. The thin, untested adapter — exactly
/// like [`run_real`] is for the checks themselves: [`Manifest::render`],
/// [`read_binary_sha256`], and [`check_binary_hash`] carry the logic.
pub fn write_manifest(path: &Path, manifest: &Manifest) -> io::Result<()> {
    fs::write(path, manifest.render())
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
        || {
            // harness/ -> e4-sqlite-cost-anatomy/ -> experiments/ -> repo root.
            let repo_root = crate_dir()
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "repo root not found above crate")
                })?;
            pinned_toolchain_channel(repo_root)
        },
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
    }
}

/// Builds the real manifest (protocol §12): the currently running
/// binary's own SHA-256, `rustc --version`'s raw output, and
/// `sqlite3_libversion()`. Called only once every preflight check has
/// passed — a manifest recording a binary preflight has already rejected
/// is not the record §12 means.
pub fn build_manifest() -> io::Result<Manifest> {
    let binary_sha256 = hash_current_binary()?;
    let rustc_version_output =
        String::from_utf8_lossy(&Command::new("rustc").arg("--version").output()?.stdout)
            .into_owned();
    let sqlite_version = unsafe {
        CStr::from_ptr(ffi::sqlite3_libversion())
            .to_str()
            .unwrap_or_default()
            .to_string()
    };
    Ok(Manifest {
        binary_sha256,
        rustc_version_output,
        sqlite_version,
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
        };
        assert!(report.all_passed());
    }

    mod kiln_e4_09 {
        use super::*;

        #[test]
        fn manifest_render_round_trips_through_read_binary_sha256() {
            let manifest = Manifest {
                binary_sha256: "deadbeef".repeat(8),
                rustc_version_output: "rustc 1.99.1 (58cf8f9c3 2026-10-13)\n".to_string(),
                sqlite_version: "3.53.2".to_string(),
            };
            let rendered = manifest.render();
            assert_eq!(read_binary_sha256(&rendered), Some(manifest.binary_sha256));
        }

        #[test]
        fn read_binary_sha256_is_none_for_a_manifest_missing_the_field() {
            assert_eq!(read_binary_sha256("rustc_version = \"1.99.1\"\n"), None);
        }

        #[test]
        fn check_binary_hash_matches_when_the_rendered_manifest_agrees() {
            let manifest = Manifest {
                binary_sha256: "abc123".to_string(),
                rustc_version_output: "rustc 1.99.1".to_string(),
                sqlite_version: "3.53.2".to_string(),
            };
            let rendered = manifest.render();
            assert_eq!(
                check_binary_hash(&rendered, "abc123"),
                BinaryHashCheck::Match
            );
        }

        #[test]
        fn check_binary_hash_reports_a_mismatch_against_a_rebuilt_binary() {
            let manifest = Manifest {
                binary_sha256: "original-hash".to_string(),
                rustc_version_output: "rustc 1.99.1".to_string(),
                sqlite_version: "3.53.2".to_string(),
            };
            let rendered = manifest.render();
            assert_eq!(
                check_binary_hash(&rendered, "rebuilt-hash"),
                BinaryHashCheck::Mismatch {
                    manifest: "original-hash".to_string(),
                    current: "rebuilt-hash".to_string(),
                }
            );
        }

        #[test]
        fn check_binary_hash_fails_closed_on_an_unreadable_manifest() {
            assert_eq!(
                check_binary_hash("not a manifest at all", "any-hash"),
                BinaryHashCheck::ManifestUnreadable
            );
        }

        #[test]
        fn write_manifest_writes_a_file_read_binary_sha256_can_read_back() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("manifest.txt");
            let manifest = Manifest {
                binary_sha256: "0123456789abcdef".to_string(),
                rustc_version_output: "rustc 1.99.1".to_string(),
                sqlite_version: "3.53.2".to_string(),
            };
            write_manifest(&path, &manifest).unwrap();
            let contents = fs::read_to_string(&path).unwrap();
            assert_eq!(read_binary_sha256(&contents), Some(manifest.binary_sha256));
        }

        #[test]
        fn build_manifest_reports_the_current_binarys_own_hash() {
            let manifest = build_manifest().expect("build_manifest against the real environment");
            let expected = hash_current_binary().expect("hash the current binary directly");
            assert_eq!(manifest.binary_sha256, expected);
            assert_eq!(manifest.sqlite_version, machine::PINNED_SQLITE_VERSION);
        }
    }
}
