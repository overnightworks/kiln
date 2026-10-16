//! The build record (protocol.md §12): what a clean build of the reviewed
//! commit produced. `build-pilot-binaries.sh` compiles both
//! configurations from a clean checkout of that commit and writes the
//! commit, the cargo features of each build and the SHA-256 of each
//! resulting binary here. A binary cannot vouch for itself — a script
//! can print any role and commit — so preflight and the pilot's admission
//! hold the binaries they measure against this record, which only the
//! build step writes.
//!
//! Rendering, reading and checking are pure and unit-tested;
//! [`BuildRecord::read_file`] is the thin file adapter.

use std::fs;
use std::io;
use std::path::Path;

use crate::builds::{Build, BuildEvidence};
use crate::manifest::{BuildMismatch, VettedBuilds, read_quoted_field};

/// What the build step recorded about the two builds it made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRecord {
    pub source_commit: String,
    pub instrumented_features: String,
    pub passthrough_features: String,
    pub instrumented_sha256: String,
    pub passthrough_sha256: String,
}

impl BuildRecord {
    /// Whether `instrumented` and `passthrough` are the builds this record
    /// made: the record names the features each role is compiled with,
    /// and each binary reports its role and the record's commit and has
    /// the record's hash.
    pub fn check_builds(
        &self,
        instrumented: &BuildEvidence,
        passthrough: &BuildEvidence,
    ) -> Result<(), BuildMismatch> {
        for (build, recorded) in [
            (Build::Instrumented, &self.instrumented_features),
            (Build::Passthrough, &self.passthrough_features),
        ] {
            if recorded != build.required_features() {
                return Err(BuildMismatch::Features {
                    build,
                    recorded: recorded.clone(),
                    required: build.required_features(),
                });
            }
        }
        VettedBuilds {
            source_commit: &self.source_commit,
            instrumented_sha256: &self.instrumented_sha256,
            passthrough_sha256: &self.passthrough_sha256,
        }
        .check(instrumented, passthrough)
    }

    /// The record as `key = "value"` lines, the format the build script
    /// writes and the manifest shares.
    pub fn render(&self) -> String {
        format!(
            "source_commit = \"{}\"\ninstrumented_features = \"{}\"\npassthrough_features = \"{}\"\n\
             instrumented_sha256 = \"{}\"\npassthrough_sha256 = \"{}\"\n",
            self.source_commit,
            self.instrumented_features,
            self.passthrough_features,
            self.instrumented_sha256,
            self.passthrough_sha256,
        )
    }

    /// Reads a rendered record back; `None` unless every field is there.
    pub fn read(contents: &str) -> Option<BuildRecord> {
        Some(BuildRecord {
            source_commit: read_quoted_field(contents, "source_commit")?,
            instrumented_features: read_quoted_field(contents, "instrumented_features")?,
            passthrough_features: read_quoted_field(contents, "passthrough_features")?,
            instrumented_sha256: read_quoted_field(contents, "instrumented_sha256")?,
            passthrough_sha256: read_quoted_field(contents, "passthrough_sha256")?,
        })
    }

    /// Reads the record at `path`; a missing file or an incomplete record
    /// is an error, never a default.
    pub fn read_file(path: &Path) -> io::Result<BuildRecord> {
        BuildRecord::read(&fs::read_to_string(path)?).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a complete build record", path.display()),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builds::BuildReport;

    fn evidence(role: Build, commit: &str, sha256: &str) -> BuildEvidence {
        BuildEvidence {
            report: BuildReport {
                role,
                source_commit: commit.to_string(),
            },
            sha256: sha256.to_string(),
        }
    }

    fn record() -> BuildRecord {
        BuildRecord {
            source_commit: "c0ffee".to_string(),
            instrumented_features: String::new(),
            passthrough_features: "passthrough_shim".to_string(),
            instrumented_sha256: "aaa".to_string(),
            passthrough_sha256: "bbb".to_string(),
        }
    }

    mod kiln_e4_27 {
        use super::*;

        #[test]
        fn a_rendered_record_reads_back_with_every_field() {
            assert_eq!(BuildRecord::read(&record().render()), Some(record()));
        }

        #[test]
        fn a_record_missing_a_field_or_not_a_record_does_not_read() {
            let rendered = record().render();
            for key in [
                "source_commit",
                "instrumented_features",
                "passthrough_features",
                "instrumented_sha256",
                "passthrough_sha256",
            ] {
                let without: String = rendered
                    .lines()
                    .filter(|line| !line.starts_with(&format!("{key} ")))
                    .map(|line| format!("{line}\n"))
                    .collect();
                assert_eq!(BuildRecord::read(&without), None, "without {key}");
            }
            assert_eq!(BuildRecord::read("nothing recorded"), None);
        }

        #[test]
        fn reading_a_missing_or_incomplete_record_file_is_an_error() {
            let dir = tempfile::tempdir().expect("tempdir");
            let incomplete = dir.path().join("incomplete");
            fs::write(&incomplete, "source_commit = \"c0ffee\"\n").unwrap();

            assert!(BuildRecord::read_file(&incomplete).is_err());
            assert!(BuildRecord::read_file(&dir.path().join("missing")).is_err());
        }

        #[test]
        fn a_record_read_from_its_file_is_the_record_written() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("build-record.txt");
            fs::write(&path, record().render()).unwrap();

            assert_eq!(BuildRecord::read_file(&path).unwrap(), record());
        }

        #[test]
        fn the_builds_the_record_made_pass_its_check() {
            assert_eq!(
                record().check_builds(
                    &evidence(Build::Instrumented, "c0ffee", "aaa"),
                    &evidence(Build::Passthrough, "c0ffee", "bbb"),
                ),
                Ok(())
            );
        }

        #[test]
        fn a_binary_with_other_bytes_than_the_record_is_refused_whatever_it_claims() {
            let stand_in = evidence(Build::Passthrough, "c0ffee", "stand-in-bytes");
            assert_eq!(
                record().check_builds(&evidence(Build::Instrumented, "c0ffee", "aaa"), &stand_in),
                Err(BuildMismatch::Sha256 {
                    build: Build::Passthrough,
                    manifest: "bbb".to_string(),
                    measured: "stand-in-bytes".to_string(),
                })
            );
        }

        #[test]
        fn a_record_of_a_build_with_other_features_than_its_role_is_refused() {
            for (instrumented_features, passthrough_features, build) in [
                ("passthrough_shim", "passthrough_shim", Build::Instrumented),
                ("", "", Build::Passthrough),
            ] {
                let wrong = BuildRecord {
                    instrumented_features: instrumented_features.to_string(),
                    passthrough_features: passthrough_features.to_string(),
                    ..record()
                };

                let result = wrong.check_builds(
                    &evidence(Build::Instrumented, "c0ffee", "aaa"),
                    &evidence(Build::Passthrough, "c0ffee", "bbb"),
                );

                assert!(
                    matches!(result, Err(BuildMismatch::Features { build: found, .. }) if found == build),
                    "{result:?}"
                );
            }
        }

        #[test]
        fn a_feature_mismatch_names_the_build_and_both_feature_sets() {
            let message = BuildMismatch::Features {
                build: Build::Passthrough,
                recorded: "".to_string(),
                required: "passthrough_shim",
            }
            .to_string();
            for expected in ["passthrough", "passthrough_shim"] {
                assert!(message.contains(expected), "{message}");
            }
        }
    }
}
