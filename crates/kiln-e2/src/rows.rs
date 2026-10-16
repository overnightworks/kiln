//! Row keys and integer rows. The one place that spells how this crate keys
//! a row, so a change of the IR's key type is local to this file.

use kiln_ir::{
    Call, Column, ColumnName, ColumnType, Expr, FunctionName, IntegerWidth, Key, KeyDeclaration,
    Row, Store, TableName, Value,
};

pub(crate) fn table(name: &str) -> TableName {
    TableName(name.to_owned())
}

pub(crate) fn column(name: &str) -> ColumnName {
    ColumnName(name.to_owned())
}

const KEY_COLUMN: &str = "key";

/// The one integer column every table of this crate is keyed by.
pub(crate) fn key_column() -> Column {
    Column {
        name: column(KEY_COLUMN),
        kind: ColumnType::Integer {
            width: IntegerWidth::I64,
            domain: None,
        },
    }
}

pub(crate) fn key_declaration() -> KeyDeclaration {
    KeyDeclaration::Columns(vec![column(KEY_COLUMN)])
}

fn key(index: i64) -> Key {
    Key(vec![Value::Integer(index)])
}

pub(crate) fn key_literal(index: i64) -> Vec<Expr> {
    vec![Expr::Literal(Value::Integer(index))]
}

pub(crate) fn integer_row(columns: &[(&str, i64)]) -> Row {
    Row {
        columns: columns
            .iter()
            .map(|(name, value)| (column(name), Value::Integer(*value)))
            .collect(),
    }
}

pub(crate) fn store_of(table_name: &str, rows: impl IntoIterator<Item = (i64, Row)>) -> Store {
    Store::containing(
        rows.into_iter()
            .map(|(index, row)| (table(table_name), key(index), row)),
    )
}

/// The integer in `column` of the row at `index`; the oracle's stores always
/// hold the rows its programs read, so a missing one is a broken invariant.
pub(crate) fn integer_at(store: &Store, table_name: &str, index: i64, column_name: &str) -> i64 {
    match store
        .row(&table(table_name), &key(index))
        .and_then(|row| row.columns.get(&column(column_name)))
    {
        Some(Value::Integer(value)) => *value,
        other => panic!("{table_name}[{index}].{column_name} is not an integer: {other:?}"),
    }
}

pub(crate) fn call(function: &str, arguments: &[i64]) -> Call {
    Call {
        function: FunctionName(function.to_owned()),
        args: arguments.iter().copied().map(Value::Integer).collect(),
    }
}
