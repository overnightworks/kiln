use crate::family::Family;
use crate::form::{CheckerForm, ComponentShape, Field};
use crate::malformed::{Malformed, Path, Step};
use crate::sort::Sort;
use crate::term::Term;

/// The terms validation walks, whichever parts the form is missing: a
/// missing part is check 3's finding, so it must not stop checks 1 and 2.
pub(super) struct Layout<'a> {
    pub components: Vec<ComponentLayout<'a>>,
    pub functions: Vec<FunctionLayout<'a>>,
}

pub(super) struct ComponentLayout<'a> {
    pub fields: &'a [Field],
    pub shape: ComponentShape,
    /// An `Err` is check 2's shape defect, reported when the walk reaches
    /// this component.
    pub applied: Result<AppliedInput, Malformed>,
    pub combine: Option<&'a Term>,
    pub apply: Option<&'a Term>,
}

/// What `x` is in a component's apply (grammar §1.3, "the apply input is
/// typed").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AppliedInput {
    /// `x` is a value of this sort.
    Value(Sort),
    /// Over a whole keyed state `x` exists only as `x(v)`, one row at a
    /// time, and the result is a `keyed` literal of this category.
    Rows(String),
}

impl AppliedInput {
    pub(super) fn result_sort(&self) -> Sort {
        match self {
            Self::Value(sort) => sort.clone(),
            Self::Rows(category) => Sort::Keyed(category.clone()),
        }
    }

    fn of(kind: &Sort, shape: ComponentShape, path: Path) -> Result<Self, Malformed> {
        match (shape, kind) {
            (ComponentShape::Whole, Sort::Keyed(category)) => Ok(Self::Rows(category.clone())),
            (ComponentShape::Whole, other) => Ok(Self::Value(other.clone())),
            (ComponentShape::PerKey, Sort::Keyed(_)) => Ok(Self::Value(Sort::Opt)),
            (ComponentShape::PerKey, _) => Err(Malformed::PerKeyOnUnkeyedComponent { path }),
        }
    }
}

pub(super) struct FunctionLayout<'a> {
    pub valid: Option<&'a Term>,
    pub atoms: &'a [Term],
    pub respond: Option<&'a Term>,
}

fn at(steps: &[Step]) -> Path {
    Path(steps.to_vec())
}

/// The part of check 1 about the form itself, after its terms' names were
/// found declared. Protocol §3.1 is silent on counts; a count against the
/// signature or the `Program` is a declaration fact, so it belongs here.
pub(super) fn declared(family: &Family, form: &CheckerForm) -> Result<(), Malformed> {
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
    for (index, component) in form.components.iter().enumerate() {
        check_field_sorts(family, &component.carrier_fields, Step::Component(index))?;
    }
    for (index, function) in form.functions.iter().enumerate() {
        if function.atom.len() != signature.components.len() {
            return Err(Malformed::AtomCountMismatch {
                path: at(&[Step::Function(index)]),
                expected: signature.components.len(),
                found: function.atom.len(),
            });
        }
    }
    Ok(())
}

/// The terms of a form that passed `declared`.
pub(super) fn layout<'a>(family: &Family, form: &'a CheckerForm) -> Layout<'a> {
    let components = form
        .components
        .iter()
        .zip(&family.signature().components)
        .enumerate()
        .map(|(index, (component, state))| ComponentLayout {
            fields: &component.carrier_fields,
            shape: component.shape,
            applied: AppliedInput::of(&state.kind, component.shape, at(&[Step::Component(index)])),
            combine: component.combine.as_ref(),
            apply: component.apply.as_ref(),
        })
        .collect();
    let functions = form
        .functions
        .iter()
        .map(|function| FunctionLayout {
            valid: function.valid.as_ref(),
            atoms: &function.atom,
            respond: function.respond.as_ref(),
        })
        .collect();
    Layout {
        components,
        functions,
    }
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
