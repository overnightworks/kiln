//! The state a [`Program`](crate::ir::Program) reads and writes: rows kept
//! per table, ordered by key.
//!
//! A `BTreeMap` orders each table's rows by key rather than merely indexing
//! them, because `docs/concept.md` §2.2 fixes bounded iteration to proceed
//! in key order; a later slice's range read walks this same ordering
//! instead of needing a different structure.

use std::collections::BTreeMap;

use crate::identifiers::TableName;
use crate::ir::{Row, Value};

/// A snapshot of every table's rows. [`crate::evaluator::run`] never mutates
/// the `Store` it is given; it returns a new one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Store {
    tables: BTreeMap<TableName, BTreeMap<Value, Row>>,
}

impl Store {
    /// A store with no rows in any table.
    pub fn empty() -> Self {
        Self::default()
    }

    /// A store containing exactly the given `(table, key, row)` triples.
    pub fn containing(rows: impl IntoIterator<Item = (TableName, Value, Row)>) -> Self {
        let mut tables: BTreeMap<TableName, BTreeMap<Value, Row>> = BTreeMap::new();
        for (table, key, row) in rows {
            tables.entry(table).or_default().insert(key, row);
        }
        Self { tables }
    }

    /// The row at `key` in `table`, or `None` if the table has no row
    /// there — an absent table behaves exactly like an empty one.
    pub fn row(&self, table: &TableName, key: &Value) -> Option<&Row> {
        self.tables.get(table).and_then(|rows| rows.get(key))
    }

    /// Writes `row` at `key` in `table`, overwriting whatever was there.
    pub(crate) fn put_row(&mut self, table: TableName, key: Value, row: Row) {
        self.tables.entry(table).or_default().insert(key, row);
    }

    /// Removes the row at `key` in `table`, if one exists.
    pub(crate) fn remove_row(&mut self, table: &TableName, key: &Value) {
        if let Some(rows) = self.tables.get_mut(table) {
            rows.remove(key);
        }
    }
}
