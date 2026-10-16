use kiln_ir::AbortReason;

use crate::sort::{Expected, Sort};

/// One step from the checker form's root to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Component(usize),
    Function(usize),
    Combine,
    Apply,
    Valid,
    Atom(usize),
    Respond,
    /// The operand or part of a term, counted from 0 in the term's own order.
    Child(usize),
}

/// The route to the offending node; empty for the form itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Path(pub Vec<Step>);

/// What a term may only be written inside of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Key,
    State,
    /// `when`, which only a keyed component's atom may use.
    Entry,
    Applied,
    LeftOperand,
    RightOperand,
    SummaryOperand,
}

/// Why a checker form fails protocol §3.1's checks 1 to 3, one variant per
/// failed check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    ComponentCountMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    FunctionCountMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    UnsupportedFieldSort {
        path: Path,
        sort: Sort,
    },
    PerKeyOnUnkeyedComponent {
        path: Path,
    },
    MissingCombine {
        path: Path,
    },
    MissingApply {
        path: Path,
    },
    MissingValid {
        path: Path,
    },
    MissingRespond {
        path: Path,
    },
    AtomCountMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    UnknownParameter {
        path: Path,
        name: String,
    },
    UnknownStateComponent {
        path: Path,
        name: String,
    },
    UnknownField {
        path: Path,
        name: String,
    },
    UnknownCategory {
        path: Path,
        name: String,
    },
    CategoryValueOutOfRange {
        path: Path,
        category: String,
        code: usize,
    },
    UnknownName {
        path: Path,
        name: Box<AbortReason>,
    },
    AbortOutsideNameSet {
        path: Path,
        name: Box<AbortReason>,
    },
    NotBound {
        path: Path,
        term: Binding,
    },
    TypeMismatch {
        path: Path,
        expected: Expected,
        found: Sort,
    },
    /// The grammar has no production of this sort for the term, for example
    /// an `ite` over names or `state(c)` of a keyed component.
    NoProduction {
        path: Path,
        sort: Sort,
    },
    TupleArityMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    ArmCountMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    TableEntryCountMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    KeyedRowCountMismatch {
        path: Path,
        expected: usize,
        found: usize,
    },
    ModulusNotPositive {
        path: Path,
        modulus: i64,
    },
}
