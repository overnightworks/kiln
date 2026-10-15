//! `kiln-ir`: the reference interpreter for kiln's semantic IR.
//!
//! E1's lowering audit, E2's elementwise comparison, E3's checker and
//! `docs/verification.md`'s layer 3 differential tests each need one
//! executable interpreter of the IR to compare against
//! (`docs/experiments.md`, "Shared dependency"). This crate is that
//! interpreter. It covers this slice's node kinds only — key read, insert,
//! update, delete, branch, abort, response, and checked arithmetic over a
//! column's declared width and optional clamping domain — with
//! read-your-writes inside a transaction; the reference meaning it
//! implements is recorded in `docs/concept.md` §2.2, the single owner of
//! that meaning.
//!
//! The crate has no I/O: [`run`] is a pure function from a [`Program`], a
//! [`Call`] and a [`Store`] to an [`Execution`], or a typed [`SchemaError`]
//! if the program's own schema is not internally satisfiable (D3).

mod call;
mod evaluator;
mod identifiers;
mod ir;
mod store;

pub use call::Call;
pub use evaluator::{
    run, AbortReason, AccessKind, AccessLogEntry, AccessedColumns, Execution, Outcome,
    OverflowReason,
};
pub use identifiers::{AbortName, ColumnName, FunctionName, ParameterName, TableName};
pub use ir::{
    ArithmeticOp, Block, ClampDomain, Column, Condition, Effect, Expr, Function, IntegerWidth,
    Program, Row, Schema, SchemaError, Slot, Table, Terminal, Value,
};
pub use store::Store;
