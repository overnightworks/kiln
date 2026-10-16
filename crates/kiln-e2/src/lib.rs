//! `kiln-e2`: E2's oracle.
//!
//! Each frozen family of `experiments/e2-parallel-ordered-transactions/inputs.md`
//! is one `kiln_ir::Program`, and `kiln_ir::replay` over it is the reference
//! the serial baseline and the summary algebras are checked against. The
//! seeded batch generators of the freeze's section 5 live beside the families.
//! The observable behaviour is owned by `specs/kiln-e2.spec.md`.

pub mod accounts;
mod builders;
pub mod inventory;
pub mod quota;
mod rng;
mod rows;

pub use rng::{
    batch_generator, BatchSize, BatchSizeOutOfRange, Family, SplitMix64, BATCH_SIZE_MAX,
};
