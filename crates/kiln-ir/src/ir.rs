//! The IR vocabulary this slice implements: a [`Program`] is a [`Schema`]
//! plus named [`Function`]s (D1), each function's body a tree of
//! [`Effect`]s terminated by a [`Terminal`] so every path through a
//! function either returns or aborts — there is no "fell off the end"
//! state to represent.
//!
//! This slice covers key read, insert, update, delete, branch, abort,
//! response, checked arithmetic and clamping domains (D3). Range read,
//! dependent read, bounded iteration and constraint nodes (`docs/concept.md`
//! §2.1) belong to later slices and have no types here yet.

use std::collections::BTreeMap;

use crate::identifiers::{AbortName, ColumnName, FunctionName, ParameterName, TableName};

/// A literal value a row's column, a call's argument, or a response can hold.
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

/// The declared width of a column's checked-integer domain (D1). Arithmetic
/// and literal writes targeting the column are checked against this width's
/// representable range (D3): a declared value outside it overflows, and
/// overflow aborts unless the column also declares a [`ClampDomain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerWidth {
    I8,
    I16,
    I32,
    I64,
}

impl IntegerWidth {
    /// The inclusive range this width can represent.
    pub fn range(self) -> (i64, i64) {
        match self {
            IntegerWidth::I8 => (i8::MIN as i64, i8::MAX as i64),
            IntegerWidth::I16 => (i16::MIN as i64, i16::MAX as i64),
            IntegerWidth::I32 => (i32::MIN as i64, i32::MAX as i64),
            IntegerWidth::I64 => (i64::MIN, i64::MAX),
        }
    }
}

/// A column's optional clamping domain (D1, D3): a computed or literal write
/// targeting the column saturates to `[min, max]` instead of overflowing.
///
/// Distinct from a check constraint (a later slice): a check judges a
/// transaction's end state and aborts on violation, where a clamp bounds
/// every write's own value as it is made and never aborts
/// (`docs/concept.md` §2.2, D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClampDomain {
    pub min: i64,
    pub max: i64,
}

/// A table's column declaration: its name, declared integer width, and
/// optional clamping domain (D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: ColumnName,
    pub width: IntegerWidth,
    pub domain: Option<ClampDomain>,
}

/// A table declaration: its name and the columns rows in it may carry.
///
/// Uniqueness, foreign keys and check constraints (D1's full schema shape)
/// are later slices' work; this slice only needs a table's name and its
/// columns' integer widths and clamping domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: TableName,
    pub columns: Vec<Column>,
}

/// The declared tables of a [`Program`] (D1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: Vec<Table>,
}

impl Schema {
    /// The declared column `column` of `table`, or `None` if either the
    /// table or the column is not declared.
    ///
    /// An undeclared column carries no declared width or domain, so a write
    /// to it resolves exactly as it always has outside this slice's reach —
    /// schema declaration is additive, not a precondition every table or
    /// column must meet yet.
    pub fn column(&self, table: &TableName, column: &ColumnName) -> Option<&Column> {
        self.tables
            .iter()
            .find(|candidate| &candidate.name == table)
            .and_then(|table| {
                table
                    .columns
                    .iter()
                    .find(|candidate| &candidate.name == column)
            })
    }

    /// Rejects a schema whose own declarations cannot jointly hold: a
    /// column's clamping domain, when it declares one, must have `min <=
    /// max` and must lie inside that same column's declared width — a
    /// domain reaching outside its column's width could clamp a write to a
    /// value the width itself forbids, silently defeating D3's width check
    /// (`docs/concept.md` §2.2, D3). `run` calls this before executing any
    /// call, so a schema this rejects never reaches the evaluator at all.
    pub fn validate(&self) -> Result<(), SchemaError> {
        for table in &self.tables {
            for column in &table.columns {
                let Some(domain) = column.domain else {
                    continue;
                };
                if domain.min > domain.max {
                    return Err(SchemaError::ReversedDomain {
                        table: table.name.clone(),
                        column: column.name.clone(),
                        min: domain.min,
                        max: domain.max,
                    });
                }
                let (width_min, width_max) = column.width.range();
                if domain.min < width_min || domain.max > width_max {
                    return Err(SchemaError::DomainExceedsWidth {
                        table: table.name.clone(),
                        column: column.name.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Why a [`Schema`] fails [`Schema::validate`]: one column's own
/// declarations cannot jointly hold (D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// `column`'s clamping domain has `min` above `max`.
    ReversedDomain {
        table: TableName,
        column: ColumnName,
        min: i64,
        max: i64,
    },
    /// `column`'s clamping domain reaches outside its own declared width.
    DomainExceedsWidth {
        table: TableName,
        column: ColumnName,
    },
}

/// A position in a function's body that a later [`Condition`] or [`Expr`]
/// can refer back to, bound by an [`Effect::Read`] in that same body.
///
/// The compiler that emits a [`Function`] is responsible for giving each
/// read a distinct slot and for never referring to one before the read that
/// binds it has run; `run` trusts this invariant rather than re-checking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot(pub u32);

/// A numeric operator over two checked-integer operands (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOp {
    Add,
    Subtract,
}

/// A value computable without touching the store: one of the call's own
/// arguments, a constant written into the program, a column projected from
/// an earlier read's row, or an arithmetic operation over two such values.
///
/// [`Expr::Projection`] is a limited dependent read: it projects a column
/// out of a row an earlier [`Effect::Read`] already bound to a [`Slot`] in
/// this same call, but it cannot itself choose *which* row to read next
/// from a projected value — that full dependent read is a later slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Value),
    Parameter(ParameterName),
    Projection { slot: Slot, column: ColumnName },
    Arithmetic(ArithmeticOp, Box<Expr>, Box<Expr>),
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
