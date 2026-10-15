//! Drives the pilot's admission checks (`admission::admit_builds`, the
//! `build-role` subcommand, the `preflight` manifest and the `pilot` entry
//! point's refusals) against the real compiled harness binary. The pilot
//! never gets as far as measuring in these tests: every case ends in an
//! admission decision or a refusal.

// The compiled harness plays the instrumented build here; a passthrough
// compile of it would be the other role.
#![cfg(not(feature = "passthrough_shim"))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use kiln_e4_harness::admission::{AdmittedBuilds, Refusal, admit_builds};
use kiln_e4_harness::builds::{Build, BuildEvidence, BuildReport, SOURCE_COMMIT, hash_file};
use kiln_e4_harness::manifest::{BuildMismatch, Manifest, write_manifest};
use kiln_e4_harness::preflight::build_manifest;

fn harness_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_kiln-e4-harness"))
}

/// A stand-in for the passthrough release build, which this test crate
/// cannot compile: it answers only the `build-role` question, the one
/// admission asks, claiming `commit`. `bytes_marker` makes two stand-ins
/// differ as files while reporting the same build.
fn stand_in_passthrough(directory: &Path, commit: &str, bytes_marker: &str) -> PathBuf {
    let path = directory.join(format!("passthrough-{bytes_marker}"));
    fs::write(
        &path,
        format!("#!/bin/sh\n# {bytes_marker}\necho build=passthrough commit={commit}\n"),
    )
    .expect("write stand-in");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// The manifest preflight writes for the real harness binary and `passthrough`.
fn preflight_manifest(directory: &Path, passthrough: &Path) -> PathBuf {
    let manifest = build_manifest(harness_binary(), passthrough)
        .expect("the harness and a matching stand-in must be recorded");
    let path = directory.join("manifest.txt");
    write_manifest(&path, &manifest).expect("write manifest");
    path
}

fn run_harness(arguments: &[&Path]) -> Output {
    Command::new(harness_binary())
        .args(arguments)
        .output()
        .expect("spawning the harness binary must succeed")
}

mod kiln_e4_09 {
    use super::*;

    #[test]
    fn preflight_records_the_commit_and_both_builds_hashes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");

        let manifest = Manifest::read(
            &fs::read_to_string(preflight_manifest(dir.path(), &passthrough)).unwrap(),
        )
        .expect("a manifest preflight wrote reads back");

        assert_eq!(manifest.source_commit, SOURCE_COMMIT);
        assert_eq!(manifest.binary_sha256, hash_file(harness_binary()).unwrap());
        assert_eq!(
            manifest.passthrough_binary_sha256,
            hash_file(&passthrough).unwrap()
        );
    }

    #[test]
    fn the_pilot_admits_the_two_builds_the_manifest_vets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);

        let admitted = admit_builds(harness_binary(), &passthrough, &manifest)
            .expect("the vetted builds must be admitted");

        assert_eq!(
            admitted,
            AdmittedBuilds {
                source_commit: SOURCE_COMMIT.to_string(),
                instrumented_sha256: hash_file(harness_binary()).unwrap(),
                passthrough_sha256: hash_file(&passthrough).unwrap(),
            }
        );
    }

    #[test]
    fn the_pilot_refuses_a_replaced_passthrough_binary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vetted = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "vetted");
        let manifest = preflight_manifest(dir.path(), &vetted);
        let replacement = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "replacement");

        let refusal = admit_builds(harness_binary(), &replacement, &manifest)
            .expect_err("other bytes than the vetted passthrough build must be refused");

        assert_eq!(
            refusal,
            Refusal::Unvetted(BuildMismatch::Sha256 {
                build: Build::Passthrough,
                manifest: hash_file(&vetted).unwrap(),
                measured: hash_file(&replacement).unwrap(),
            })
        );
    }

    #[test]
    fn the_pilot_refuses_a_passthrough_binary_built_from_another_commit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vetted = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "vetted");
        let manifest = preflight_manifest(dir.path(), &vetted);
        let stale = stand_in_passthrough(dir.path(), "0000stale", "stale");

        let refusal = admit_builds(harness_binary(), &stale, &manifest)
            .expect_err("a build of another commit must be refused");

        assert!(matches!(
            refusal,
            Refusal::Unvetted(BuildMismatch::SourceCommit { .. })
        ));
    }

    #[test]
    fn the_pilot_refuses_a_manifest_recorded_for_a_different_instrumented_binary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest_path = dir.path().join("manifest.txt");
        let manifest = Manifest {
            source_commit: SOURCE_COMMIT.to_string(),
            binary_sha256: "0".repeat(64),
            passthrough_binary_sha256: hash_file(&passthrough).unwrap(),
            rustc_version_output: "rustc 1.99.1".to_string(),
            sqlite_version: "3.53.2".to_string(),
        };
        write_manifest(&manifest_path, &manifest).unwrap();

        let refusal = admit_builds(harness_binary(), &passthrough, &manifest_path)
            .expect_err("a manifest for other bytes must be refused");

        assert!(matches!(
            refusal,
            Refusal::Unvetted(BuildMismatch::Sha256 {
                build: Build::Instrumented,
                ..
            })
        ));
    }

    #[test]
    fn the_pilot_refuses_a_missing_or_incomplete_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let incomplete = dir.path().join("incomplete.txt");
        fs::write(&incomplete, "binary_sha256 = \"abc\"\n").unwrap();

        for manifest in [incomplete, dir.path().join("missing.txt")] {
            assert_eq!(
                admit_builds(harness_binary(), &passthrough, &manifest),
                Err(Refusal::ManifestUnreadable)
            );
        }
    }

    #[test]
    fn preflight_records_no_manifest_for_the_harness_given_as_both_builds() {
        let result = build_manifest(harness_binary(), harness_binary());

        assert!(result.is_err());
    }

    #[test]
    fn preflight_needs_both_a_manifest_path_and_a_passthrough_binary_or_neither() {
        let output = run_harness(&[Path::new("preflight"), Path::new("manifest-only")]);

        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage"));
    }
}

mod kiln_e4_20 {
    use super::*;

    #[test]
    fn a_harness_binary_reports_the_build_and_commit_it_was_compiled_as() {
        let evidence = BuildEvidence::collect(harness_binary()).expect("build-role");

        assert_eq!(evidence.report, BuildReport::of_this_binary());
        assert_eq!(evidence.sha256, hash_file(harness_binary()).unwrap());
    }

    #[test]
    fn the_pilot_refuses_the_same_build_given_as_both_roles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);

        let refusal = admit_builds(harness_binary(), harness_binary(), &manifest)
            .expect_err("one build cannot be both roles");

        assert!(matches!(
            refusal,
            Refusal::Unvetted(BuildMismatch::Role { .. })
        ));
    }

    #[test]
    fn the_pilot_refuses_a_passthrough_binary_that_cannot_report_its_role() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);

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
            let output = run_harness(&[
                Path::new("pilot"),
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
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);
        let report = dir.path().join("pilot-report.txt");
        fs::write(&report, "completed evidence").expect("seed the report");

        let output = run_harness(&[
            Path::new("pilot"),
            Path::new("--output-dir"),
            dir.path(),
            Path::new("--passthrough-binary"),
            &passthrough,
            Path::new("--manifest"),
            &manifest,
        ]);

        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("refused to start"));
        assert_eq!(fs::read_to_string(&report).unwrap(), "completed evidence");
    }
}
