use std::collections::HashSet;

use crate::identifiers::{ColumnName, IndexName, TableName};

use super::value::{Key, Row, Value};

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

impl Column {
    /// Why `value` cannot be this column's component of a key: another kind
    /// than the column's, or an integer outside its declared width (D8).
    pub fn key_component_fault(&self, value: &Value) -> Option<KeyComponentFault> {
        match (self.kind, value) {
            (ColumnType::Integer { width, .. }, Value::Integer(component)) => {
                let (min, max) = width.range();
                (!(min..=max).contains(component)).then_some(KeyComponentFault::OutsideWidth)
            }
            (ColumnType::Text, Value::Text(_)) => None,
            _ => Some(KeyComponentFault::WrongKind),
        }
    }

    /// Why a value of the column `source` cannot be this column's component
    /// of a key: another kind than this column's (D8). Its width is a run-time
    /// matter.
    pub fn key_component_fault_from(&self, source: &Column) -> Option<KeyComponentFault> {
        (!self.kind.is_same_kind_as(&source.kind)).then_some(KeyComponentFault::WrongKind)
    }
}

/// Why a key component cannot name a row (D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyComponentFault {
    WrongKind,
    OutsideWidth,
}

/// A unique constraint (D1, D8): no two rows of `table` may hold the same
/// tuple of values in `columns` in a transaction's final state (D2). A row
/// lacking any of the columns is bound by nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueConstraint {
    pub table: TableName,
    pub columns: Vec<ColumnName>,
}

/// A foreign key (D1, D8): every row of `table` that holds a value in every
/// one of `columns` must, in a transaction's final state, name the key of a
/// row of `references` by that tuple, and a row of `references` may not be
/// deleted while such a row still names its key (D2). The columns map one to
/// one, in order, onto the referenced table's whole primary key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub table: TableName,
    pub columns: Vec<ColumnName>,
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

    /// Whether both are integers, whatever their widths, or both are text.
    fn is_same_kind_as(&self, other: &ColumnType) -> bool {
        matches!(
            (self, other),
            (ColumnType::Integer { .. }, ColumnType::Integer { .. })
                | (ColumnType::Text, ColumnType::Text)
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

/// How a table identifies its rows (D8): by an ordered, non-empty list of
/// its own columns, or not at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDeclaration {
    Columns(Vec<ColumnName>),
    Keyless,
}

/// A table declaration: its name, its columns and its key. The key columns
/// are among `columns` for their kind and width, but a stored row never
/// repeats them: they live only in the row's [`Key`] (D8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: TableName,
    pub columns: Vec<Column>,
    pub key: KeyDeclaration,
}

impl Table {
    /// The declared column `name`, or `None` if the table has none.
    pub fn column(&self, name: &ColumnName) -> Option<&Column> {
        self.columns
            .iter()
            .find(|candidate| &candidate.name == name)
    }

    /// The key columns in key order; none for a keyless table.
    pub fn key_columns(&self) -> &[ColumnName] {
        match &self.key {
            KeyDeclaration::Columns(columns) => columns,
            KeyDeclaration::Keyless => &[],
        }
    }

    /// Where `column` sits in the key, or `None` if it is no key column.
    pub fn key_position(&self, column: &ColumnName) -> Option<usize> {
        self.key_columns()
            .iter()
            .position(|candidate| candidate == column)
    }

    /// The value the row stored under `key` holds in `column`: the matching
    /// key component for a key column, the row's own value otherwise.
    pub fn column_value<'a>(
        &self,
        key: &'a Key,
        row: &'a Row,
        column: &ColumnName,
    ) -> Option<&'a Value> {
        match self.key_position(column) {
            Some(position) => key.0.get(position),
            None => row.columns.get(column),
        }
    }

    /// The values the row stored under `key` holds in `columns`, in order, or
    /// `None` if it lacks any of them.
    pub fn tuple_of(&self, key: &Key, row: &Row, columns: &[ColumnName]) -> Option<Vec<Value>> {
        columns
            .iter()
            .map(|column| self.column_value(key, row, column).cloned())
            .collect()
    }
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
        self.table(table).and_then(|table| table.column(column))
    }

    /// The declared table `name`, or `None` if the schema declares none.
    pub fn table(&self, name: &TableName) -> Option<&Table> {
        self.tables.iter().find(|candidate| &candidate.name == name)
    }

    /// The declared index named `name`, or `None` if the schema declares none.
    pub fn index(&self, name: &IndexName) -> Option<&Index> {
        self.indexes
            .iter()
            .find(|candidate| &candidate.name == name)
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
        self.validate_columns()?;
        self.validate_keys()?;
        self.validate_unique_constraints()?;
        self.validate_foreign_keys()?;
        self.validate_check_constraints()?;
        self.validate_indexes()
    }

    fn validate_columns(&self) -> Result<(), SchemaError> {
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
        Ok(())
    }

    /// A key is a non-empty list of the table's own columns, none twice (D8).
    fn validate_keys(&self) -> Result<(), SchemaError> {
        for table in &self.tables {
            let KeyDeclaration::Columns(key) = &table.key else {
                continue;
            };
            if key.is_empty() {
                return Err(SchemaError::EmptyKey(table.name.clone()));
            }
            let mut seen = HashSet::new();
            for column in key {
                let located = KeyColumn {
                    table: table.name.clone(),
                    column: column.clone(),
                };
                if table.column(column).is_none() {
                    return Err(SchemaError::UnknownKeyColumn(located));
                }
                if !seen.insert(column) {
                    return Err(SchemaError::RepeatedKeyColumn(located));
                }
            }
        }
        Ok(())
    }

    /// A unique constraint names at least one column, all declared (D8).
    fn validate_unique_constraints(&self) -> Result<(), SchemaError> {
        for constraint in &self.unique_constraints {
            if constraint.columns.is_empty() {
                return Err(SchemaError::UniqueWithoutColumns(constraint.clone()));
            }
            if self
                .columns_of(&constraint.table, &constraint.columns)
                .is_none()
            {
                return Err(SchemaError::UnknownUniqueTarget(constraint.clone()));
            }
        }
        Ok(())
    }

    /// A foreign key names declared tables and columns, and its columns match
    /// the referenced table's primary key in number and, position by position,
    /// in kind (D8).
    fn validate_foreign_keys(&self) -> Result<(), SchemaError> {
        for foreign_key in &self.foreign_keys {
            let child = self.columns_of(&foreign_key.table, &foreign_key.columns);
            let parent = self.table(&foreign_key.references);
            let (Some(child), Some(parent)) = (child, parent) else {
                return Err(SchemaError::UnknownForeignKeyTarget(foreign_key.clone()));
            };
            let parent_key = parent.key_columns();
            if child.is_empty() || child.len() != parent_key.len() {
                return Err(SchemaError::ForeignKeyKeyArityMismatch(foreign_key.clone()));
            }
            let kinds_match = child.iter().zip(parent_key).all(|(child, parent_key)| {
                let parent_key = parent.column(parent_key).expect("validated key column");
                child.kind.is_same_kind_as(&parent_key.kind)
            });
            if !kinds_match {
                return Err(SchemaError::ForeignKeyTypeMismatch(foreign_key.clone()));
            }
        }
        Ok(())
    }

    /// The declared `columns` of `table`, in order, or `None` if the table or
    /// any of them is not declared.
    fn columns_of(&self, table: &TableName, columns: &[ColumnName]) -> Option<Vec<&Column>> {
        let table = self.table(table)?;
        columns.iter().map(|column| table.column(column)).collect()
    }

    fn validate_check_constraints(&self) -> Result<(), SchemaError> {
        for constraint in &self.check_constraints {
            let Some(constrained) = self.column(&constraint.table, &constraint.column) else {
                return Err(SchemaError::UnknownCheckTarget(constraint.clone()));
            };
            if !constrained.kind.holds_kind_of(&constraint.operand) {
                return Err(SchemaError::CheckOperandTypeMismatch(constraint.clone()));
            }
        }
        Ok(())
    }

    fn validate_indexes(&self) -> Result<(), SchemaError> {
        let mut names = HashSet::new();
        for index in &self.indexes {
            if index.columns.is_empty() {
                return Err(SchemaError::IndexWithoutColumns(index.clone()));
            }
            let table = self.table(&index.table);
            let all_declared = table.is_some_and(|table| {
                index
                    .columns
                    .iter()
                    .all(|column| table.column(column).is_some())
            });
            if !all_declared {
                return Err(SchemaError::UnknownIndexTarget(index.clone()));
            }
            if table.is_some_and(|table| table.key == KeyDeclaration::Keyless) {
                return Err(SchemaError::IndexOnKeylessTable(index.clone()));
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

/// A column of a table's key, as a [`SchemaError`] names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyColumn {
    pub table: TableName,
    pub column: ColumnName,
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
    /// A unique constraint names no column, so it would bind every row alike.
    UniqueWithoutColumns(UniqueConstraint),
    /// A foreign key names a table or column, or references a table, the
    /// schema does not declare.
    UnknownForeignKeyTarget(ForeignKey),
    /// A check constraint names a table or column the schema does not declare.
    UnknownCheckTarget(CheckConstraint),
    /// One of a foreign key's columns is of another kind (integer or text)
    /// than the referenced table's key column at its position, so no value of
    /// it could name a parent.
    ForeignKeyTypeMismatch(ForeignKey),
    /// A foreign key names no column, or as many as the referenced table's
    /// primary key has components in number other than that.
    ForeignKeyKeyArityMismatch(ForeignKey),
    /// A table declares an empty list of key columns; a table without a key
    /// declares itself keyless.
    EmptyKey(TableName),
    /// A table's key names a column the table does not declare.
    UnknownKeyColumn(KeyColumn),
    /// A table's key names the same column twice.
    RepeatedKeyColumn(KeyColumn),
    /// An index sits on a keyless table, which no read can reach (D9).
    IndexOnKeylessTable(Index),
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
