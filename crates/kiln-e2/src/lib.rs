//! `kiln-e2`: E2's oracle.
//!
//! Each frozen family of `experiments/e2-parallel-ordered-transactions/inputs.md`
//! is one `kiln_ir::Program`, and `kiln_ir::replay` over it is the reference
//! the serial baseline and the summary algebras are checked against. The
//! seeded batch generators of the freeze's section 5 live beside the families.
//! Beside them, each family's hand-written serial fold and the element-wise
//! comparator that checks a fold against the oracle.
//! The quota family also has its summary algebra, the sequential scan over it
//! and the operation counter the growth verdict reads.
//! The observable behaviour is owned by `specs/kiln-e2.spec.md`.

pub mod accounts;
mod builders;
pub mod comparison;
pub mod corpus;
pub mod cost;
pub mod inventory;
pub mod oracle;
pub mod quota;
mod rng;
mod rows;
pub mod run;
pub mod scan;
pub mod summary;

pub use rng::{
    batch_generator, BatchSize, BatchSizeOutOfRange, Family, SplitMix64, BATCH_SIZE_MAX,
};
