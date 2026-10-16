//! A key or bound as the classifier reasons about it: the IR expression with
//! every slot replaced by the read that bound it, so two accesses compare equal
//! exactly when they name the same data.

use kiln_ir::{ArithmeticOp, ColumnName, Expr, ParameterName, Schema, Slot, Value};

use super::facts::{BindingKind, PathFacts};
use super::node_path::NodePath;
use super::ClassifyError;

/// The read that bound a slot: the slot's origin and the read's IR node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadRef {
    pub slot: Slot,
    pub node: NodePath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Literal(Value),
    Parameter(ParameterName),
    /// `column` of the row `row` stands for, which `read` produced. Rows of
    /// one read are many; only components of one `row` come from one row.
    Column {
        read: ReadRef,
        row: Slot,
        column: ColumnName,
    },
    Arithmetic(ArithmeticOp, Box<Operand>, Box<Operand>),
}

impl Operand {
    /// The reads whose results the operand reads.
    pub fn uses(&self) -> Vec<ReadRef> {
        match self {
            Self::Literal(_) | Self::Parameter(_) => Vec::new(),
            Self::Column { read, .. } => vec![read.clone()],
            Self::Arithmetic(_, left, right) => {
                let mut uses = left.uses();
                uses.extend(right.uses());
                uses
            }
        }
    }

    /// Arithmetic over a read result: its value is no read's column or key, so
    /// no declared foreign key or range identifies it.
    pub fn is_computed_from_reads(&self) -> bool {
        matches!(self, Self::Arithmetic(..)) && !self.uses().is_empty()
    }
}

/// `expression` with its slots resolved against what the path has read so far.
/// A key column of a point-read row is the operand the read used for it
/// (`inputs.md` §5, Clarification rule 1).
pub fn operand_of(
    schema: &Schema,
    facts: &PathFacts,
    expression: &Expr,
    at: &NodePath,
) -> Result<Operand, ClassifyError> {
    Ok(match expression {
        Expr::Literal(value) => Operand::Literal(value.clone()),
        Expr::Parameter(parameter) => Operand::Parameter(parameter.clone()),
        Expr::Projection { slot, column } => {
            let read = facts.read_of(*slot, at)?;
            match point_read_key_operand(schema, facts, &read, column) {
                Some(used) => used.clone(),
                None => Operand::Column {
                    read,
                    row: *slot,
                    column: column.clone(),
                },
            }
        }
        Expr::Arithmetic(operator, left, right) => Operand::Arithmetic(
            *operator,
            Box::new(operand_of(schema, facts, left, at)?),
            Box::new(operand_of(schema, facts, right, at)?),
        ),
    })
}

/// What a point read used for its key column `column`, if `read` is one.
fn point_read_key_operand<'f>(
    schema: &Schema,
    facts: &'f PathFacts,
    read: &ReadRef,
    column: &ColumnName,
) -> Option<&'f Operand> {
    let Some(BindingKind::PointRead { table, key }) =
        facts.binding_of(read).map(|binding| &binding.kind)
    else {
        return None;
    };
    let position = schema.table(table)?.key_position(column)?;
    key.get(position)
}
