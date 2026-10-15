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

use std::collections::{BTreeMap, HashSet};

use crate::identifiers::{
    AbortName, ColumnName, FunctionName, IndexName, ParameterName, TableName,
};

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

/// What a column holds: a checked integer at a declared width with an
/// optional clamping domain, or text (D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Integer {
        width: IntegerWidth,
        domain: Option<ClampDomain>,
    },
    Text,
}

/// A table's column declaration: its name and declared type (D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: ColumnName,
    pub kind: ColumnType,
}

/// A unique constraint (D1): no two rows of `table` may hold the same value
/// in `column` in a transaction's final state (D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueConstraint {
    pub table: TableName,
    pub column: ColumnName,
}

/// A foreign key (D1): every row of `table` that holds a value in `column`
/// must, in a transaction's final state, name the key of a row of
/// `references`, and a row of `references` may not be deleted while such a
/// row still names its key (D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub table: TableName,
    pub column: ColumnName,
    pub references: TableName,
}

/// A secondary index (D1): the rows of `table` ordered by the values they hold
/// in `columns`, in that order, ties broken by primary key. A row lacking any
/// of the columns is not in the index. `unique` only declares the index's
/// intent; a unique constraint is a separate declaration and an index enforces
/// nothing (D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub name: IndexName,
    pub table: TableName,
    pub columns: Vec<ColumnName>,
    pub unique: bool,
}

/// How a check constraint compares a row's column with its operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    AtLeast,
    AtMost,
}

impl ColumnType {
    fn holds_kind_of(&self, value: &Value) -> bool {
        matches!(
            (self, value),
            (ColumnType::Integer { .. }, Value::Integer(_)) | (ColumnType::Text, Value::Text(_))
        )
    }
}

impl Comparison {
    fn holds(self, value: &Value, operand: &Value) -> bool {
        match self {
            Comparison::AtLeast => value >= operand,
            Comparison::AtMost => value <= operand,
        }
    }
}

/// A check constraint (D1): in a transaction's final state, every row of
/// `table` that holds a value in `column` must satisfy `comparison` against
/// `operand`. It judges one row at a time, so it reads nothing but the row.
///
/// Distinct from a [`ClampDomain`]: a violated check aborts the call, where
/// a clamp bounds every write's own value and never aborts (D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckConstraint {
    pub table: TableName,
    pub column: ColumnName,
    pub comparison: Comparison,
    pub operand: Value,
}

impl CheckConstraint {
    /// Whether a row holding `value` in the constrained column satisfies the
    /// check.
    pub fn accepts(&self, value: &Value) -> bool {
        self.comparison.holds(value, &self.operand)
    }
}

/// A table declaration: its name and the columns rows in it may carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: TableName,
    pub columns: Vec<Column>,
}

/// The declared tables of a [`Program`] (D1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: Vec<Table>,
    pub unique_constraints: Vec<UniqueConstraint>,
    pub foreign_keys: Vec<ForeignKey>,
    pub check_constraints: Vec<CheckConstraint>,
    pub indexes: Vec<Index>,
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

    /// The declared index named `name`, or `None` if the schema declares none.
    pub fn index(&self, name: &IndexName) -> Option<&Index> {
        self.indexes
            .iter()
            .find(|candidate| &candidate.name == name)
    }

    fn has_table(&self, table: &TableName) -> bool {
        self.tables.iter().any(|candidate| &candidate.name == table)
    }

    /// Rejects a schema whose own declarations cannot jointly hold: a
    /// column's clamping domain, when it declares one, must have `min <=
    /// max` and must lie inside that same column's declared width — a
    /// domain reaching outside its column's width could clamp a write to a
    /// value the width itself forbids, silently defeating D3's width check
    /// (`docs/concept.md` §2.2, D3). `run` calls this before executing any
    /// call, so a schema this rejects never reaches the evaluator at all.
    /// A unique constraint, foreign key or check constraint must likewise
    /// name declared tables and columns.
    pub fn validate(&self) -> Result<(), SchemaError> {
        for table in &self.tables {
            for column in &table.columns {
                if let ColumnType::Integer {
                    width,
                    domain: Some(domain),
                } = column.kind
                {
                    validate_domain(&table.name, &column.name, width, domain)?;
                }
            }
        }
        for constraint in &self.unique_constraints {
            if self.column(&constraint.table, &constraint.column).is_none() {
                return Err(SchemaError::UnknownUniqueTarget(constraint.clone()));
            }
        }
        for foreign_key in &self.foreign_keys {
            let child = self.column(&foreign_key.table, &foreign_key.column);
            let Some(child) = child.filter(|_| self.has_table(&foreign_key.references)) else {
                return Err(SchemaError::UnknownForeignKeyTarget(foreign_key.clone()));
            };
            if child.kind == ColumnType::Text {
                return Err(SchemaError::ForeignKeyTypeMismatch(foreign_key.clone()));
            }
        }
        for constraint in &self.check_constraints {
            let Some(constrained) = self.column(&constraint.table, &constraint.column) else {
                return Err(SchemaError::UnknownCheckTarget(constraint.clone()));
            };
            if !constrained.kind.holds_kind_of(&constraint.operand) {
                return Err(SchemaError::CheckOperandTypeMismatch(constraint.clone()));
            }
        }
        self.validate_indexes()
    }

    fn validate_indexes(&self) -> Result<(), SchemaError> {
        let mut names = HashSet::new();
        for index in &self.indexes {
            if index.columns.is_empty() {
                return Err(SchemaError::IndexWithoutColumns(index.clone()));
            }
            let all_declared = index
                .columns
                .iter()
                .all(|column| self.column(&index.table, column).is_some());
            if !all_declared {
                return Err(SchemaError::UnknownIndexTarget(index.clone()));
            }
            if !names.insert(&index.name) {
                return Err(SchemaError::DuplicateIndexName(index.name.clone()));
            }
        }
        Ok(())
    }
}

fn validate_domain(
    table: &TableName,
    column: &ColumnName,
    width: IntegerWidth,
    domain: ClampDomain,
) -> Result<(), SchemaError> {
    if domain.min > domain.max {
        return Err(SchemaError::ReversedDomain {
            table: table.clone(),
            column: column.clone(),
            min: domain.min,
            max: domain.max,
        });
    }
    let (width_min, width_max) = width.range();
    if domain.min < width_min || domain.max > width_max {
        return Err(SchemaError::DomainExceedsWidth {
            table: table.clone(),
            column: column.clone(),
        });
    }
    Ok(())
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
    /// A unique constraint names a table or column the schema does not declare.
    UnknownUniqueTarget(UniqueConstraint),
    /// A foreign key names a table or column, or references a table, the
    /// schema does not declare.
    UnknownForeignKeyTarget(ForeignKey),
    /// A check constraint names a table or column the schema does not declare.
    UnknownCheckTarget(CheckConstraint),
    /// A foreign key sits on a column that is not an integer, but tables are
    /// keyed by integers, so no value of that column could name a parent.
    ForeignKeyTypeMismatch(ForeignKey),
    /// A check constraint compares its column with an operand of the other
    /// type (integer or text), so the comparison would not depend on the data.
    CheckOperandTypeMismatch(CheckConstraint),
    /// An index names a table or a column the schema does not declare.
    UnknownIndexTarget(Index),
    /// An index declares no column, so it would order nothing.
    IndexWithoutColumns(Index),
    /// Two indexes share a name, so a read could not say which it means.
    DuplicateIndexName(IndexName),
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
/// [`Expr::Projection`] projects a column out of a row an earlier
/// [`Effect::Read`] or [`Effect::Iterate`] already bound to a [`Slot`] in
/// this same call. Used as another [`Effect::Read`]'s `key`, it makes that
/// read a dependent read: which row to read next is itself a value the
/// call computed, not a literal or argument (`docs/concept.md` §2.1, §2.2
/// D6). A projection out of a [`Slot`] an [`Effect::ReadRange`] bound has no
/// single current row to read — whether because that range matched no row
/// or because the slot holds the whole matched sequence rather than one row
/// — and a projection out of an absent point-read row has none either;
/// `run` reports both the same way, as the typed
/// [`AbortReason::ProjectionSourceAbsent`](crate::evaluator::AbortReason::ProjectionSourceAbsent),
/// since which row a compiled program ends up projecting from is a fact
/// about the data it ran against, not something `run` can rule out ahead of
/// time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Value),
    Parameter(ParameterName),
    Projection { slot: Slot, column: ColumnName },
    Arithmetic(ArithmeticOp, Box<Expr>, Box<Expr>),
}

/// A condition a [`Terminal::Branch`] decides on.
///
/// `RowPresent` tests one fact for either read kind that can bind a
/// [`Slot`]: a point read (`docs/concept.md` §2.2 D6, "an absent key is a
/// branchable value") is present when it found a row, and a range read
/// (§2.2 D6, "an empty range reads as an empty sequence") is present when
/// its matched sequence is non-empty — the same branchable-absence fact
/// under one name, since both ultimately ask "did this read find anything".
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
    /// Read every row whose key falls in `[start, end]`, in key order,
    /// binding the matched sequence to `into` — empty if `start` sorts
    /// after `end` or no row's key falls between them (`docs/concept.md`
    /// §2.2 D6, "an empty range reads as an empty sequence").
    ///
    /// Always bounded (§2.1, "never an open-ended loop"): if more rows
    /// match than `bound` resolves to, the call aborts with
    /// [`AbortReason::BoundExceeded`](crate::evaluator::AbortReason::BoundExceeded)
    /// instead of binding `into` at all.
    ReadRange {
        table: TableName,
        start: Expr,
        end: Expr,
        bound: Expr,
        into: Slot,
    },
    /// Read every row of `index`'s table whose indexed values, taken in the
    /// index's column order, lie in `[start, end]` (one bound per indexed
    /// column, compared lexicographically), in index order with ties broken
    /// by primary key, binding the matched sequence to `into` exactly as
    /// [`Effect::ReadRange`] does: empty if `start` sorts after `end` or
    /// nothing matches, and aborting past `bound` (`docs/concept.md` §2.2).
    ///
    /// The log records one entry for the index read itself; rows reached
    /// from the result are read by ordinary [`Effect::Read`]s.
    ReadIndexRange {
        index: IndexName,
        start: Vec<Expr>,
        end: Vec<Expr>,
        bound: Expr,
        into: Slot,
    },
    /// Run `body` once for each row `source` bound, in the order the read bound
    /// them, binding
    /// the current row to `row` so the body's own reads, writes, and
    /// projections can use it (§2.1, "bounded iteration"; `docs/concept.md`
    /// §2.2 D6, "bounded iteration proceeds in key order").
    ///
    /// `source` must be a [`Slot`] an earlier [`Effect::ReadRange`] in this
    /// same body bound: that read already resolved the bound and checked it
    /// against the matched row count (`docs/concept.md` §2.1, "never an
    /// open-ended loop"), so `Iterate` re-reads nothing and checks no bound
    /// of its own. An abort raised by any effect in `body` aborts the whole
    /// call, discarding every effect applied so far, in this row's part of
    /// the iteration or an earlier one (`docs/concept.md` §2.2).
    Iterate {
        source: Slot,
        row: Slot,
        body: Vec<Effect>,
    },
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

/// A function parameter's declared type: an integer at a mandatory width,
/// or text. Every parameter declares one of the two — there is no
/// undeclared or optionally-typed parameter, so an argument's kind is
/// always known ahead of checking its value (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterType {
    Integer(IntegerWidth),
    Text,
}

/// A function's declared parameter: its name and declared type.
///
/// An argument bound to `name` is checked against `kind` exactly as a
/// write is checked against its target column's declared width (D3,
/// `docs/concept.md` §2.2: "Arithmetic runs over checked integers at the
/// column's or argument's declared width"): an argument whose own kind does
/// not match `kind` aborts, and an [`IntegerWidth`]-declared argument
/// outside that width's representable range aborts too. Unlike `Column`, a
/// declared parameter carries no clamping domain: D3 clamps a column's own
/// write, never a call's argument, so an out-of-width argument always
/// aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: ParameterName,
    pub kind: ParameterType,
}

/// A named transaction function: its parameters and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: FunctionName,
    pub parameters: Vec<Parameter>,
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
