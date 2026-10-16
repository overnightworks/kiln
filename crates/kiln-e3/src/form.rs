use crate::sort::Sort;
use crate::term::Term;

/// A named, typed field of a component's carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub sort: Sort,
}

/// One state component's algebra. A missing part stays representable so
/// validation, not construction, rejects it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDef {
    pub carrier_fields: Vec<Field>,
    pub combine: Option<Term>,
    pub apply: Option<Term>,
}

/// One function's three terms. The atom has one carrier term per component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub valid: Option<Term>,
    pub atom: Vec<Term>,
    pub respond: Option<Term>,
}

/// What the checker evaluates (protocol §3): components in signature order
/// and functions in the `Program`'s order. A mutant is written by hand, and a
/// candidate lowers into one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckerForm {
    pub components: Vec<ComponentDef>,
    pub functions: Vec<FunctionDef>,
}
