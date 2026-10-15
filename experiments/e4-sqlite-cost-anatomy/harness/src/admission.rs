//! What the pilot requires before it measures or writes anything
//! (protocol.md §5, §12, §13): the builds it compares must be the ones
//! preflight vetted and must each be the build they claim to be, and its
//! evidence files must not already exist. A refusal here costs nothing; a
//! run that went ahead on the wrong binary or over an earlier report would
//! produce evidence that looks valid and is not.
//!
//! The decisions ([`check_build_roles`], [`check_manifest_link`],
//! [`parse_build_role_line`]) are pure and tested without a process;
//! [`admit_builds`] and [`reserve_evidence_files`] are the thin adapters
//! that read files and spawn the `build-role` child.

use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::comparison::Build;
use crate::preflight::{self, BinaryHashCheck};

/// Name of the pilot report inside an evidence directory.
pub const REPORT_FILE_NAME: &str = "pilot-report.txt";
/// Name of the rejected-attempt log inside an evidence directory.
pub const REJECTED_LOG_FILE_NAME: &str = "rejected.log";

/// Why the pilot refuses to start.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The instrumented binary is not the one preflight hashed into the
    /// manifest (§12), or the manifest cannot say which it was.
    ManifestLink(BinaryHashCheck),
    /// A binary reports a different build than the role it was given.
    BuildRole { expected: Build, reported: Build },
    /// A binary could not be hashed or asked for its role.
    Unverifiable(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::ManifestLink(BinaryHashCheck::Mismatch { manifest, current }) => write!(
                f,
                "the instrumented binary (sha256 {current}) is not the build preflight \
                 recorded in the manifest (sha256 {manifest})"
            ),
            Refusal::ManifestLink(_) => {
                write!(
                    f,
                    "the preflight manifest is unreadable or records no binary hash"
                )
            }
            Refusal::BuildRole { expected, reported } => write!(
                f,
                "the binary given as the {} build reports itself as the {} build",
                expected.label(),
                reported.label()
            ),
            Refusal::Unverifiable(reason) => write!(f, "could not verify a build: {reason}"),
        }
    }
}

impl std::error::Error for Refusal {}

/// The two builds the pilot compares, vetted and hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedBuilds {
    pub instrumented_sha256: String,
    pub passthrough_sha256: String,
}

/// The `build-role` child's one-line stdout contract.
pub fn render_build_role_line(build: Build) -> String {
    format!("build={}", build.label())
}

/// Reads a `build-role` child's stdout back into the build it reported.
pub fn parse_build_role_line(stdout: &str) -> Option<Build> {
    stdout.trim().strip_prefix("build=").and_then(Build::parse)
}

/// The first binary must be the instrumented build and the second the
/// passthrough build; the same build twice, or the two swapped, fails
/// here (§5 compares two different builds).
pub fn check_build_roles(instrumented: Build, passthrough: Build) -> Result<(), Refusal> {
    for (expected, reported) in [
        (Build::Instrumented, instrumented),
        (Build::Passthrough, passthrough),
    ] {
        if expected != reported {
            return Err(Refusal::BuildRole { expected, reported });
        }
    }
    Ok(())
}

/// The instrumented binary's hash must be the manifest's recorded one
/// (§12: the run measures the exact bytes preflight vetted).
pub fn check_manifest_link(
    manifest_contents: &str,
    instrumented_sha256: &str,
) -> Result<(), Refusal> {
    match preflight::check_binary_hash(manifest_contents, instrumented_sha256) {
        BinaryHashCheck::Match => Ok(()),
        other => Err(Refusal::ManifestLink(other)),
    }
}

/// Asks `binary` which build it is, through its `build-role` subcommand.
pub fn query_build_role(binary: &Path) -> io::Result<Build> {
    let output = Command::new(binary).arg("build-role").output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "build-role against {} exited with {}: {}",
            binary.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_build_role_line(&stdout).ok_or_else(|| {
        io::Error::other(format!(
            "could not parse a build role from {}: {stdout:?}",
            binary.display()
        ))
    })
}

/// Vets the two binaries the pilot is about to measure: the instrumented
/// one against the preflight manifest, then each one's own report of its
/// role. Returns their hashes for the pilot report.
pub fn admit_builds(
    instrumented: &Path,
    passthrough: &Path,
    manifest_path: &Path,
) -> Result<AdmittedBuilds, Refusal> {
    let unverifiable = |what: &str, path: &Path, err: io::Error| {
        Refusal::Unverifiable(format!("{what} {}: {err}", path.display()))
    };
    let instrumented_sha256 = preflight::hash_file(instrumented)
        .map_err(|err| unverifiable("hashing", instrumented, err))?;
    let manifest_contents = fs::read_to_string(manifest_path)
        .map_err(|_| Refusal::ManifestLink(BinaryHashCheck::ManifestUnreadable))?;
    check_manifest_link(&manifest_contents, &instrumented_sha256)?;

    let instrumented_role = query_build_role(instrumented)
        .map_err(|err| unverifiable("asking the role of", instrumented, err))?;
    let passthrough_role = query_build_role(passthrough)
        .map_err(|err| unverifiable("asking the role of", passthrough, err))?;
    check_build_roles(instrumented_role, passthrough_role)?;

    let passthrough_sha256 = preflight::hash_file(passthrough)
        .map_err(|err| unverifiable("hashing", passthrough, err))?;
    Ok(AdmittedBuilds {
        instrumented_sha256,
        passthrough_sha256,
    })
}

/// The files one pilot run owns inside its evidence directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceFiles {
    pub report: PathBuf,
    pub rejected_log: PathBuf,
}

/// Creates the evidence directory if needed and both evidence files
/// exclusively, so no run can replace an earlier run's report or mix its
/// rejections into an earlier log: a file that already exists is an
/// error and stays untouched.
pub fn reserve_evidence_files(directory: &Path) -> io::Result<EvidenceFiles> {
    fs::create_dir_all(directory)?;
    let files = EvidenceFiles {
        report: directory.join(REPORT_FILE_NAME),
        rejected_log: directory.join(REJECTED_LOG_FILE_NAME),
    };
    for path in [&files.report, &files.rejected_log] {
        File::create_new(path).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("evidence file {} cannot be created: {err}", path.display()),
            )
        })?;
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight::Manifest;

    fn manifest_for(binary_sha256: &str) -> String {
        Manifest {
            binary_sha256: binary_sha256.to_string(),
            rustc_version_output: "rustc 1.99.1".to_string(),
            sqlite_version: "3.53.2".to_string(),
        }
        .render()
    }

    mod kiln_e4_09 {
        use super::*;

        #[test]
        fn the_pilot_accepts_the_binary_the_manifest_recorded() {
            assert_eq!(check_manifest_link(&manifest_for("abc"), "abc"), Ok(()));
        }

        #[test]
        fn the_pilot_refuses_a_binary_other_than_the_one_the_manifest_recorded() {
            let refusal = check_manifest_link(&manifest_for("abc"), "def").unwrap_err();
            assert_eq!(
                refusal,
                Refusal::ManifestLink(BinaryHashCheck::Mismatch {
                    manifest: "abc".to_string(),
                    current: "def".to_string()
                })
            );
            assert!(refusal.to_string().contains("abc") && refusal.to_string().contains("def"));
        }

        #[test]
        fn the_pilot_refuses_a_manifest_that_records_no_binary_hash() {
            assert_eq!(
                check_manifest_link("rustc_version = \"x\"\n", "abc"),
                Err(Refusal::ManifestLink(BinaryHashCheck::ManifestUnreadable))
            );
        }

        #[test]
        fn the_pilot_refuses_a_manifest_file_that_does_not_exist() {
            let dir = tempfile::tempdir().expect("tempdir");
            let binary = dir.path().join("binary");
            fs::write(&binary, "bytes").unwrap();
            let refusal = admit_builds(&binary, &binary, &dir.path().join("missing-manifest.txt"))
                .unwrap_err();
            assert_eq!(
                refusal,
                Refusal::ManifestLink(BinaryHashCheck::ManifestUnreadable)
            );
        }
    }

    mod kiln_e4_20 {
        use super::*;

        #[test]
        fn the_pilot_accepts_an_instrumented_build_and_a_passthrough_build() {
            assert_eq!(
                check_build_roles(Build::Instrumented, Build::Passthrough),
                Ok(())
            );
        }

        #[test]
        fn the_pilot_refuses_the_same_build_twice_or_the_two_builds_swapped() {
            for (instrumented, passthrough) in [
                (Build::Instrumented, Build::Instrumented),
                (Build::Passthrough, Build::Passthrough),
                (Build::Passthrough, Build::Instrumented),
            ] {
                assert!(
                    check_build_roles(instrumented, passthrough).is_err(),
                    "{instrumented:?} / {passthrough:?}"
                );
            }
        }

        #[test]
        fn a_refusal_names_the_role_and_what_the_binary_reported() {
            let message = check_build_roles(Build::Instrumented, Build::Instrumented)
                .unwrap_err()
                .to_string();
            assert!(message.contains("passthrough") && message.contains("instrumented"));
        }

        #[test]
        fn a_reported_build_round_trips_through_its_line() {
            for build in [Build::Instrumented, Build::Passthrough] {
                assert_eq!(
                    parse_build_role_line(&render_build_role_line(build)),
                    Some(build)
                );
            }
            assert_eq!(parse_build_role_line("build=debug"), None);
            assert_eq!(parse_build_role_line(""), None);
        }

        #[cfg(not(feature = "passthrough_shim"))]
        #[test]
        fn this_binary_reports_itself_as_the_instrumented_build() {
            assert_eq!(Build::of_this_binary(), Build::Instrumented);
        }

        #[cfg(feature = "passthrough_shim")]
        #[test]
        fn this_binary_reports_itself_as_the_passthrough_build() {
            assert_eq!(Build::of_this_binary(), Build::Passthrough);
        }
    }

    mod kiln_e4_22 {
        use super::*;

        #[test]
        fn reserving_evidence_files_creates_both_in_a_new_directory() {
            let root = tempfile::tempdir().expect("tempdir");
            let directory = root.path().join("nested").join("run");

            let files = reserve_evidence_files(&directory).unwrap();

            assert_eq!(files.report, directory.join("pilot-report.txt"));
            assert_eq!(files.rejected_log, directory.join("rejected.log"));
            assert!(files.report.is_file() && files.rejected_log.is_file());
        }

        #[test]
        fn reserving_evidence_files_leaves_an_existing_report_or_log_untouched() {
            for existing in [REPORT_FILE_NAME, REJECTED_LOG_FILE_NAME] {
                let directory = tempfile::tempdir().expect("tempdir");
                let path = directory.path().join(existing);
                fs::write(&path, "earlier evidence").unwrap();

                let err = reserve_evidence_files(directory.path()).unwrap_err();

                assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
                assert_eq!(fs::read_to_string(&path).unwrap(), "earlier evidence");
            }
        }
    }
}
