//! Named things a [`Program`](crate::ir::Program) can declare or refer to.
//!
//! Each identifier is its own type instead of a bare `String` so a table
//! name can never be passed where a column name is expected, even though
//! both are backed by text.

/// The name of a table declared in a [`Schema`](crate::ir::Schema).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName(pub String);

/// The name of a column within a table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnName(pub String);

/// The name of a transaction function declared on a [`Program`](crate::ir::Program).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionName(pub String);

/// The name of one of a function's declared parameters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterName(pub String);

/// The name a transaction gives an explicit, declared abort.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbortName(pub String);
