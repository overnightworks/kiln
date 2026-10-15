//! E4 harness: the mechanics the frozen protocol specifies
//! (`experiments/e4-sqlite-cost-anatomy/protocol.md`) — the timing VFS
//! shim, the four workloads and their frozen generators, pristine-state
//! restore, the interference monitor, the preflight checks, and the run
//! driver (cells, repetitions, warm-up, interleaved order, retry budget,
//! rejected-attempt log).
//!
//! The result-bearing run itself (protocol §10's verdict quantities) does
//! not live here yet; the feasibility pilot (§13, mechanics only) does.

pub mod driver;
pub mod generators;
pub mod machine;
pub mod monitor;
pub mod pilot;
pub mod preflight;
pub mod restore;
#[cfg(test)]
pub(crate) mod test_support;
pub mod vfs;
pub mod workloads;
