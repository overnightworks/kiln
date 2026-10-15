//! E4 harness: the mechanics the frozen protocol specifies
//! (`experiments/e4-sqlite-cost-anatomy/protocol.md`) — the timing VFS
//! shim, the four workloads and their frozen generators, pristine-state
//! restore, the interference monitor, and the preflight checks.
//!
//! This crate builds mechanics only. No benchmark, pilot, or
//! result-bearing run lives here; that is issue #13's slice.

pub mod generators;
pub mod machine;
pub mod monitor;
pub mod preflight;
pub mod restore;
pub mod vfs;
pub mod workloads;
