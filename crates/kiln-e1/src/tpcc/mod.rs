//! TPC-C as E1's second corpus: the transaction types and their frozen mix,
//! the schema of Clause 1.3.1, the transaction profiles of Clauses 2.4-2.8
//! and the classified units they lower to.

mod lowering_refusal;
mod profile;
mod tables;
mod transaction;
mod units;

pub use lowering_refusal::{Blocker, LoweringRefusal};
pub use profile::{Profile, Requirement, Step};
pub use tables::{tpcc_schema, TpccTable};
pub use transaction::TpccTransaction;
pub use units::{tpcc_units, TpccUnits};
