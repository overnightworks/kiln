//! Key components that can never name a row, found before any call runs: a
//! literal of another kind or outside the key column's width, and an
//! expression whose static kind differs from the key column's (D8). A
//! component known only at run time stays a run-time abort.

use std::collections::HashMap;

use crate::identifiers::{ColumnName, TableName};

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

/// Where a slot's rows can come from.
enum Origin<'a> {
    /// A read of this table, or of a table that cannot be named (`None`).
    Table(Option<&'a TableName>),
    /// The rows of an iterated slot.
    RowsOf(Slot),
}

/// What validating one function's effects needs besides the schema: its
/// declared parameters and every table each slot is ever bound from.
pub(super) struct FunctionScope<'a> {
    pub(super) schema: &'a Schema,
    parameters: &'a [Parameter],
    /// A slot with a `None` among its tables has one that cannot be named, so
    /// its projections have no static kind.
    slot_tables: HashMap<Slot, Vec<Option<&'a TableName>>>,
}

impl<'a> FunctionScope<'a> {
    pub(super) fn new(schema: &'a Schema, function: &'a Function) -> Self {
        let mut origins = Vec::new();
        collect_origins_of_block(schema, &function.body, &mut origins);
        let mut scope = Self {
            schema,
            parameters: &function.parameters,
            slot_tables: HashMap::new(),
        };
        scope.settle_tables(&origins);
        scope
    }

    /// Grows each slot's tables until the rows of an iterated slot carry every
    /// table that slot can hold anywhere in the function.
    fn settle_tables(&mut self, origins: &[(Slot, Origin<'a>)]) {
        let mut grown = true;
        while grown {
            grown = false;
            for (slot, origin) in origins {
                let tables = match origin {
                    Origin::Table(table) => vec![*table],
                    Origin::RowsOf(source) => {
                        self.slot_tables.get(source).cloned().unwrap_or_default()
                    }
                };
                let known = self.slot_tables.entry(*slot).or_default();
                for table in tables {
                    if !known.contains(&table) {
                        known.push(table);
                        grown = true;
                    }
                }
            }
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
        if let Expr::Projection { slot, column: name } = expr {
            return self.projection_fault(column, *slot, name);
        }
        let kind = self.static_kind(expr)?;
        (kind != StaticKind::from(column.kind)).then_some(KeyComponentFault::WrongKind)
    }

    /// The fault shared by every table `slot` can hold, so a projection that
    /// fits in even one of them is left to the run.
    fn projection_fault(
        &self,
        column: &Column,
        slot: Slot,
        name: &ColumnName,
    ) -> Option<KeyComponentFault> {
        let mut fault = None;
        for table in self.slot_tables.get(&slot)? {
            let source = self.schema.table((*table)?)?.column(name)?;
            fault = Some(column.key_component_fault_from(source)?);
        }
        fault
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
            Expr::Projection { .. } => None,
            Expr::Arithmetic(..) => Some(StaticKind::Integer),
            Expr::Concat(..) | Expr::Truncate { .. } => Some(StaticKind::Text),
            Expr::Conditional {
                when_true,
                when_false,
                ..
            } => {
                let kind = self.static_kind(when_true)?;
                (self.static_kind(when_false)? == kind).then_some(kind)
            }
        }
    }
}

fn collect_origins_of_block<'a>(
    schema: &'a Schema,
    block: &'a Block,
    origins: &mut Vec<(Slot, Origin<'a>)>,
) {
    collect_origins_of(schema, &block.effects, origins);
    if let Terminal::Branch {
        when_present,
        when_absent,
        ..
    } = &block.terminal
    {
        collect_origins_of_block(schema, when_present, origins);
        collect_origins_of_block(schema, when_absent, origins);
    }
}

fn collect_origins_of<'a>(
    schema: &'a Schema,
    effects: &'a [Effect],
    origins: &mut Vec<(Slot, Origin<'a>)>,
) {
    for effect in effects {
        match effect {
            Effect::Read { table, into, .. } | Effect::ReadRange { table, into, .. } => {
                origins.push((*into, Origin::Table(Some(table))));
            }
            Effect::ReadIndexRange { index, into, .. } => {
                let table = schema.index(index).map(|index| &index.table);
                origins.push((*into, Origin::Table(table)));
            }
            Effect::Iterate { source, row, body } => {
                origins.push((*row, Origin::RowsOf(*source)));
                collect_origins_of(schema, body, origins);
            }
            Effect::Insert { .. } | Effect::Update { .. } | Effect::Delete { .. } => {}
        }
    }
}
