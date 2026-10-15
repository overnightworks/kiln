//! E4 harness: the mechanics the frozen protocol specifies
//! (`experiments/e4-sqlite-cost-anatomy/protocol.md`) — the timing VFS
//! shim, the four workloads and their frozen generators, pristine-state
//! restore, the interference monitor, CPU affinity pinning, the
//! preflight checks, and the run driver (cells, repetitions, warm-up,
//! interleaved order, retry budget, rejected-attempt log). [`run`] wires
//! that driver into the executable `run` path: one cell's repetitions,
//! each a separately pinned, interference-gated child process.
//!
//! The result-bearing verdict itself (protocol §10's thresholds, shares,
//! and §11's output tables) does not live here yet; the feasibility
//! pilot (§13, mechanics only) does, and now also runs its own repeated
//! comparisons through the same driver.

pub mod affinity;
pub mod driver;
pub mod generators;
pub mod machine;
pub mod monitor;
pub mod pilot;
pub mod preflight;
pub mod restore;
pub mod run;
#[cfg(test)]
pub(crate) mod test_support;
pub mod vfs;
pub mod workloads;
