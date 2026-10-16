use std::collections::BTreeMap;

use crate::identifiers::ColumnName;

/// A literal value a row's column, a call's argument, or a response can hold.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    Integer(i64),
    Text(String),
}

/// A table's primary key: one value per key column, in the order the table
/// declares them (D8). Keys are equal when every component is and order
/// lexicographically, component by component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub Vec<Value>);

/// A row's columns, keyed by name. The row's key columns are not among them:
/// they live only in the [`Key`] a [`Store`](crate::store::Store) looks the row
/// up by (D8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub columns: BTreeMap<ColumnName, Value>,
}
