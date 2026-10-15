//! Which build a harness binary is (protocol.md §5, §12): the instrumented
//! build or the passthrough build, which source commit it was compiled
//! from, and the SHA-256 of its bytes. A binary states its role and
//! commit itself through the `build-role` subcommand; the hash is read
//! from the file. The preflight manifest records these facts
//! ([`crate::manifest`]) and the pilot's admission checks them
//! ([`crate::admission`]).
//!
//! Rendering and parsing the `build-role` line are pure and unit-tested;
//! [`BuildEvidence::collect`] is the thin adapter that hashes a file and
//! spawns the child.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

/// The commit this binary was compiled from (`build.rs`), with a
/// `-dirty` suffix when tracked files differed from it, or `unknown`
/// outside a git checkout.
pub const SOURCE_COMMIT: &str = env!("KILN_E4_SOURCE_COMMIT");

/// The two builds protocol §5's overhead bound compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    Instrumented,
    Passthrough,
}

impl Build {
    pub fn label(self) -> &'static str {
        match self {
            Build::Instrumented => "instrumented",
            Build::Passthrough => "passthrough",
        }
    }

    pub fn parse(label: &str) -> Option<Build> {
        [Build::Instrumented, Build::Passthrough]
            .into_iter()
            .find(|build| build.label() == label)
    }

    /// The role this very binary was compiled for: the `passthrough_shim`
    /// feature (see `Cargo.toml`) selects the build protocol §5's overhead
    /// bound compares against.
    pub fn of_this_binary() -> Build {
        if cfg!(feature = "passthrough_shim") {
            Build::Passthrough
        } else {
            Build::Instrumented
        }
    }

    pub fn other(self) -> Build {
        match self {
            Build::Instrumented => Build::Passthrough,
            Build::Passthrough => Build::Instrumented,
        }
    }
}

/// What a binary says about itself: its role and the commit it was
/// compiled from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub role: Build,
    pub source_commit: String,
}

impl BuildReport {
    pub fn of_this_binary() -> Self {
        BuildReport {
            role: Build::of_this_binary(),
            source_commit: SOURCE_COMMIT.to_string(),
        }
    }

    /// The `build-role` child's one-line stdout contract.
    pub fn render_line(&self) -> String {
        format!("build={} commit={}", self.role.label(), self.source_commit)
    }

    /// Reads a `build-role` child's stdout back into its report.
    pub fn parse_line(stdout: &str) -> Option<Self> {
        let field = |key: &str| {
            stdout
                .split_whitespace()
                .find_map(|part| part.strip_prefix(key))
        };
        Some(BuildReport {
            role: Build::parse(field("build=")?)?,
            source_commit: field("commit=")?.to_string(),
        })
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

/// Everything verified about one binary: its role and commit as it reports
/// them, and the hash of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildEvidence {
    pub report: BuildReport,
    pub sha256: String,
}

impl BuildEvidence {
    /// Hashes `binary` and asks it which build it is, through its
    /// `build-role` subcommand.
    pub fn collect(binary: &Path) -> io::Result<Self> {
        let sha256 = hash_file(binary)?;
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
        let report = BuildReport::parse_line(&stdout).ok_or_else(|| {
            io::Error::other(format!(
                "could not parse a build role from {}: {stdout:?}",
                binary.display()
            ))
        })?;
        Ok(BuildEvidence { report, sha256 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod kiln_e4_09 {
        use super::*;

        #[test]
        fn hash_file_is_deterministic_and_sensitive_to_content() {
            let dir = tempfile::tempdir().expect("tempdir");
            let a = dir.path().join("a");
            let b = dir.path().join("b");
            fs::write(&a, b"same bytes").unwrap();
            fs::write(&b, b"same bytes").unwrap();
            assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());

            fs::write(&b, b"different bytes").unwrap();
            assert_ne!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
        }

        #[test]
        fn hash_file_is_the_lowercase_hex_sha256() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("abc");
            fs::write(&path, b"abc").unwrap();
            assert_eq!(
                hash_file(&path).unwrap(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
        }
    }

    mod kiln_e4_20 {
        use super::*;

        fn report(role: Build) -> BuildReport {
            BuildReport {
                role,
                source_commit: "abc123".to_string(),
            }
        }

        #[test]
        fn a_reported_build_round_trips_through_its_line() {
            for role in [Build::Instrumented, Build::Passthrough] {
                let line = report(role).render_line();
                assert_eq!(BuildReport::parse_line(&line), Some(report(role)));
            }
        }

        #[test]
        fn a_line_missing_the_role_the_commit_or_naming_an_unknown_build_is_rejected() {
            for line in [
                "build=debug commit=abc123",
                "build=passthrough",
                "commit=abc123",
                "",
            ] {
                assert_eq!(BuildReport::parse_line(line), None, "{line:?}");
            }
        }

        #[test]
        fn build_labels_round_trip_and_the_other_build_is_the_opposite_role() {
            for build in [Build::Instrumented, Build::Passthrough] {
                assert_eq!(Build::parse(build.label()), Some(build));
                assert_ne!(build.other(), build);
                assert_eq!(build.other().other(), build);
            }
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

        #[test]
        fn this_binary_reports_the_commit_it_was_compiled_from() {
            let report = BuildReport::of_this_binary();
            assert_eq!(report.source_commit, SOURCE_COMMIT);
            assert!(!report.source_commit.is_empty());
        }
    }
}
