//! E4 harness: the mechanics the frozen protocol specifies
//! (`experiments/e4-sqlite-cost-anatomy/protocol.md`) — the timing VFS
//! shim, the four workloads and their frozen generators, pristine-state
//! restore, the interference monitor, CPU affinity pinning, the
//! preflight checks, the run driver (cells, repetitions, warm-up,
//! interleaved order, retry budget, rejected-attempt log), the admission
//! checks the pilot passes before it measures, and the feasibility pilot
//! (§13, mechanics only) that drives it: its twelve gated cells' pairs,
//! each half a pinned measurement child watched by a pinned monitor
//! process.
//!
//! The result-bearing run and its verdict (protocol §10's thresholds,
//! shares, and §11's output tables) do not live here yet.

pub mod admission;
pub mod affinity;
pub mod build_record;
pub mod builds;
pub mod cells;
pub mod cli;
pub mod comparison;
pub mod driver;
pub mod generators;
pub mod host;
pub mod interference;
pub mod machine;
pub mod manifest;
pub mod measure_cell;
pub mod measure_error;
pub mod measure_protocol;
pub mod monitor;
pub mod pilot;
pub mod pilot_run;
pub mod preflight;
pub mod repetition_checks;
pub mod restore;
#[cfg(test)]
pub(crate) mod test_support;
pub mod vfs;
pub mod workloads;
