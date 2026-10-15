//! `kiln-e1`: E1's unit record contract, coverage arithmetic, evidence gate and
//! verdict.
//!
//! The extractors emit [`UnitRecord`]s, the classifier turns them into
//! [`ClassifiedUnit`]s, and [`evaluate`] computes the gate that
//! `docs/experiments.md` ("Gating numerator and verdict") defines. The
//! observable behaviour is owned by `specs/kiln-e1.spec.md`.

mod classified;
mod command;
mod coverage;
mod evaluation;
mod jsonl;
mod manifest;
mod tpcc;
mod unit;
mod verdict;

pub use classified::{
    Classification, ClassifiedUnit, ContractViolation, Evidence, ProvenCategory, Reason,
    S4Exclusion,
};
pub use command::{run_verdict_command, CommandError};
pub use coverage::{median, Percent, Tally};
pub use evaluation::{evaluate, Evaluation, Refusal};
pub use jsonl::{parse_units, render_units, JsonlError};
pub use manifest::{Manifest, ManifestError};
pub use tpcc::TpccTransaction;
pub use unit::{ApplicationId, SiteLocation, UnitRecord, UnitRule};
pub use verdict::Verdict;
