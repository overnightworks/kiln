//! The IR vocabulary this slice implements: a [`Program`] is a [`Schema`]
//! plus named [`Function`]s (D1), each function's body a tree of
//! [`Effect`]s terminated by a [`Terminal`] so every path through a
//! function either returns or aborts — there is no "fell off the end"
//! state to represent.
//!
//! This slice covers key read, insert, update, delete, branch, abort and
//! response. Range read, dependent read, bounded iteration, constraint and
//! arithmetic nodes (`docs/concept.md` §2.1) belong to later slices and have
//! no types here yet.

use std::collections::BTreeMap;

use crate::identifiers::{AbortName, ColumnName, FunctionName, ParameterName, TableName};

/// A literal value a row's column, a call's argument, or a response can hold.
///
/// Arithmetic and declared numeric domains (D3) are a later slice; this
/// slice only ever compares, stores and returns values, never computes with
/// them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    Integer(i64),
    Text(String),
}

/// A row's columns, keyed by name. The row's primary key is not one of its
/// columns: it is the key a [`Store`](crate::store::Store) looks the row up
/// by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub columns: BTreeMap<ColumnName, Value>,
}

/// A table declaration: its name and the columns rows in it may carry.
///
/// Column width, numeric domains, uniqueness, foreign keys and check
/// constraints (D1's full schema shape) are later slices' work; this slice
/// only needs a table's name and column vocabulary to exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: TableName,
    pub columns: Vec<ColumnName>,
}

/// The declared tables of a [`Program`] (D1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: Vec<Table>,
}

/// A position in a function's body that a later [`Condition`] or [`Expr`]
/// can refer back to, bound by an [`Effect::Read`] in that same body.
///
/// The compiler that emits a [`Function`] is responsible for giving each
/// read a distinct slot and for never referring to one before the read that
/// binds it has run; `run` trusts this invariant rather than re-checking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot(pub u32);

/// A value computable without touching the store: one of the call's own
/// arguments, or a constant written into the program.
///
/// Projecting a column out of an earlier read's row (a limited dependent
/// read) is not here: D1's dependent read is a later slice, and no node in
/// this slice's set needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Value),
    Parameter(ParameterName),
}

/// A condition a [`Terminal::Branch`] decides on.
///
/// This slice's only branchable fact is whether a prior read found a row —
/// "an absent key is a branchable value" (`docs/concept.md` §2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    RowPresent(Slot),
}

/// One state-touching step of a function's body, executed in program order
/// before the body's [`Terminal`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Read one row by key, binding its presence or absence to `into` for a
    /// later [`Condition::RowPresent`] to test.
    Read {
        table: TableName,
        key: Expr,
        into: Slot,
    },
    /// Insert a new row. Aborts if a row already exists at `key`.
    Insert {
        table: TableName,
        key: Expr,
        columns: BTreeMap<ColumnName, Expr>,
    },
    /// Overwrite the named columns of an existing row, leaving its other
    /// columns unchanged. Aborts if no row exists at `key`.
    Update {
        table: TableName,
        key: Expr,
        columns: BTreeMap<ColumnName, Expr>,
    },
    /// Remove a row. Aborts if no row exists at `key`.
    Delete { table: TableName, key: Expr },
}

/// How a block of [`Effect`]s ends: every path through a function's body
/// reaches exactly one `Terminal`, so a function always either returns or
/// aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminal {
    /// Return a response to the caller, committing every effect applied so
    /// far.
    Return(Expr),
    /// Reject the call by name. Every effect this call applied, in either
    /// branch, is discarded (`docs/concept.md` §2.2).
    Abort(AbortName),
    /// Take `when_present` if `condition` holds, `when_absent` otherwise.
    Branch {
        condition: Condition,
        when_present: Box<Block>,
        when_absent: Box<Block>,
    },
}

/// A sequence of effects followed by the terminal that ends them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub effects: Vec<Effect>,
    pub terminal: Terminal,
}

/// A named transaction function: its parameters and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: FunctionName,
    pub parameters: Vec<ParameterName>,
    pub body: Block,
}

/// A schema plus its named transaction functions (D1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub schema: Schema,
    pub functions: Vec<Function>,
}

impl Program {
    /// The function named `name`.
    ///
    /// Panics if the program declares no such function: a [`Call`](crate::call::Call)
    /// naming an undeclared function is a contract violation by whatever
    /// built the call, not a semantic outcome `run` reports as an [`AbortReason`](crate::evaluator::AbortReason).
    pub fn function(&self, name: &FunctionName) -> &Function {
        self.functions
            .iter()
            .find(|function| &function.name == name)
            .unwrap_or_else(|| panic!("program declares no function named {name:?}"))
    }
}
