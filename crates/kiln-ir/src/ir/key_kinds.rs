//! Key components that can never name a row, found before any call runs: a
//! literal of another kind or outside the key column's width, and an
//! expression whose static kind differs from the key column's (D8). A
//! component known only at run time stays a run-time abort.

use std::collections::HashMap;

use crate::identifiers::TableName;

use super::block::{Block, Terminal};
use super::effect::Effect;
use super::expression::{Expr, Slot};
use super::program::{Function, Parameter, ParameterType};
use super::schema::{Column, ColumnType, KeyComponentFault, Schema, Table};
use super::validation::FunctionFault;
use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticKind {
    Integer,
    Text,
}

impl From<ColumnType> for StaticKind {
    fn from(kind: ColumnType) -> Self {
        match kind {
            ColumnType::Integer { .. } => Self::Integer,
            ColumnType::Text => Self::Text,
        }
    }
}

impl From<ParameterType> for StaticKind {
    fn from(kind: ParameterType) -> Self {
        match kind {
            ParameterType::Integer(_) => Self::Integer,
            ParameterType::Text => Self::Text,
        }
    }
}

/// What validating one function's effects needs besides the schema: its
/// declared parameters and the one table each slot is bound from.
pub(super) struct FunctionScope<'a> {
    pub(super) schema: &'a Schema,
    parameters: &'a [Parameter],
    /// `None` for a slot bound from more than one table or from a table that
    /// cannot be named, whose projections have no static kind.
    slot_tables: HashMap<Slot, Option<&'a TableName>>,
}

impl<'a> FunctionScope<'a> {
    pub(super) fn new(schema: &'a Schema, function: &'a Function) -> Self {
        let mut scope = Self {
            schema,
            parameters: &function.parameters,
            slot_tables: HashMap::new(),
        };
        scope.bind_slots_of_block(&function.body);
        scope
    }

    fn bind_slots_of_block(&mut self, block: &'a Block) {
        self.bind_slots_of(&block.effects);
        if let Terminal::Branch {
            when_present,
            when_absent,
            ..
        } = &block.terminal
        {
            self.bind_slots_of_block(when_present);
            self.bind_slots_of_block(when_absent);
        }
    }

    fn bind_slots_of(&mut self, effects: &'a [Effect]) {
        for effect in effects {
            match effect {
                Effect::Read { table, into, .. } | Effect::ReadRange { table, into, .. } => {
                    self.bind(*into, Some(table));
                }
                Effect::ReadIndexRange { index, into, .. } => {
                    let table = self.schema.index(index).map(|index| &index.table);
                    self.bind(*into, table);
                }
                Effect::Iterate { source, row, body } => {
                    let table = self.slot_tables.get(source).copied().flatten();
                    self.bind(*row, table);
                    self.bind_slots_of(body);
                }
                Effect::Insert { .. } | Effect::Update { .. } | Effect::Delete { .. } => {}
            }
        }
    }

    fn bind(&mut self, slot: Slot, table: Option<&'a TableName>) {
        let bound = self.slot_tables.entry(slot).or_insert(table);
        if *bound != table {
            *bound = None;
        }
    }

    /// The first component of `key` that can never be a component of
    /// `table`'s key.
    pub(super) fn check_key_kinds(&self, table: &Table, key: &[Expr]) -> Result<(), FunctionFault> {
        for (name, expr) in table.key_columns().iter().zip(key) {
            let column = table
                .column(name)
                .expect("a validated schema's key names declared columns");
            if let Some(fault) = self.component_fault(column, expr) {
                return Err(FunctionFault::KeyComponent {
                    table: table.name.clone(),
                    column: name.clone(),
                    fault,
                });
            }
        }
        Ok(())
    }

    fn component_fault(&self, column: &Column, expr: &Expr) -> Option<KeyComponentFault> {
        if let Expr::Literal(value) = expr {
            return column.key_component_fault(value);
        }
        let kind = self.static_kind(expr)?;
        (kind != StaticKind::from(column.kind)).then_some(KeyComponentFault::WrongKind)
    }

    /// The kind `expr` always resolves to, or `None` when the program does
    /// not declare enough to know it.
    fn static_kind(&self, expr: &Expr) -> Option<StaticKind> {
        match expr {
            Expr::Literal(Value::Integer(_)) => Some(StaticKind::Integer),
            Expr::Literal(Value::Text(_)) => Some(StaticKind::Text),
            Expr::Parameter(name) => self
                .parameters
                .iter()
                .find(|parameter| &parameter.name == name)
                .map(|parameter| parameter.kind.into()),
            Expr::Projection { slot, column } => {
                let table = self.schema.table((*self.slot_tables.get(slot)?)?)?;
                table.column(column).map(|column| column.kind.into())
            }
            Expr::Arithmetic(..) => Some(StaticKind::Integer),
        }
    }
}
