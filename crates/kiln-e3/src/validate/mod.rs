mod operations;
mod references;
mod structure;
mod walker;

use crate::family::Family;
use crate::form::CheckerForm;
use crate::malformed::Malformed;

/// A checker form that passed validation; later code evaluates only these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidCheckerForm(CheckerForm);

impl ValidCheckerForm {
    pub fn form(&self) -> &CheckerForm {
        &self.0
    }
}

/// Protocol §3.1's checker-form checks in order, stopping at the first check
/// that fails: 1, every name a term mentions and every count and carrier field
/// is declared; 2, every term and each component's shape is well-typed; 3,
/// every operation is total and no part is missing. The failure is that
/// check's first defect in the order the form is written.
pub fn validate(family: &Family, form: &CheckerForm) -> Result<ValidCheckerForm, Malformed> {
    references::declared(family, form)?;
    structure::declared(family, form)?;
    walker::well_typed_and_total(family, &structure::layout(family, form))?;
    Ok(ValidCheckerForm(form.clone()))
}
