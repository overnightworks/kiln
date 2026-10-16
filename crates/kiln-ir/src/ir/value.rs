use std::collections::BTreeMap;

use crate::identifiers::ColumnName;

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
