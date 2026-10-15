//! What the pilot requires before it measures or writes anything
//! (protocol.md §5, §12, §13): the two builds it compares must be the ones
//! preflight vetted — the exact bytes, the commit and the role the
//! manifest records — and its evidence files must not already exist. A
//! refusal here costs nothing; a run that went ahead on the wrong binary
//! or over an earlier report would produce evidence that looks valid and
//! is not.
//!
//! The decision lives in [`crate::manifest::Manifest::check_builds`];
//! [`admit_builds`] and [`reserve_evidence_files`] are the thin adapters
//! that read files and spawn the `build-role` child.

use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::builds::BuildEvidence;
use crate::manifest::{BuildMismatch, Manifest};

/// Name of the pilot report inside an evidence directory.
pub const REPORT_FILE_NAME: &str = "pilot-report.txt";
/// Name of the rejected-attempt log inside an evidence directory.
pub const REJECTED_LOG_FILE_NAME: &str = "rejected.log";
/// Name of the monitor-sample log inside an evidence directory.
pub const SAMPLES_LOG_FILE_NAME: &str = "monitor-samples.log";

/// Why the pilot refuses to start.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The manifest cannot be read or lacks a recorded field (§12).
    ManifestUnreadable,
    /// A binary is not the build the manifest vets.
    Unvetted(BuildMismatch),
    /// A binary could not be hashed or asked for its role.
    Unverifiable(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::ManifestUnreadable => {
                write!(f, "the preflight manifest is unreadable or incomplete")
            }
            Refusal::Unvetted(mismatch) => write!(f, "{mismatch}"),
            Refusal::Unverifiable(reason) => write!(f, "could not verify a build: {reason}"),
        }
    }
}

impl std::error::Error for Refusal {}

/// The two builds the pilot compares, vetted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedBuilds {
    pub source_commit: String,
    pub instrumented_sha256: String,
    pub passthrough_sha256: String,
}

/// Vets the two binaries the pilot is about to measure against the
/// preflight manifest: the exact bytes, the source commit and the role of
/// each (§5, §12). Returns what the pilot report names.
pub fn admit_builds(
    instrumented: &Path,
    passthrough: &Path,
    manifest_path: &Path,
) -> Result<AdmittedBuilds, Refusal> {
    let manifest = fs::read_to_string(manifest_path)
        .ok()
        .and_then(|contents| Manifest::read(&contents))
        .ok_or(Refusal::ManifestUnreadable)?;
    let collect = |binary: &Path| {
        BuildEvidence::collect(binary)
            .map_err(|err| Refusal::Unverifiable(format!("verifying {}: {err}", binary.display())))
    };
    let instrumented_evidence = collect(instrumented)?;
    let passthrough_evidence = collect(passthrough)?;
    manifest
        .check_builds(&instrumented_evidence, &passthrough_evidence)
        .map_err(Refusal::Unvetted)?;
    Ok(AdmittedBuilds {
        source_commit: manifest.source_commit,
        instrumented_sha256: instrumented_evidence.sha256,
        passthrough_sha256: passthrough_evidence.sha256,
    })
}

/// The files one pilot run owns inside its evidence directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceFiles {
    pub report: PathBuf,
    pub rejected_log: PathBuf,
    pub samples_log: PathBuf,
}

/// Creates the evidence directory if needed and every evidence file
/// exclusively, so no run can replace an earlier run's report or mix its
/// rejections or samples into an earlier log: a file that already exists
/// is an error and stays untouched.
pub fn reserve_evidence_files(directory: &Path) -> io::Result<EvidenceFiles> {
    fs::create_dir_all(directory)?;
    let files = EvidenceFiles {
        report: directory.join(REPORT_FILE_NAME),
        rejected_log: directory.join(REJECTED_LOG_FILE_NAME),
        samples_log: directory.join(SAMPLES_LOG_FILE_NAME),
    };
    for path in [&files.report, &files.rejected_log, &files.samples_log] {
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

    mod kiln_e4_09 {
        use super::*;
        use crate::builds::Build;

        #[test]
        fn a_refusal_says_why_the_pilot_will_not_start() {
            let unvetted = Refusal::Unvetted(BuildMismatch::Role {
                expected: Build::Passthrough,
                reported: Build::Instrumented,
            });
            for (refusal, expected) in [
                (Refusal::ManifestUnreadable, "manifest"),
                (unvetted, "passthrough"),
                (
                    Refusal::Unverifiable("no such file".to_string()),
                    "no such file",
                ),
            ] {
                let message = refusal.to_string();
                assert!(message.contains(expected), "{message}");
            }
        }
    }

    mod kiln_e4_22 {
        use super::*;

        #[test]
        fn reserving_evidence_files_creates_all_three_in_a_new_directory() {
            let root = tempfile::tempdir().expect("tempdir");
            let directory = root.path().join("nested").join("run");

            let files = reserve_evidence_files(&directory).unwrap();

            assert_eq!(files.report, directory.join("pilot-report.txt"));
            assert_eq!(files.rejected_log, directory.join("rejected.log"));
            assert_eq!(files.samples_log, directory.join("monitor-samples.log"));
            assert!(files.report.is_file());
            assert!(files.rejected_log.is_file());
            assert!(files.samples_log.is_file());
        }

        #[test]
        fn reserving_evidence_files_leaves_an_existing_report_or_log_untouched() {
            for existing in [
                REPORT_FILE_NAME,
                REJECTED_LOG_FILE_NAME,
                SAMPLES_LOG_FILE_NAME,
            ] {
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
