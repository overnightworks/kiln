//! Key components that can never name a row, found before any call runs: a
//! literal of another kind or outside the key column's width, and an
//! expression whose static kind differs from the key column's (D8); and the
//! text forms no call can run: an operand that cannot be text, a length that
//! cannot be an integer, a negative literal length (D9). What is known only at
//! run time stays a run-time abort.

use std::collections::HashMap;

use crate::identifiers::{ColumnName, TableName};

use super::block::{Block, Terminal};
use super::effect::Effect;
use super::expression::{Condition, Expr, Slot};
use super::program::{Function, Parameter, ParameterType};
use super::schema::{Column, ColumnType, KeyComponentFault, Schema, Table};
use super::validation::{FunctionFault, TextFormFault};
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

    /// The first fault, in the order the evaluator resolves them, of a text
    /// form in `expr` that no call can get past. Like a key component's kind
    /// (`KILN-IR-82`), it is a typing check that ignores which arm of a
    /// conditional a call chooses.
    pub(super) fn check_text_forms(&self, expr: &Expr) -> Result<(), FunctionFault> {
        self.text_form_fault(expr)
            .map_or(Ok(()), |fault| Err(FunctionFault::TextForm(fault)))
    }

    pub(super) fn check_condition_text_forms(
        &self,
        condition: &Condition,
    ) -> Result<(), FunctionFault> {
        self.condition_fault(condition)
            .map_or(Ok(()), |fault| Err(FunctionFault::TextForm(fault)))
    }

    fn text_form_fault(&self, expr: &Expr) -> Option<TextFormFault> {
        match expr {
            Expr::Literal(_) | Expr::Parameter(_) | Expr::Projection { .. } => None,
            Expr::Arithmetic(_, left, right) => self
                .text_form_fault(left)
                .or_else(|| self.text_form_fault(right)),
            Expr::Concat(first, second) => self
                .operand_fault(first, StaticKind::Text)
                .or_else(|| self.operand_fault(second, StaticKind::Text)),
            Expr::Truncate { text, length } => self
                .operand_fault(text, StaticKind::Text)
                .or_else(|| self.operand_fault(length, StaticKind::Integer))
                .or_else(|| {
                    matches!(**length, Expr::Literal(Value::Integer(n)) if n < 0)
                        .then_some(TextFormFault::NegativeLength)
                }),
            Expr::Conditional {
                condition,
                when_true,
                when_false,
            } => self
                .condition_fault(condition)
                .or_else(|| self.text_form_fault(when_true))
                .or_else(|| self.text_form_fault(when_false)),
        }
    }

    fn condition_fault(&self, condition: &Condition) -> Option<TextFormFault> {
        match condition {
            Condition::RowPresent(_) => None,
            Condition::Compare { left, right, .. } => self
                .text_form_fault(left)
                .or_else(|| self.text_form_fault(right)),
            Condition::Contains { text, part } => self
                .operand_fault(text, StaticKind::Text)
                .or_else(|| self.operand_fault(part, StaticKind::Text)),
        }
    }

    /// Why `operand` cannot be a value of the `needed` kind: a fault of its
    /// own comes first, as it aborts before the kind is judged.
    fn operand_fault(&self, operand: &Expr, needed: StaticKind) -> Option<TextFormFault> {
        self.text_form_fault(operand).or_else(|| {
            let kind = self.static_kind(operand)?;
            (kind != needed).then_some(match needed {
                StaticKind::Text => TextFormFault::OperandNotText,
                StaticKind::Integer => TextFormFault::LengthNotInteger,
            })
        })
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
            Expr::Projection { slot, column } => self.projection_kind(*slot, column),
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

    /// The kind of column `name` in every table `slot` can hold, when they
    /// all agree.
    fn projection_kind(&self, slot: Slot, name: &ColumnName) -> Option<StaticKind> {
        let kind_in = |table: &Option<&TableName>| {
            let column = self.schema.table((*table)?)?.column(name)?;
            Some(StaticKind::from(column.kind))
        };
        let mut tables = self.slot_tables.get(&slot)?.iter();
        let kind = kind_in(tables.next()?)?;
        tables
            .all(|table| kind_in(table) == Some(kind))
            .then_some(kind)
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
