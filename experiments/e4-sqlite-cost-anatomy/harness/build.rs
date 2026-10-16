//! Embeds the commit this binary is compiled from, so a binary can say
//! which source it came from (`src/builds.rs`; protocol.md §12).

use std::process::Command;

fn git_output(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn source_commit() -> String {
    let Some(commit) = git_output(&["rev-parse", "HEAD"]) else {
        return "unknown".to_string();
    };
    let tracked_changes = git_output(&["status", "--porcelain", "--untracked-files=no"]);
    match tracked_changes {
        Some(changes) if changes.is_empty() => commit,
        _ => format!("{commit}-dirty"),
    }
}

fn main() {
    println!("cargo:rustc-env=KILN_E4_SOURCE_COMMIT={}", source_commit());
    // A path that never exists makes cargo rerun this script on every
    // build, so the embedded commit can never be a stale one.
    println!("cargo:rerun-if-changed=.kiln-e4-always-rerun");
}
