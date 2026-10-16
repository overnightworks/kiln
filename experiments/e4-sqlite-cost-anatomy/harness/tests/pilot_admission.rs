//! Drives the pilot's admission checks (`admission::admit_builds`, the
//! `build-role` subcommand, the build record, the `preflight` manifest and
//! the `pilot` entry point's refusals) against the real compiled harness
//! binary. The pilot
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
use kiln_e4_harness::build_record::BuildRecord;
use kiln_e4_harness::builds::{Build, BuildEvidence, BuildReport, SOURCE_COMMIT, hash_file};
use kiln_e4_harness::cli::ManifestTarget;
use kiln_e4_harness::manifest::{BuildMismatch, Manifest, write_manifest};
use kiln_e4_harness::preflight::{CheckResult, PreflightReport, build_manifest, conclude};

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

/// What the build step records for the real harness binary and
/// `passthrough`, which the test says the step built from `SOURCE_COMMIT`.
fn record_of(passthrough: &Path) -> BuildRecord {
    BuildRecord {
        source_commit: SOURCE_COMMIT.to_string(),
        instrumented_features: Build::Instrumented.required_features().to_string(),
        passthrough_features: Build::Passthrough.required_features().to_string(),
        instrumented_sha256: hash_file(harness_binary()).expect("hash the harness"),
        passthrough_sha256: hash_file(passthrough).expect("hash the stand-in"),
    }
}

fn write_record(directory: &Path, record: &BuildRecord) -> PathBuf {
    let path = directory.join("build-record.txt");
    fs::write(&path, record.render()).expect("write record");
    path
}

/// The record of a build step that made the real harness binary and
/// `passthrough`.
fn build_record_naming(directory: &Path, passthrough: &Path) -> PathBuf {
    write_record(directory, &record_of(passthrough))
}

/// The manifest preflight writes for the real harness binary and `passthrough`.
fn preflight_manifest(directory: &Path, passthrough: &Path) -> PathBuf {
    let manifest = build_manifest(harness_binary(), passthrough, &record_of(passthrough))
        .expect("the harness and a recorded stand-in must be recorded");
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
        let record = build_record_naming(dir.path(), &passthrough);

        let admitted = admit_builds(harness_binary(), &passthrough, &manifest, &record)
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
        let record = build_record_naming(dir.path(), &vetted);
        let replacement = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "replacement");

        let refusal = admit_builds(harness_binary(), &replacement, &manifest, &record)
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
        let record = write_record(dir.path(), &record_of(&stale));

        let refusal = admit_builds(harness_binary(), &stale, &manifest, &record)
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
        let record = build_record_naming(dir.path(), &passthrough);

        let refusal = admit_builds(harness_binary(), &passthrough, &manifest_path, &record)
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
        let record = build_record_naming(dir.path(), &passthrough);

        for manifest in [incomplete, dir.path().join("missing.txt")] {
            assert_eq!(
                admit_builds(harness_binary(), &passthrough, &manifest, &record),
                Err(Refusal::ManifestUnreadable)
            );
        }
    }

    #[test]
    fn preflight_records_no_manifest_for_the_harness_given_as_both_builds() {
        let record = record_of(harness_binary());

        let result = build_manifest(harness_binary(), harness_binary(), &record);

        assert!(result.is_err());
    }

    #[test]
    fn a_failing_preflight_check_fails_closed_and_writes_no_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = dir.path().join("manifest.txt");
        let record = build_record_naming(dir.path(), &passthrough);

        let output = Command::new(harness_binary())
            .args(["preflight"])
            .args([&manifest, &passthrough, &record])
            // A watched build variable makes the build-environment check fail.
            .env("LIBSQLITE3_FLAGS", "-DSQLITE_DEBUG")
            .output()
            .expect("spawning the harness binary must succeed");

        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("failed closed"));
        assert!(!manifest.exists(), "a failed preflight must record nothing");
    }

    #[test]
    fn preflight_needs_a_manifest_path_a_passthrough_binary_and_a_build_record_or_nothing() {
        let output = run_harness(&[
            Path::new("preflight"),
            Path::new("manifest"),
            Path::new("passthrough"),
        ]);

        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage"));
    }

    mod preflight_conclusion {
        use super::*;

        fn report_with(rustc_version: CheckResult) -> PreflightReport {
            PreflightReport {
                rustc_version,
                sqlite_version: CheckResult::Pass,
                build_env: CheckResult::Pass,
                cpu_siblings: CheckResult::Pass,
                shim_vfs: CheckResult::Pass,
                source_commit: CheckResult::Pass,
            }
        }

        fn target_in(directory: &Path, passthrough: &Path) -> ManifestTarget {
            ManifestTarget {
                manifest_path: directory.join("manifest.txt"),
                passthrough_binary: passthrough.to_path_buf(),
                build_record: build_record_naming(directory, passthrough),
            }
        }

        #[test]
        fn a_preflight_whose_checks_all_passed_writes_the_manifest_of_the_recorded_builds() {
            let dir = tempfile::tempdir().expect("tempdir");
            let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
            let target = target_in(dir.path(), &passthrough);
            let mut printed = Vec::new();

            conclude(
                &report_with(CheckResult::Pass),
                Some(&target),
                harness_binary(),
                &mut printed,
            )
            .expect("a passing preflight with recorded builds concludes");

            let manifest = Manifest::read(&fs::read_to_string(&target.manifest_path).unwrap())
                .expect("the manifest was written");
            assert_eq!(manifest.source_commit, SOURCE_COMMIT);
            assert_eq!(manifest.binary_sha256, hash_file(harness_binary()).unwrap());
            assert_eq!(
                manifest.passthrough_binary_sha256,
                hash_file(&passthrough).unwrap()
            );
            let printed = String::from_utf8(printed).unwrap();
            assert!(printed.contains("PASS  rustc version"), "{printed}");
            assert!(printed.contains("manifest written to"), "{printed}");
        }

        #[test]
        fn a_failed_check_fails_closed_names_itself_and_writes_no_manifest() {
            let dir = tempfile::tempdir().expect("tempdir");
            let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
            let target = target_in(dir.path(), &passthrough);
            let mut printed = Vec::new();

            let failure = conclude(
                &report_with(CheckResult::Fail("too old".to_string())),
                Some(&target),
                harness_binary(),
                &mut printed,
            )
            .expect_err("a failing check must fail the preflight");

            assert!(failure.contains("failed closed"), "{failure}");
            assert!(
                String::from_utf8(printed)
                    .unwrap()
                    .contains("FAIL  rustc version: too old")
            );
            assert!(!target.manifest_path.exists());
        }

        #[test]
        fn a_preflight_without_a_target_only_reports_its_checks() {
            let mut printed = Vec::new();

            conclude(
                &report_with(CheckResult::Pass),
                None,
                harness_binary(),
                &mut printed,
            )
            .expect("checks alone conclude");

            assert!(
                String::from_utf8(printed)
                    .unwrap()
                    .contains("PASS  shim VFS")
            );
        }

        #[test]
        fn a_missing_build_record_or_an_unrecorded_stand_in_fails_the_preflight_without_a_manifest()
        {
            let dir = tempfile::tempdir().expect("tempdir");
            let built = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "built");
            let forged = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "forged");
            let recorded = target_in(dir.path(), &built);
            let no_record = ManifestTarget {
                build_record: dir.path().join("absent.txt"),
                ..recorded.clone()
            };
            let unrecorded = ManifestTarget {
                passthrough_binary: forged,
                ..recorded
            };

            for target in [no_record, unrecorded] {
                let failure = conclude(
                    &report_with(CheckResult::Pass),
                    Some(&target),
                    harness_binary(),
                    Vec::new(),
                )
                .expect_err("builds the record does not vouch for are not recorded");

                assert!(!failure.is_empty());
                assert!(!target.manifest_path.exists());
            }
        }
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
        let record = build_record_naming(dir.path(), &passthrough);

        let refusal = admit_builds(harness_binary(), harness_binary(), &manifest, &record)
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
        let record = build_record_naming(dir.path(), &passthrough);

        let refusal = admit_builds(
            harness_binary(),
            Path::new("/nonexistent-kiln-e4-passthrough-binary"),
            &manifest,
            &record,
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
        let record = build_record_naming(dir.path(), &passthrough);
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
            Path::new("--build-record"),
            &record,
        ]);

        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("refused to start"));
        assert_eq!(fs::read_to_string(&report).unwrap(), "completed evidence");
    }
}

mod kiln_e4_27 {
    use super::*;

    #[test]
    fn the_pilot_refuses_a_self_declared_stand_in_the_build_step_never_made() {
        let dir = tempfile::tempdir().expect("tempdir");
        let built = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "built");
        let record = build_record_naming(dir.path(), &built);
        let manifest = preflight_manifest(dir.path(), &built);
        let forged = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "forged");

        let refusal = admit_builds(harness_binary(), &forged, &manifest, &record)
            .expect_err("a binary that only claims the role and the commit must be refused");

        assert_eq!(
            refusal,
            Refusal::Unvetted(BuildMismatch::Sha256 {
                build: Build::Passthrough,
                manifest: hash_file(&built).unwrap(),
                measured: hash_file(&forged).unwrap(),
            })
        );
    }

    #[test]
    fn preflight_records_no_manifest_for_a_stand_in_the_build_record_does_not_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let built = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "built");
        let forged = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "forged");

        let result = build_manifest(harness_binary(), &forged, &record_of(&built));

        assert!(result.is_err());
    }

    #[test]
    fn the_pilot_refuses_a_missing_or_incomplete_build_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);
        let incomplete = dir.path().join("incomplete.txt");
        fs::write(&incomplete, "source_commit = \"abc\"\n").unwrap();

        for record in [incomplete, dir.path().join("missing.txt")] {
            let refusal = admit_builds(harness_binary(), &passthrough, &manifest, &record)
                .expect_err("without a build record there is nothing to trust");

            assert!(
                matches!(refusal, Refusal::BuildRecordUnreadable(_)),
                "{refusal}"
            );
        }
    }

    #[test]
    fn the_pilot_refuses_a_build_record_of_other_features_than_the_roles_need() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);
        let wrong_features = BuildRecord {
            passthrough_features: String::new(),
            ..record_of(&passthrough)
        };
        let record = write_record(dir.path(), &wrong_features);

        let refusal = admit_builds(harness_binary(), &passthrough, &manifest, &record)
            .expect_err("a passthrough build compiled without its feature is no passthrough build");

        assert!(matches!(
            refusal,
            Refusal::Unvetted(BuildMismatch::Features {
                build: Build::Passthrough,
                ..
            })
        ));
    }

    #[test]
    fn the_pilot_refuses_to_start_without_a_build_record_argument() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passthrough = stand_in_passthrough(dir.path(), SOURCE_COMMIT, "a");
        let manifest = preflight_manifest(dir.path(), &passthrough);

        let output = run_harness(&[
            Path::new("pilot"),
            Path::new("--output-dir"),
            &dir.path().join("evidence"),
            Path::new("--passthrough-binary"),
            &passthrough,
            Path::new("--manifest"),
            &manifest,
        ]);

        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("--build-record"));
    }
}
