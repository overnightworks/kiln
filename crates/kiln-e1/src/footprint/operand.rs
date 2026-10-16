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

/// The binding occurrence of a row: the iteration that yielded it or the read
/// that bound it, never the slot name, which may be bound again. It says which
/// row a column came from: two columns name components of one row exactly when
/// their occurrences are equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowOrigin(NodePath);

impl RowOrigin {
    pub fn new(binding: NodePath) -> Self {
        Self(binding)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Literal(Value),
    Parameter(ParameterName),
    /// `column` of the row `row` identifies, which `read` produced. Rows of
    /// one read are many; only components of one `row` come from one row. A
    /// key column of a point-read row carries the binding occurrence of the
    /// row the read's own key came from.
    Column {
        read: ReadRef,
        row: RowOrigin,
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

    /// The row occurrences the operand's columns come from, in reading order.
    pub fn origins(&self) -> Vec<&RowOrigin> {
        match self {
            Self::Literal(_) | Self::Parameter(_) => Vec::new(),
            Self::Column { row, .. } => vec![row],
            Self::Arithmetic(_, left, right) => {
                let mut origins = left.origins();
                origins.extend(right.origins());
                origins
            }
        }
    }

    /// The operand with every row occurrence renamed by `rename`, or `None`
    /// when `rename` has no name for one of them.
    pub fn renamed(&self, rename: &impl Fn(&RowOrigin) -> Option<RowOrigin>) -> Option<Self> {
        Some(match self {
            Self::Literal(_) | Self::Parameter(_) => self.clone(),
            Self::Column { read, row, column } => Self::Column {
                read: read.clone(),
                row: rename(row)?,
                column: column.clone(),
            },
            Self::Arithmetic(operator, left, right) => Self::Arithmetic(
                *operator,
                Box::new(left.renamed(rename)?),
                Box::new(right.renamed(rename)?),
            ),
        })
    }

    /// Arithmetic over a read result: its value is no read's column or key, so
    /// no declared foreign key or range identifies it.
    pub fn is_computed_from_reads(&self) -> bool {
        matches!(self, Self::Arithmetic(..)) && !self.uses().is_empty()
    }
}

/// Why an expression has no operand where the path reaches it.
#[derive(Debug)]
pub enum Unresolvable {
    /// It projects a slot that holds no current row where the path reads it:
    /// the evaluator aborts there with `ProjectionSourceAbsent` (`KILN-IR-15`).
    AbsentSource,
    Fault(ClassifyError),
}

impl From<ClassifyError> for Unresolvable {
    fn from(error: ClassifyError) -> Self {
        Self::Fault(error)
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
) -> Result<Operand, Unresolvable> {
    Ok(match expression {
        Expr::Literal(value) => Operand::Literal(value.clone()),
        Expr::Parameter(parameter) => Operand::Parameter(parameter.clone()),
        Expr::Projection { slot, .. } if facts.holds_no_row(*slot) => {
            return Err(Unresolvable::AbsentSource)
        }
        Expr::Projection { slot, column } => {
            let read = facts.read_of(*slot, at)?;
            let row = facts.row_origin_of(*slot, at)?;
            column_operand(schema, facts, read, row, column)
        }
        Expr::Arithmetic(operator, left, right) => Operand::Arithmetic(
            *operator,
            Box::new(operand_of(schema, facts, left, at)?),
            Box::new(operand_of(schema, facts, right, at)?),
        ),
    })
}

/// `column` of the row `row`, which `read` produced: for a key column of a
/// point-read row, the operand the read used for it (`inputs.md` §5,
/// Clarification rule 1).
pub fn column_operand(
    schema: &Schema,
    facts: &PathFacts,
    read: ReadRef,
    row: RowOrigin,
    column: &ColumnName,
) -> Operand {
    match point_read_key_operand(schema, facts, &read, column) {
        Some(used) => used.clone(),
        None => Operand::Column {
            read,
            row,
            column: column.clone(),
        },
    }
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
