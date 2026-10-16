//! `kiln-ir`: the reference interpreter for kiln's semantic IR.
//!
//! E1's lowering audit, E2's elementwise comparison, E3's checker and
//! `docs/verification.md`'s layer 3 differential tests each need one
//! executable interpreter of the IR to compare against
//! (`docs/experiments.md`, "Shared dependency"). This crate is that
//! interpreter. It covers this slice's node kinds only — key read, range
//! read, dependent read, insert, update, delete, branch, abort, response,
//! checked arithmetic over a column's or argument's declared width, and
//! optional clamping domains — with read-your-writes inside a transaction;
//! the reference meaning it implements is recorded in `docs/concept.md`
//! §2.2, the single owner of that meaning.
//!
//! The crate has no I/O: [`run`] is a pure function from a [`Program`],
//! a [`Call`] and a [`Store`] to an [`Execution`], and [`replay`] chains
//! it over a call sequence (D4). Both return a typed [`ProgramError`] if the
//! program is not internally satisfiable: its schema, or a function body that
//! names a table, a key or a written column the schema does not allow (D3, D8).

mod call;
mod evaluator;
mod identifiers;
mod ir;
mod replay;
mod store;

pub use call::Call;
pub use evaluator::{
    run, AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns,
    ConstraintRef, Execution, Outcome, OverflowReason,
};
pub use identifiers::{AbortName, ColumnName, FunctionName, IndexName, ParameterName, TableName};
pub use ir::{
    slots_released_by_iteration, ArithmeticOp, Block, CheckConstraint, ClampDomain, Column,
    ColumnType, Comparison, Condition, Effect, Expr, ForeignKey, Function, FunctionFault, Index,
    IntegerWidth, Key, KeyColumn, KeyComponentFault, KeyDeclaration, KeyInterval, Parameter,
    ParameterType, Program, ProgramError, Relation, Row, Schema, SchemaError, Slot, Table,
    Terminal, TextFormFault, UniqueConstraint, Value,
};
pub use replay::replay;
pub use store::{KeyedRow, Store};
