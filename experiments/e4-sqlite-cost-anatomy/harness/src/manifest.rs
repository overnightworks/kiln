//! The preflight manifest (protocol.md §12): what preflight vetted and
//! recorded, and what a later run must find unchanged. It names the
//! source commit and the SHA-256 of both builds — the instrumented one
//! preflight ran from and the passthrough one §5's overhead bound
//! compares it against — "the exact bytes producing the result, not a
//! rebuild in between".
//!
//! [`Manifest::record`] (what preflight writes) and
//! [`Manifest::check_builds`] (what the pilot's admission requires) share
//! one decision, so a manifest preflight accepts is a manifest admission
//! accepts for the same binaries. Rendering, reading and checking are
//! pure and unit-tested; [`write_manifest`] is the thin file adapter.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::builds::{Build, BuildEvidence};

/// Why a binary is not the build the manifest vets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildMismatch {
    /// The binary reports a different build than the role it was given.
    Role { expected: Build, reported: Build },
    /// The binary was compiled from another commit than the manifest names.
    SourceCommit {
        build: Build,
        manifest: String,
        reported: String,
    },
    /// The binary's bytes are not the ones the manifest hashed.
    Sha256 {
        build: Build,
        manifest: String,
        measured: String,
    },
    /// The build record names other cargo features for a build than the
    /// ones its role is compiled with.
    Features {
        build: Build,
        recorded: String,
        required: &'static str,
    },
}

impl fmt::Display for BuildMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildMismatch::Role { expected, reported } => write!(
                f,
                "the binary given as the {} build reports itself as the {} build",
                expected.label(),
                reported.label()
            ),
            BuildMismatch::SourceCommit {
                build,
                manifest,
                reported,
            } => write!(
                f,
                "the {} binary was compiled from commit {reported}, not the manifest's {manifest}",
                build.label()
            ),
            BuildMismatch::Sha256 {
                build,
                manifest,
                measured,
            } => write!(
                f,
                "the {} binary (sha256 {measured}) is not the build recorded \
                 in the manifest or build record (sha256 {manifest})",
                build.label()
            ),
            BuildMismatch::Features {
                build,
                recorded,
                required,
            } => write!(
                f,
                "the build record says the {} build was compiled with features {recorded:?}, \
                 not {required:?}",
                build.label()
            ),
        }
    }
}

impl std::error::Error for BuildMismatch {}

/// The identity of the two builds a pilot may measure: one commit and the
/// bytes of each. A manifest and a build record each vet builds against
/// one, so the two cannot disagree on what a matching build is.
#[derive(Debug, Clone, Copy)]
pub struct VettedBuilds<'a> {
    pub source_commit: &'a str,
    pub instrumented_sha256: &'a str,
    pub passthrough_sha256: &'a str,
}

impl VettedBuilds<'_> {
    /// Whether `instrumented` and `passthrough` are these builds: each
    /// reports its own role, was compiled from this commit, and has this
    /// hash.
    pub fn check(
        &self,
        instrumented: &BuildEvidence,
        passthrough: &BuildEvidence,
    ) -> Result<(), BuildMismatch> {
        for (expected, evidence, sha256) in [
            (Build::Instrumented, instrumented, self.instrumented_sha256),
            (Build::Passthrough, passthrough, self.passthrough_sha256),
        ] {
            if evidence.report.role != expected {
                return Err(BuildMismatch::Role {
                    expected,
                    reported: evidence.report.role,
                });
            }
            if evidence.report.source_commit != self.source_commit {
                return Err(BuildMismatch::SourceCommit {
                    build: expected,
                    manifest: self.source_commit.to_string(),
                    reported: evidence.report.source_commit.clone(),
                });
            }
            if evidence.sha256 != sha256 {
                return Err(BuildMismatch::Sha256 {
                    build: expected,
                    manifest: sha256.to_string(),
                    measured: evidence.sha256.clone(),
                });
            }
        }
        Ok(())
    }
}

/// The preflight manifest (protocol §12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub source_commit: String,
    pub binary_sha256: String,
    pub passthrough_binary_sha256: String,
    pub rustc_version_output: String,
    pub sqlite_version: String,
}

impl Manifest {
    /// What preflight records for the two builds it was handed: refuses
    /// unless the first is the instrumented build, the second the
    /// passthrough build, and both were compiled from one commit.
    pub fn record(
        instrumented: &BuildEvidence,
        passthrough: &BuildEvidence,
        rustc_version_output: String,
        sqlite_version: String,
    ) -> Result<Manifest, BuildMismatch> {
        let manifest = Manifest {
            source_commit: instrumented.report.source_commit.clone(),
            binary_sha256: instrumented.sha256.clone(),
            passthrough_binary_sha256: passthrough.sha256.clone(),
            rustc_version_output,
            sqlite_version,
        };
        manifest.check_builds(instrumented, passthrough)?;
        Ok(manifest)
    }

    /// Whether `instrumented` and `passthrough` are the two builds this
    /// manifest vets: each reports its own role, was compiled from the
    /// manifest's commit, and has the manifest's hash.
    pub fn check_builds(
        &self,
        instrumented: &BuildEvidence,
        passthrough: &BuildEvidence,
    ) -> Result<(), BuildMismatch> {
        VettedBuilds {
            source_commit: &self.source_commit,
            instrumented_sha256: &self.binary_sha256,
            passthrough_sha256: &self.passthrough_binary_sha256,
        }
        .check(instrumented, passthrough)
    }

    /// Renders the manifest as simple `key = value` lines — the same
    /// hand-rolled format `rust-toolchain.toml` already uses here, so one
    /// small, stable record pulls in no serialization dependency.
    pub fn render(&self) -> String {
        format!(
            "source_commit = \"{}\"\nbinary_sha256 = \"{}\"\npassthrough_binary_sha256 = \"{}\"\n\
             rustc_version = \"{}\"\nsqlite_version = \"{}\"\n",
            self.source_commit,
            self.binary_sha256,
            self.passthrough_binary_sha256,
            self.rustc_version_output.trim(),
            self.sqlite_version,
        )
    }

    /// Reads a rendered manifest back; `None` unless every field is there.
    pub fn read(contents: &str) -> Option<Manifest> {
        Some(Manifest {
            source_commit: read_quoted_field(contents, "source_commit")?,
            binary_sha256: read_quoted_field(contents, "binary_sha256")?,
            passthrough_binary_sha256: read_quoted_field(contents, "passthrough_binary_sha256")?,
            rustc_version_output: read_quoted_field(contents, "rustc_version")?,
            sqlite_version: read_quoted_field(contents, "sqlite_version")?,
        })
    }
}

/// Reads one quoted `key = "value"` line's value out of a rendered
/// manifest, matching the whole key so `binary_sha256` never reads
/// `passthrough_binary_sha256`'s line.
pub(crate) fn read_quoted_field(manifest_contents: &str, key: &str) -> Option<String> {
    manifest_contents.lines().find_map(|line| {
        let (line_key, value) = line.trim().split_once('=')?;
        if line_key.trim() != key {
            return None;
        }
        let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
        Some(value.to_string())
    })
}

/// Writes the manifest to `path`. The thin, untested adapter:
/// [`Manifest::render`] and [`Manifest::read`] carry the logic.
pub fn write_manifest(path: &Path, manifest: &Manifest) -> io::Result<()> {
    fs::write(path, manifest.render())
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

    fn instrumented() -> BuildEvidence {
        evidence(Build::Instrumented, "c0ffee", "aaa")
    }

    fn passthrough() -> BuildEvidence {
        evidence(Build::Passthrough, "c0ffee", "bbb")
    }

    fn recorded() -> Manifest {
        Manifest::record(
            &instrumented(),
            &passthrough(),
            "rustc 1.99.1 (58cf8f9c3 2026-10-13)\n".to_string(),
            "3.53.2".to_string(),
        )
        .expect("two matching builds must be recorded")
    }

    mod kiln_e4_09 {
        use super::*;

        #[test]
        fn the_manifest_records_the_commit_and_both_builds_hashes() {
            let manifest = recorded();
            assert_eq!(manifest.source_commit, "c0ffee");
            assert_eq!(manifest.binary_sha256, "aaa");
            assert_eq!(manifest.passthrough_binary_sha256, "bbb");
        }

        #[test]
        fn a_rendered_manifest_reads_back_with_every_field() {
            let manifest = recorded();
            let read = Manifest::read(&manifest.render()).expect("a rendered manifest reads back");
            assert_eq!(
                read,
                Manifest {
                    rustc_version_output: "rustc 1.99.1 (58cf8f9c3 2026-10-13)".to_string(),
                    ..manifest
                }
            );
        }

        #[test]
        fn a_manifest_missing_a_field_or_not_a_manifest_does_not_read() {
            let rendered = recorded().render();
            for key in [
                "source_commit",
                "binary_sha256",
                "passthrough_binary_sha256",
                "rustc_version",
                "sqlite_version",
            ] {
                let without: String = rendered
                    .lines()
                    .filter(|line| !line.starts_with(&format!("{key} ")))
                    .map(|line| format!("{line}\n"))
                    .collect();
                assert_eq!(Manifest::read(&without), None, "without {key}");
            }
            assert_eq!(Manifest::read("not a manifest at all"), None);
        }

        #[test]
        fn the_instrumented_hash_is_never_read_from_the_passthrough_hash_line() {
            let only_passthrough = "passthrough_binary_sha256 = \"bbb\"\n";
            assert_eq!(read_quoted_field(only_passthrough, "binary_sha256"), None);
        }

        #[test]
        fn write_manifest_writes_a_file_read_can_read_back() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("manifest.txt");
            let manifest = recorded();
            write_manifest(&path, &manifest).unwrap();
            let contents = fs::read_to_string(&path).unwrap();
            assert_eq!(
                Manifest::read(&contents).map(|read| read.binary_sha256),
                Some(manifest.binary_sha256)
            );
        }

        #[test]
        fn the_vetted_builds_pass_the_manifest_check() {
            assert_eq!(
                recorded().check_builds(&instrumented(), &passthrough()),
                Ok(())
            );
        }

        #[test]
        fn a_replaced_passthrough_binary_is_refused() {
            let replaced = evidence(Build::Passthrough, "c0ffee", "other-bytes");
            assert_eq!(
                recorded().check_builds(&instrumented(), &replaced),
                Err(BuildMismatch::Sha256 {
                    build: Build::Passthrough,
                    manifest: "bbb".to_string(),
                    measured: "other-bytes".to_string(),
                })
            );
        }

        #[test]
        fn a_replaced_instrumented_binary_is_refused() {
            let replaced = evidence(Build::Instrumented, "c0ffee", "other-bytes");
            assert_eq!(
                recorded().check_builds(&replaced, &passthrough()),
                Err(BuildMismatch::Sha256 {
                    build: Build::Instrumented,
                    manifest: "aaa".to_string(),
                    measured: "other-bytes".to_string(),
                })
            );
        }

        #[test]
        fn a_binary_compiled_from_another_commit_is_refused_even_with_the_recorded_hash() {
            let stale = evidence(Build::Passthrough, "deadbeef", "bbb");
            assert_eq!(
                recorded().check_builds(&instrumented(), &stale),
                Err(BuildMismatch::SourceCommit {
                    build: Build::Passthrough,
                    manifest: "c0ffee".to_string(),
                    reported: "deadbeef".to_string(),
                })
            );
        }

        #[test]
        fn preflight_records_no_manifest_for_builds_of_different_commits() {
            let stale = evidence(Build::Passthrough, "deadbeef", "bbb");
            let result = Manifest::record(
                &instrumented(),
                &stale,
                "rustc".to_string(),
                "3.53.2".to_string(),
            );
            assert!(matches!(result, Err(BuildMismatch::SourceCommit { .. })));
        }

        #[test]
        fn a_mismatch_names_the_build_and_both_values() {
            let message = BuildMismatch::Sha256 {
                build: Build::Passthrough,
                manifest: "abc".to_string(),
                measured: "def".to_string(),
            }
            .to_string();
            for expected in ["passthrough", "abc", "def"] {
                assert!(message.contains(expected), "{message}");
            }
        }
    }

    mod kiln_e4_20 {
        use super::*;

        #[test]
        fn the_manifest_refuses_the_same_build_twice_or_the_two_swapped() {
            let manifest = recorded();
            for (first, second) in [
                (instrumented(), instrumented()),
                (passthrough(), passthrough()),
                (passthrough(), instrumented()),
            ] {
                assert!(
                    matches!(
                        manifest.check_builds(&first, &second),
                        Err(BuildMismatch::Role { .. })
                    ),
                    "{first:?} / {second:?}"
                );
            }
        }

        #[test]
        fn preflight_records_no_manifest_for_the_same_build_given_twice() {
            let result = Manifest::record(
                &instrumented(),
                &instrumented(),
                "rustc".to_string(),
                "3.53.2".to_string(),
            );
            assert!(matches!(result, Err(BuildMismatch::Role { .. })));
        }

        #[test]
        fn a_role_mismatch_names_the_expected_and_the_reported_build() {
            let message = BuildMismatch::Role {
                expected: Build::Instrumented,
                reported: Build::Passthrough,
            }
            .to_string();
            assert!(message.contains("instrumented") && message.contains("passthrough"));
        }
    }
}
