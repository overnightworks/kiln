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
/// that fails: 1, every part and every name a term mentions is declared; 2,
/// every term is well-typed; 3, every operation is total. The failure is that
/// check's first defect in the order the form is written.
pub fn validate(family: &Family, form: &CheckerForm) -> Result<ValidCheckerForm, Malformed> {
    let layout = structure::declared(family, form)?;
    references::declared(family, &layout)?;
    walker::well_typed_and_total(family, &layout)?;
    Ok(ValidCheckerForm(form.clone()))
}
