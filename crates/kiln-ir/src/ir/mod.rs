//! The IR vocabulary this slice implements: a [`Program`] is a [`Schema`]
//! plus named [`Function`]s (D1), each function's body a tree of
//! [`Effect`]s terminated by a [`Terminal`] so every path through a
//! function either returns or aborts — there is no "fell off the end"
//! state to represent.
//!
//! This slice covers key read, insert, update, delete, branch, abort,
//! response, checked arithmetic, clamping domains (D3), range read, bounded
//! iteration, dependent read and typed function parameters. A dependent read
//! has no type of its own: it is an [`Effect::Read`] whose `key` is an
//! [`Expr::Projection`] into a row an earlier read bound, so "which row to
//! read next" is itself a value the call computed, not a literal or
//! argument. Bounded iteration is two cooperating nodes: [`Effect::ReadRange`]
//! binds the key-ordered rows a range matched, and [`Effect::Iterate`] runs a
//! body once per row of a slot a range read bound, with the current row
//! itself bound to a slot the body's own reads, writes, and projections can
//! use (§2.1, "bounded iteration"). Constraints are schema declarations
//! only: no function body can state one (D1).

mod block;
mod effect;
mod expression;
mod key_kinds;
mod program;
mod schema;
mod validation;
mod value;

pub use block::{Block, Terminal};
pub use effect::{slots_released_by_iteration, Effect, KeyInterval};
pub use expression::{ArithmeticOp, Condition, Expr, Relation, Slot};
pub use program::{Function, Parameter, ParameterType, Program};
pub use schema::{
    CheckConstraint, ClampDomain, Column, ColumnType, Comparison, ForeignKey, Index, IntegerWidth,
    KeyColumn, KeyComponentFault, KeyDeclaration, Schema, SchemaError, Table, UniqueConstraint,
};
pub use validation::{FunctionFault, ProgramError, TextFormFault};
pub use value::{Key, Row, Value};
