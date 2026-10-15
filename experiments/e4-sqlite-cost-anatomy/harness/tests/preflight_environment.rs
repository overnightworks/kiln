//! Drives the real `preflight` entry point to exercise
//! `preflight::run_real`'s actual environment-reading path
//! (`std::env::var_os`, protocol.md §12) — not a reimplementation of it
//! via an injected closure. The offending variable is set only on the
//! spawned child process (`Command::env`), never on this test process
//! itself, so other tests running in parallel in this same process never
//! observe it.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

/// A non-UTF-8 byte sequence, valid as a Unix environment variable value
/// (anything but `NUL`) but not valid UTF-8 — `std::env::var` would see
/// this as unset (`Err(NotUnicode)` collapses to `None`), while
/// `std::env::var_os`, the reader `run_real` actually uses, still
/// reports it set.
fn non_utf8_value() -> &'static OsStr {
    OsStr::from_bytes(&[0xFF, 0xFE])
}

#[test]
fn preflight_reports_a_present_non_utf8_libsqlite3_flags_as_an_offending_build_env_var() {
    let output = Command::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
        .arg("preflight")
        .env("LIBSQLITE3_FLAGS", non_utf8_value())
        .output()
        .expect("spawning the harness binary must succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("FAIL  build environment")
                && line.contains("LIBSQLITE3_FLAGS")),
        "expected a FAIL build environment line naming LIBSQLITE3_FLAGS, got:\n{stdout}"
    );
}

#[test]
fn preflight_passes_build_env_when_none_of_the_watched_variables_are_set() {
    let output = Command::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
        .arg("preflight")
        .env_remove("CC")
        .env_remove("CFLAGS")
        .env_remove("LIBSQLITE3_FLAGS")
        .env_remove("LIBSQLITE3_SYS_USE_PKG_CONFIG")
        .output()
        .expect("spawning the harness binary must succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line == "PASS  build environment"),
        "expected a PASS build environment line, got:\n{stdout}"
    );
}
