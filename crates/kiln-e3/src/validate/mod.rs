mod operations;
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

/// Protocol §3.1's checker-form checks, stopping at the first failure:
/// every part is declared, then every term is well-typed and total.
///
/// Within one term a node's declaredness is checked before its type, but the
/// two are not separate passes over the whole form.
pub fn validate(family: &Family, form: &CheckerForm) -> Result<ValidCheckerForm, Malformed> {
    let layout = structure::declared(family, form)?;
    walker::well_typed_and_total(family, &layout)?;
    Ok(ValidCheckerForm(form.clone()))
}
