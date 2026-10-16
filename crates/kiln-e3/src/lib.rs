//! `kiln-e3`: E3's instrument. The candidate term of
//! `experiments/e3-summary-synthesis/grammar.md` §1.3, the family context it
//! is checked against, and checker-form validation (protocol §3.1, checks 1
//! to 3). Evaluation is not here.
//! The observable behaviour is owned by `specs/kiln-e3.spec.md`.

mod family;
mod form;
mod malformed;
mod scan;
mod sort;
mod term;
mod validate;

pub use family::{Category, Family, Signature, StateComponent};
pub use form::{CheckerForm, ComponentDef, Field, FunctionDef};
pub use malformed::{Binding, Malformed, Path, Step};
pub use sort::{Expected, Sort};
pub use term::{Operand, Term};
pub use validate::{validate, ValidCheckerForm};
