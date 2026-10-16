//! The state a [`Program`](crate::ir::Program) reads and writes: rows kept
//! per table, ordered by key.
//!
//! A `BTreeMap` orders each table's rows by key rather than merely indexing
//! them, because `docs/concept.md` §2.2 fixes bounded iteration to proceed
//! in key order, and keys order component by component (D8); a later
//! slice's range read walks this same ordering instead of needing a
//! different structure.

use std::collections::BTreeMap;

use crate::identifiers::{ColumnName, TableName};
use crate::ir::{Key, Row, Table, Value};

/// A row together with the key it is stored under, so a read's result can
/// answer a key-column projection without the row restating its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyedRow {
    pub key: Key,
    pub row: Row,
}

/// A snapshot of every table's rows. [`crate::evaluator::run`] never mutates
/// the `Store` it is given; it returns a new one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Store {
    tables: BTreeMap<TableName, BTreeMap<Key, Row>>,
}

impl Store {
    /// A store with no rows in any table.
    pub fn empty() -> Self {
        Self::default()
    }

    /// A store containing exactly the given `(table, key, row)` triples.
    pub fn containing(rows: impl IntoIterator<Item = (TableName, Key, Row)>) -> Self {
        let mut tables: BTreeMap<TableName, BTreeMap<Key, Row>> = BTreeMap::new();
        for (table, key, row) in rows {
            tables.entry(table).or_default().insert(key, row);
        }
        Self { tables }
    }

    /// The row at `key` in `table`, or `None` if the table has no row
    /// there — an absent table behaves exactly like an empty one.
    pub fn row(&self, table: &TableName, key: &Key) -> Option<&Row> {
        self.tables.get(table).and_then(|rows| rows.get(key))
    }

    /// Every row of `table` whose key holds `prefix` as its first components
    /// and, when `interval` is given, whose next component lies in it
    /// (inclusive), with its key, in key order. Without an interval every
    /// value of the next component matches.
    ///
    /// Empty if `table` has no rows, if no key matches, or if the interval is
    /// reversed — a reversed bound is an empty range, not a panic
    /// (`docs/concept.md` §2.2 D6, D8). `prefix` is shorter than the table's
    /// keys, which `Program::validate` guarantees.
    pub fn rows_in_range(
        &self,
        table: &TableName,
        prefix: &[Value],
        interval: Option<(&Value, &Value)>,
    ) -> Vec<KeyedRow> {
        let Some(rows) = self.tables.get(table) else {
            return Vec::new();
        };
        let mut first = prefix.to_vec();
        first.extend(interval.map(|(from, _)| from.clone()));
        rows.range(Key(first)..)
            .take_while(|(key, _)| {
                key.0.starts_with(prefix)
                    && interval.is_none_or(|(_, to)| &key.0[prefix.len()] <= to)
            })
            .map(|(key, row)| KeyedRow {
                key: key.clone(),
                row: row.clone(),
            })
            .collect()
    }

    /// Every row of `table` holding all of `columns` whose values, in that
    /// order, lie in `[start, end]`, ordered by those values and then by
    /// key — empty if `start` sorts after `end`. A key column holds its
    /// key's component. Each result carries its key and the indexed columns
    /// that are no key column: the index holds nothing else.
    pub fn rows_in_index_range(
        &self,
        table: &Table,
        columns: &[ColumnName],
        start: &[Value],
        end: &[Value],
    ) -> Vec<KeyedRow> {
        let mut entries: Vec<(Vec<&Value>, &Key)> = self
            .tables
            .get(&table.name)
            .into_iter()
            .flatten()
            .filter_map(|(key, row)| {
                let held: Option<Vec<&Value>> = columns
                    .iter()
                    .map(|column| table.column_value(key, row, column))
                    .collect();
                held.map(|held| (held, key))
            })
            .filter(|(held, _)| {
                held.iter().copied().ge(start.iter()) && held.iter().copied().le(end.iter())
            })
            .collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
            .into_iter()
            .map(|(held, key)| KeyedRow {
                key: key.clone(),
                row: Row {
                    columns: columns
                        .iter()
                        .zip(held)
                        .filter(|(column, _)| table.key_position(column).is_none())
                        .map(|(column, value)| (column.clone(), value.clone()))
                        .collect(),
                },
            })
            .collect()
    }

    /// Whether a row of `table` other than the one at `except_key` holds
    /// `tuple` in `columns`.
    pub(crate) fn tuple_held_by_another_row(
        &self,
        table: &Table,
        columns: &[ColumnName],
        tuple: &[Value],
        except_key: &Key,
    ) -> bool {
        self.keys_holding(table, columns, tuple)
            .any(|key| key != except_key)
    }

    /// Whether any row of `table` holds `tuple` in `columns`.
    pub(crate) fn any_row_holds(
        &self,
        table: &Table,
        columns: &[ColumnName],
        tuple: &[Value],
    ) -> bool {
        self.keys_holding(table, columns, tuple).next().is_some()
    }

    fn keys_holding<'a>(
        &'a self,
        table: &'a Table,
        columns: &'a [ColumnName],
        tuple: &'a [Value],
    ) -> impl Iterator<Item = &'a Key> {
        self.tables
            .get(&table.name)
            .into_iter()
            .flatten()
            .filter(move |(key, row)| {
                columns
                    .iter()
                    .zip(tuple)
                    .all(|(column, value)| table.column_value(key, row, column) == Some(value))
            })
            .map(|(key, _)| key)
    }

    /// Writes `row` at `key` in `table`, overwriting whatever was there.
    pub(crate) fn put_row(&mut self, table: TableName, key: Key, row: Row) {
        self.tables.entry(table).or_default().insert(key, row);
    }

    /// Removes the row at `key` in `table`, if one exists.
    ///
    /// Drops the table's own entry once its last row is gone, so a store
    /// left with no rows anywhere compares equal to [`Store::empty`] — an
    /// empty table map left behind would otherwise make two stores with
    /// identical readable contents compare unequal.
    pub(crate) fn remove_row(&mut self, table: &TableName, key: &Key) {
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

    fn one_part(component: i64) -> Key {
        Key(vec![Value::Integer(component)])
    }

    mod kiln_ir_19 {
        use super::*;

        #[test]
        fn insert_then_delete_from_empty_store_equals_the_empty_store() {
            let table = TableName("t".to_owned());
            let key = one_part(1);
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

    fn rows_between(store: &Store, table: &TableName, from: i64, to: i64) -> Vec<KeyedRow> {
        let (from, to) = (Value::Integer(from), Value::Integer(to));
        store.rows_in_range(table, &[], Some((&from, &to)))
    }

    fn observed_keys(rows: &[KeyedRow]) -> Vec<i64> {
        rows.iter()
            .map(
                |keyed| match keyed.row.columns.get(&ColumnName("k".to_owned())) {
                    Some(Value::Integer(key)) => *key,
                    other => panic!("expected row_keyed's own integer column, got {other:?}"),
                },
            )
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
                    keys.iter().map(|&key| (table.clone(), one_part(key), row_keyed(key))),
                );

                let rows = rows_between(&store, &table, i64::MIN, i64::MAX);

                prop_assert_eq!(observed_keys(&rows), sorted_keys);
            }
        }

        #[test]
        fn rows_in_range_includes_both_inclusive_bounds_and_excludes_rows_outside_them() {
            let table = TableName("t".to_owned());
            let store = Store::containing(
                [0, 5, 10, 15]
                    .into_iter()
                    .map(|key| (table.clone(), one_part(key), row_keyed(key))),
            );

            let rows = rows_between(&store, &table, 5, 10);

            assert_eq!(observed_keys(&rows), vec![5, 10]);
        }

        #[test]
        fn rows_in_range_includes_the_single_row_when_start_equals_end() {
            let table = TableName("t".to_owned());
            let store = Store::containing([
                (table.clone(), one_part(4), row_keyed(4)),
                (table.clone(), one_part(5), row_keyed(5)),
                (table.clone(), one_part(6), row_keyed(6)),
            ]);

            let rows = rows_between(&store, &table, 5, 5);

            assert_eq!(observed_keys(&rows), vec![5]);
        }
    }

    mod kiln_ir_10 {
        use super::*;

        #[test]
        fn rows_in_range_is_empty_when_start_sorts_after_end() {
            let table = TableName("t".to_owned());
            let store = Store::containing([(table.clone(), one_part(5), Row::default())]);

            let rows = rows_between(&store, &table, 10, 0);

            assert!(rows.is_empty());
        }

        #[test]
        fn rows_in_range_is_empty_for_a_table_with_no_rows() {
            let table = TableName("t".to_owned());
            let store = Store::empty();

            let rows = rows_between(&store, &table, 0, 10);

            assert!(rows.is_empty());
        }
    }
}
