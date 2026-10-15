//! Drives the pilot's admission checks (`admission::admit_builds`, the
//! `build-role` subcommand and the `pilot` entry point's refusals) against
//! the real compiled harness binary. The pilot never gets as far as
//! measuring in these tests: every case ends in an admission decision or a
//! refusal.

// The compiled harness plays the instrumented build here; a passthrough
// compile of it would be the other role.
#![cfg(not(feature = "passthrough_shim"))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use kiln_e4_harness::admission::{AdmittedBuilds, Refusal, admit_builds, query_build_role};
use kiln_e4_harness::comparison::Build;
use kiln_e4_harness::preflight::{self, BinaryHashCheck, Manifest};

fn harness_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
}

/// A stand-in for the passthrough release build, which this test crate
/// cannot compile: it answers only the `build-role` question, the one
/// admission asks.
fn stand_in_passthrough_binary() -> &'static Path {
    static BINARY: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &BINARY
        .get_or_init(|| {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("passthrough");
            fs::write(&path, "#!/bin/sh\necho build=passthrough\n").expect("write stand-in");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
            (dir, path)
        })
        .1
}

fn write_manifest_recording(directory: &Path, binary_sha256: &str) -> PathBuf {
    let path = directory.join("manifest.txt");
    let manifest = Manifest {
        binary_sha256: binary_sha256.to_string(),
        rustc_version_output: "rustc 1.99.1".to_string(),
        sqlite_version: "3.53.2".to_string(),
    };
    preflight::write_manifest(&path, &manifest).expect("write manifest");
    path
}

fn harness_hash() -> String {
    preflight::hash_file(harness_binary()).expect("hash the harness binary")
}

fn run_pilot(arguments: &[&Path]) -> Output {
    Command::new(harness_binary())
        .arg("pilot")
        .args(arguments)
        .output()
        .expect("spawning the harness binary must succeed")
}

mod kiln_e4_09 {
    use super::*;

    #[test]
    fn the_pilot_refuses_a_manifest_recorded_for_a_different_binary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_manifest_recording(dir.path(), &"0".repeat(64));

        let refusal = admit_builds(harness_binary(), stand_in_passthrough_binary(), &manifest)
            .expect_err("a manifest for other bytes must be refused");

        assert!(matches!(
            refusal,
            Refusal::ManifestLink(BinaryHashCheck::Mismatch { .. })
        ));
    }
}

mod kiln_e4_20 {
    use super::*;

    #[test]
    fn a_harness_binary_reports_the_build_it_was_compiled_as() {
        assert_eq!(
            query_build_role(harness_binary()).expect("build-role"),
            Build::of_this_binary()
        );
    }

    #[test]
    fn the_pilot_admits_the_manifest_binary_with_a_passthrough_build() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_manifest_recording(dir.path(), &harness_hash());

        let admitted = admit_builds(harness_binary(), stand_in_passthrough_binary(), &manifest)
            .expect("the vetted builds must be admitted");

        assert_eq!(
            admitted,
            AdmittedBuilds {
                instrumented_sha256: harness_hash(),
                passthrough_sha256: preflight::hash_file(stand_in_passthrough_binary()).unwrap(),
            }
        );
    }

    #[test]
    fn the_pilot_refuses_the_same_build_given_as_both_roles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_manifest_recording(dir.path(), &harness_hash());

        let refusal = admit_builds(harness_binary(), harness_binary(), &manifest)
            .expect_err("one build cannot be both roles");

        assert!(matches!(refusal, Refusal::BuildRole { .. }), "{refusal}");
    }

    #[test]
    fn the_pilot_refuses_a_passthrough_binary_that_cannot_report_its_role() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_manifest_recording(dir.path(), &harness_hash());

        let refusal = admit_builds(
            harness_binary(),
            Path::new("/nonexistent-kiln-e4-passthrough-binary"),
            &manifest,
        )
        .expect_err("an unreadable build must be refused");

        assert!(matches!(refusal, Refusal::Unverifiable(_)), "{refusal}");
    }
}

mod kiln_e4_21 {
    use super::*;

    #[test]
    fn the_pilot_refuses_to_change_its_frozen_operation_and_warmup_counts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output_dir = dir.path().join("evidence");

        for option in ["--operations", "--warmup"] {
            let output = run_pilot(&[
                Path::new(option),
                Path::new("5"),
                Path::new("--output-dir"),
                &output_dir,
            ]);

            assert_eq!(output.status.code(), Some(2), "{option}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("frozen"));
            assert!(!output_dir.exists(), "{option} must not start a run");
        }
    }
}

mod kiln_e4_22 {
    use super::*;

    #[test]
    fn the_pilot_refuses_a_directory_that_already_holds_a_report_and_leaves_it_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_manifest_recording(dir.path(), &harness_hash());
        let report = dir.path().join("pilot-report.txt");
        fs::write(&report, "completed evidence").expect("seed the report");

        let output = run_pilot(&[
            Path::new("--output-dir"),
            dir.path(),
            Path::new("--passthrough-binary"),
            stand_in_passthrough_binary(),
            Path::new("--manifest"),
            &manifest,
        ]);

        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("refused to start"));
        assert_eq!(fs::read_to_string(&report).unwrap(), "completed evidence");
    }
}
