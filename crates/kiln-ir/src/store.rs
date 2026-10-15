//! The state a [`Program`](crate::ir::Program) reads and writes: rows kept
//! per table, ordered by key.
//!
//! A `BTreeMap` orders each table's rows by key rather than merely indexing
//! them, because `docs/concept.md` §2.2 fixes bounded iteration to proceed
//! in key order; a later slice's range read walks this same ordering
//! instead of needing a different structure.

use std::collections::BTreeMap;

use crate::identifiers::{ColumnName, TableName};
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

    /// Every row of `table` whose key falls in `[start, end]`, in key
    /// order.
    ///
    /// Empty if `table` has no rows, if no key falls between `start` and
    /// `end`, or if `start` sorts after `end` — a reversed bound is an
    /// empty range, not a panic: `BTreeMap::range` would panic on a
    /// reversed bound, so this checks for it first instead of ever calling
    /// into that panic (`docs/concept.md` §2.2 D6).
    pub fn rows_in_range(&self, table: &TableName, start: &Value, end: &Value) -> Vec<Row> {
        if start > end {
            return Vec::new();
        }
        self.tables
            .get(table)
            .map(|rows| {
                rows.range(start.clone()..=end.clone())
                    .map(|(_, row)| row.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether a row of `table` other than the one at `except_key` holds
    /// `value` in `column`.
    pub(crate) fn value_held_by_another_row(
        &self,
        table: &TableName,
        column: &ColumnName,
        value: &Value,
        except_key: &Value,
    ) -> bool {
        self.tables.get(table).is_some_and(|rows| {
            rows.iter()
                .any(|(key, row)| key != except_key && row.columns.get(column) == Some(value))
        })
    }

    /// Writes `row` at `key` in `table`, overwriting whatever was there.
    pub(crate) fn put_row(&mut self, table: TableName, key: Value, row: Row) {
        self.tables.entry(table).or_default().insert(key, row);
    }

    /// Removes the row at `key` in `table`, if one exists.
    ///
    /// Drops the table's own entry once its last row is gone, so a store
    /// left with no rows anywhere compares equal to [`Store::empty`] — an
    /// empty table map left behind would otherwise make two stores with
    /// identical readable contents compare unequal.
    pub(crate) fn remove_row(&mut self, table: &TableName, key: &Value) {
        if let Some(rows) = self.tables.get_mut(table) {
            rows.remove(key);
            if rows.is_empty() {
                self.tables.remove(table);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::ColumnName;
    use proptest::prelude::*;

    mod kiln_ir_19 {
        use super::*;

        #[test]
        fn insert_then_delete_from_empty_store_equals_the_empty_store() {
            let table = TableName("t".to_owned());
            let key = Value::Integer(1);
            let mut store = Store::empty();

            store.put_row(table.clone(), key.clone(), Row::default());
            store.remove_row(&table, &key);

            assert_eq!(store, Store::empty());
        }
    }

    /// Builds a row whose own column `"k"` restates the key it was inserted
    /// under, so a test can read a `rows_in_range` result's row order back
    /// out as plain integers without the key itself ever leaving the store.
    fn row_keyed(key: i64) -> Row {
        Row {
            columns: BTreeMap::from([(ColumnName("k".to_owned()), Value::Integer(key))]),
        }
    }

    fn observed_keys(rows: &[Row]) -> Vec<i64> {
        rows.iter()
            .map(|row| match row.columns.get(&ColumnName("k".to_owned())) {
                Some(Value::Integer(key)) => *key,
                other => panic!("expected row_keyed's own integer column, got {other:?}"),
            })
            .collect()
    }

    mod kiln_ir_09 {
        use super::*;

        proptest! {
            #[test]
            fn rows_in_range_returns_every_matched_row_in_key_order(
                keys in prop::collection::hash_set(any::<i64>(), 1..20),
            ) {
                let table = TableName("t".to_owned());
                let mut sorted_keys: Vec<i64> = keys.iter().copied().collect();
                sorted_keys.sort_unstable();
                let store = Store::containing(
                    keys.iter().map(|&key| (table.clone(), Value::Integer(key), row_keyed(key))),
                );

                let rows = store.rows_in_range(&table, &Value::Integer(i64::MIN), &Value::Integer(i64::MAX));

                prop_assert_eq!(observed_keys(&rows), sorted_keys);
            }
        }

        #[test]
        fn rows_in_range_includes_both_inclusive_bounds_and_excludes_rows_outside_them() {
            let table = TableName("t".to_owned());
            let store = Store::containing(
                [0, 5, 10, 15]
                    .into_iter()
                    .map(|key| (table.clone(), Value::Integer(key), row_keyed(key))),
            );

            let rows = store.rows_in_range(&table, &Value::Integer(5), &Value::Integer(10));

            assert_eq!(observed_keys(&rows), vec![5, 10]);
        }

        #[test]
        fn rows_in_range_includes_the_single_row_when_start_equals_end() {
            let table = TableName("t".to_owned());
            let store = Store::containing([
                (table.clone(), Value::Integer(4), row_keyed(4)),
                (table.clone(), Value::Integer(5), row_keyed(5)),
                (table.clone(), Value::Integer(6), row_keyed(6)),
            ]);

            let rows = store.rows_in_range(&table, &Value::Integer(5), &Value::Integer(5));

            assert_eq!(observed_keys(&rows), vec![5]);
        }
    }

    mod kiln_ir_10 {
        use super::*;

        #[test]
        fn rows_in_range_is_empty_when_start_sorts_after_end() {
            let table = TableName("t".to_owned());
            let store = Store::containing([(table.clone(), Value::Integer(5), Row::default())]);

            let rows = store.rows_in_range(&table, &Value::Integer(10), &Value::Integer(0));

            assert!(rows.is_empty());
        }

        #[test]
        fn rows_in_range_is_empty_for_a_table_with_no_rows() {
            let table = TableName("t".to_owned());
            let store = Store::empty();

            let rows = store.rows_in_range(&table, &Value::Integer(0), &Value::Integer(10));

            assert!(rows.is_empty());
        }
    }
}
