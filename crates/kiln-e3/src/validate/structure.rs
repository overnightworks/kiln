use crate::family::Family;
use crate::form::{CheckerForm, Field};
use crate::malformed::{Malformed, Path, Step};
use crate::sort::Sort;
use crate::term::Term;

/// A checker form whose parts are all present: the terms validation walks.
pub(super) struct Layout<'a> {
    pub components: Vec<ComponentLayout<'a>>,
    pub functions: Vec<FunctionLayout<'a>>,
}

pub(super) struct ComponentLayout<'a> {
    pub fields: &'a [Field],
    pub kind: &'a Sort,
    pub combine: &'a Term,
    pub apply: &'a Term,
}

pub(super) struct FunctionLayout<'a> {
    pub parameters: &'a [kiln_ir::Parameter],
    pub valid: &'a Term,
    pub atoms: &'a [Term],
    pub respond: &'a Term,
}

fn at(steps: &[Step]) -> Path {
    Path(steps.to_vec())
}

/// Check 1 of protocol §3.1 for the parts of the form itself.
pub(super) fn declared<'a>(
    family: &'a Family,
    form: &'a CheckerForm,
) -> Result<Layout<'a>, Malformed> {
    let signature = family.signature();
    if form.components.len() != signature.components.len() {
        return Err(Malformed::ComponentCountMismatch {
            path: Path::default(),
            expected: signature.components.len(),
            found: form.components.len(),
        });
    }
    let program_functions = &family.program().functions;
    if form.functions.len() != program_functions.len() {
        return Err(Malformed::FunctionCountMismatch {
            path: Path::default(),
            expected: program_functions.len(),
            found: form.functions.len(),
        });
    }
    Ok(Layout {
        components: component_layouts(family, form)?,
        functions: function_layouts(family, form)?,
    })
}

fn component_layouts<'a>(
    family: &'a Family,
    form: &'a CheckerForm,
) -> Result<Vec<ComponentLayout<'a>>, Malformed> {
    form.components
        .iter()
        .zip(&family.signature().components)
        .enumerate()
        .map(|(index, (component, state))| {
            let here = Step::Component(index);
            check_field_sorts(family, &component.carrier_fields, here)?;
            let combine = component
                .combine
                .as_ref()
                .ok_or_else(|| Malformed::MissingCombine {
                    path: at(&[here, Step::Combine]),
                })?;
            let apply = component
                .apply
                .as_ref()
                .ok_or_else(|| Malformed::MissingApply {
                    path: at(&[here, Step::Apply]),
                })?;
            Ok(ComponentLayout {
                fields: &component.carrier_fields,
                kind: &state.kind,
                combine,
                apply,
            })
        })
        .collect()
}

fn function_layouts<'a>(
    family: &'a Family,
    form: &'a CheckerForm,
) -> Result<Vec<FunctionLayout<'a>>, Malformed> {
    let component_count = family.signature().components.len();
    form.functions
        .iter()
        .zip(&family.program().functions)
        .enumerate()
        .map(|(index, (function, declared))| {
            let here = Step::Function(index);
            if function.atom.len() != component_count {
                return Err(Malformed::AtomCountMismatch {
                    path: at(&[here]),
                    expected: component_count,
                    found: function.atom.len(),
                });
            }
            let valid = function
                .valid
                .as_ref()
                .ok_or_else(|| Malformed::MissingValid {
                    path: at(&[here, Step::Valid]),
                })?;
            let respond = function
                .respond
                .as_ref()
                .ok_or_else(|| Malformed::MissingRespond {
                    path: at(&[here, Step::Respond]),
                })?;
            Ok(FunctionLayout {
                parameters: &declared.parameters,
                valid,
                atoms: &function.atom,
                respond,
            })
        })
        .collect()
}

fn check_field_sorts(
    family: &Family,
    fields: &[Field],
    at_component: Step,
) -> Result<(), Malformed> {
    for field in fields {
        match &field.sort {
            Sort::Int | Sort::Bool | Sort::Opt => {}
            Sort::Cat(category) | Sort::Map(category) => {
                if family.signature().category(category).is_none() {
                    return Err(Malformed::UnknownCategory {
                        path: at(&[at_component]),
                        name: category.clone(),
                    });
                }
            }
            unsupported => {
                return Err(Malformed::UnsupportedFieldSort {
                    path: at(&[at_component]),
                    sort: unsupported.clone(),
                });
            }
        }
    }
    Ok(())
}
