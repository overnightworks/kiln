//! The faults of a [`Program`] that no call's data can cause, found before any
//! call runs (D8, D9): an effect that names a table the schema does not key
//! the way the effect needs.

use std::collections::BTreeMap;

use crate::identifiers::{ColumnName, FunctionName, TableName};

use super::block::{Block, Terminal};
use super::effect::Effect;
use super::expression::Expr;
use super::schema::{Schema, SchemaError, Table};

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
    /// A key expression or range bound with `given` components for a table
    /// keyed by `expected`.
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
}

pub(super) fn check_block(schema: &Schema, block: &Block) -> Result<(), FunctionFault> {
    for effect in &block.effects {
        check_effect(schema, effect)?;
    }
    match &block.terminal {
        Terminal::Return(_) | Terminal::Abort(_) => Ok(()),
        Terminal::Branch {
            when_present,
            when_absent,
            ..
        } => {
            check_block(schema, when_present)?;
            check_block(schema, when_absent)
        }
    }
}

fn check_effect(schema: &Schema, effect: &Effect) -> Result<(), FunctionFault> {
    match effect {
        Effect::Read { table, key, .. } | Effect::Delete { table, key } => {
            check_key(schema, table, key).map(drop)
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
            let declared = check_key(schema, table, key)?;
            check_written_columns(declared, columns)
        }
        Effect::ReadRange {
            table, start, end, ..
        } => {
            check_key(schema, table, start)?;
            check_key(schema, table, end).map(drop)
        }
        Effect::ReadIndexRange { .. } => Ok(()),
        Effect::Iterate { body, .. } => body
            .iter()
            .try_for_each(|effect| check_effect(schema, effect)),
    }
}

fn keyed_table<'a>(schema: &'a Schema, name: &TableName) -> Result<&'a Table, FunctionFault> {
    let table = schema
        .table(name)
        .ok_or_else(|| FunctionFault::UndeclaredTable(name.clone()))?;
    if table.key_columns().is_empty() {
        return Err(FunctionFault::KeylessTable(name.clone()));
    }
    Ok(table)
}

/// The declared, keyed `table` when `key` has one expression per key column.
fn check_key<'a>(
    schema: &'a Schema,
    table: &TableName,
    key: &[Expr],
) -> Result<&'a Table, FunctionFault> {
    let declared = keyed_table(schema, table)?;
    let expected = declared.key_columns().len();
    if key.len() != expected {
        return Err(FunctionFault::KeyArity {
            table: table.clone(),
            expected,
            given: key.len(),
        });
    }
    Ok(declared)
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
