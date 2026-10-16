//! The faults of a [`Program`] that no call's data can cause, found before any
//! call runs (D8, D9): an effect that names a table the schema does not key
//! the way the effect needs.

use std::collections::BTreeMap;

use crate::identifiers::{ColumnName, FunctionName, TableName};

use super::block::{Block, Terminal};
use super::effect::{Effect, KeyInterval};
use super::expression::Expr;
use super::key_kinds::FunctionScope;
use super::schema::{KeyComponentFault, SchemaError, Table};

/// Why a [`Program`] is rejected before any call runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramError {
    Schema(SchemaError),
    Function {
        function: FunctionName,
        fault: FunctionFault,
    },
}

impl From<SchemaError> for ProgramError {
    fn from(error: SchemaError) -> Self {
        Self::Schema(error)
    }
}

/// What is wrong with an effect of a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionFault {
    UndeclaredTable(TableName),
    /// A keyless table is reached only by appends and constraint checks (D9).
    KeylessTable(TableName),
    /// A key expression with `given` components for a table keyed by
    /// `expected`, or a range prefix with as many components as the key or
    /// more.
    KeyArity {
        table: TableName,
        expected: usize,
        given: usize,
    },
    /// An insert or update names a key column among its written columns, but
    /// a key is given only by its key expression and never changes (D8).
    WritesKeyColumn {
        table: TableName,
        column: ColumnName,
    },
    /// A key component that no call can make fit its key column: a literal
    /// of another kind or outside the column's width, or an expression whose
    /// static kind differs from the column's (D8).
    KeyComponent {
        table: TableName,
        column: ColumnName,
        fault: KeyComponentFault,
    },
}

pub(super) fn check_block(scope: &FunctionScope, block: &Block) -> Result<(), FunctionFault> {
    for effect in &block.effects {
        check_effect(scope, effect)?;
    }
    match &block.terminal {
        Terminal::Return(_) | Terminal::Abort(_) => Ok(()),
        Terminal::Branch {
            when_present,
            when_absent,
            ..
        } => {
            check_block(scope, when_present)?;
            check_block(scope, when_absent)
        }
    }
}

fn check_effect(scope: &FunctionScope, effect: &Effect) -> Result<(), FunctionFault> {
    match effect {
        Effect::Read { table, key, .. } | Effect::Delete { table, key } => {
            check_key(scope, table, key).map(drop)
        }
        Effect::Insert {
            table,
            key,
            columns,
        }
        | Effect::Update {
            table,
            key,
            columns,
        } => {
            let declared = check_key(scope, table, key)?;
            check_written_columns(declared, columns)
        }
        Effect::ReadRange {
            table,
            prefix,
            interval,
            ..
        } => check_range(scope, table, prefix, interval.as_ref()),
        Effect::ReadIndexRange { .. } => Ok(()),
        Effect::Iterate { body, .. } => body
            .iter()
            .try_for_each(|effect| check_effect(scope, effect)),
    }
}

fn keyed_table<'a>(
    schema: &'a super::schema::Schema,
    name: &TableName,
) -> Result<&'a Table, FunctionFault> {
    let table = schema
        .table(name)
        .ok_or_else(|| FunctionFault::UndeclaredTable(name.clone()))?;
    if table.key_columns().is_empty() {
        return Err(FunctionFault::KeylessTable(name.clone()));
    }
    Ok(table)
}

/// The declared, keyed `table` when `key` has one expression per key column,
/// each of a kind and value its key column accepts.
fn check_key<'a>(
    scope: &FunctionScope<'a>,
    table: &TableName,
    key: &[Expr],
) -> Result<&'a Table, FunctionFault> {
    let declared = keyed_table(scope.schema, table)?;
    let expected = declared.key_columns().len();
    if key.len() != expected {
        return Err(FunctionFault::KeyArity {
            table: table.clone(),
            expected,
            given: key.len(),
        });
    }
    scope.check_key_kinds(declared, key)?;
    Ok(declared)
}

/// A range's prefix names fewer components than the key (D8); its interval
/// bounds the component after the prefix.
fn check_range(
    scope: &FunctionScope,
    table: &TableName,
    prefix: &[Expr],
    interval: Option<&KeyInterval>,
) -> Result<(), FunctionFault> {
    let declared = keyed_table(scope.schema, table)?;
    let expected = declared.key_columns().len();
    if prefix.len() >= expected {
        return Err(FunctionFault::KeyArity {
            table: table.clone(),
            expected,
            given: prefix.len(),
        });
    }
    scope.check_key_kinds(declared, prefix)?;
    interval.map_or(Ok(()), |interval| {
        [&interval.from, &interval.to]
            .into_iter()
            .try_for_each(|bound| scope.check_component_kind(declared, prefix.len(), bound))
    })
}

fn check_written_columns(
    table: &Table,
    columns: &BTreeMap<ColumnName, Expr>,
) -> Result<(), FunctionFault> {
    match columns
        .keys()
        .find(|column| table.key_position(column).is_some())
    {
        Some(column) => Err(FunctionFault::WritesKeyColumn {
            table: table.name.clone(),
            column: column.clone(),
        }),
        None => Ok(()),
    }
}
