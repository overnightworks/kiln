//! A key or bound as the classifier reasons about it: the IR expression with
//! every slot replaced by the read that bound it, so two accesses compare equal
//! exactly when they name the same data. It is read from the expression's
//! syntax, every arm of every conditional included, reached or not.

use kiln_ir::{
    ArithmeticOp, ColumnName, Condition, Expr, ParameterName, Relation, Schema, Slot, Value,
};

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
    /// A condition of a conditional value: whether the read found a row.
    Presence(ReadRef),
    /// A condition of a conditional value that tests a slot an ended iteration
    /// released: it finds no row, whatever the data.
    Absence,
    /// A condition of a conditional value: whether `left` stands in the
    /// relation to `right`.
    Comparison(Relation, Box<Operand>, Box<Operand>),
    /// The first operand is a condition; the value is the second when it
    /// holds and the third otherwise.
    Conditional(Box<Operand>, Box<Operand>, Box<Operand>),
    /// `column` of a slot an ended iteration released: the slot holds no row,
    /// so it names no read result. Resolving it aborts, so it stands only in
    /// an arm the path never runs.
    ReleasedColumn {
        slot: Slot,
        column: ColumnName,
    },
}

impl Operand {
    /// The reads whose results the operand reads.
    pub fn uses(&self) -> Vec<ReadRef> {
        match self {
            Self::Literal(_) | Self::Parameter(_) | Self::Absence | Self::ReleasedColumn { .. } => {
                Vec::new()
            }
            Self::Column { read, .. } | Self::Presence(read) => vec![read.clone()],
            Self::Arithmetic(_, first, second) | Self::Comparison(_, first, second) => {
                let mut uses = first.uses();
                uses.extend(second.uses());
                uses
            }
            Self::Conditional(condition, when_true, when_false) => {
                let mut uses = condition.uses();
                uses.extend(when_true.uses());
                uses.extend(when_false.uses());
                uses
            }
        }
    }

    /// The row occurrences the operand's columns come from, in reading order.
    pub fn origins(&self) -> Vec<&RowOrigin> {
        match self {
            Self::Literal(_)
            | Self::Parameter(_)
            | Self::Presence(_)
            | Self::Absence
            | Self::ReleasedColumn { .. } => Vec::new(),
            Self::Column { row, .. } => vec![row],
            Self::Arithmetic(_, first, second) | Self::Comparison(_, first, second) => {
                let mut origins = first.origins();
                origins.extend(second.origins());
                origins
            }
            Self::Conditional(condition, when_true, when_false) => {
                let mut origins = condition.origins();
                origins.extend(when_true.origins());
                origins.extend(when_false.origins());
                origins
            }
        }
    }

    /// The operand with every row occurrence renamed by `rename`, or `None`
    /// when `rename` has no name for one of them.
    pub fn renamed(&self, rename: &impl Fn(&RowOrigin) -> Option<RowOrigin>) -> Option<Self> {
        Some(match self {
            Self::Literal(_)
            | Self::Parameter(_)
            | Self::Presence(_)
            | Self::Absence
            | Self::ReleasedColumn { .. } => self.clone(),
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
            Self::Comparison(relation, left, right) => Self::Comparison(
                *relation,
                Box::new(left.renamed(rename)?),
                Box::new(right.renamed(rename)?),
            ),
            Self::Conditional(condition, when_true, when_false) => Self::Conditional(
                Box::new(condition.renamed(rename)?),
                Box::new(when_true.renamed(rename)?),
                Box::new(when_false.renamed(rename)?),
            ),
        })
    }

    /// Arithmetic or a conditional over a read result: its value is no read's
    /// column or key, so no declared foreign key or range identifies it.
    pub fn is_computed_from_reads(&self) -> bool {
        matches!(self, Self::Arithmetic(..) | Self::Conditional(..)) && !self.uses().is_empty()
    }
}

/// `expression` with its slots resolved against what the path has read so
/// far. Every arm of a conditional counts, whichever the path runs: what a
/// value is made of never depends on the arm chosen (`inputs.md` §5,
/// Clarification 7(c)). A key column of a point-read row is the operand the
/// read used for it, and one a primary-key range's static prefix covers is
/// that prefix operand (Clarification rule 1).
pub fn operand_of(
    schema: &Schema,
    facts: &PathFacts,
    expression: &Expr,
    at: &NodePath,
) -> Result<Operand, ClassifyError> {
    Ok(match expression {
        Expr::Literal(value) => Operand::Literal(value.clone()),
        Expr::Parameter(parameter) => Operand::Parameter(parameter.clone()),
        Expr::Projection { slot, column } if facts.is_released(*slot) => Operand::ReleasedColumn {
            slot: *slot,
            column: column.clone(),
        },
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
        Expr::Conditional {
            condition,
            when_true,
            when_false,
        } => Operand::Conditional(
            Box::new(condition_operand(schema, facts, condition, at)?),
            Box::new(operand_of(schema, facts, when_true, at)?),
            Box::new(operand_of(schema, facts, when_false, at)?),
        ),
    })
}

fn condition_operand(
    schema: &Schema,
    facts: &PathFacts,
    condition: &Condition,
    at: &NodePath,
) -> Result<Operand, ClassifyError> {
    Ok(match condition {
        Condition::RowPresent(slot) if facts.is_released(*slot) => Operand::Absence,
        Condition::RowPresent(slot) => Operand::Presence(facts.read_of(*slot, at)?),
        Condition::Compare {
            left,
            relation,
            right,
        } => Operand::Comparison(
            *relation,
            Box::new(operand_of(schema, facts, left, at)?),
            Box::new(operand_of(schema, facts, right, at)?),
        ),
    })
}

/// `column` of the row `row`, which `read` produced: for a key column of a
/// point-read row, the operand the read used for it, and for one a
/// primary-key range's static prefix covers, that prefix operand (`inputs.md`
/// §5, Clarification rule 1).
pub fn column_operand(
    schema: &Schema,
    facts: &PathFacts,
    read: ReadRef,
    row: RowOrigin,
    column: &ColumnName,
) -> Operand {
    match aliased_key_operand(schema, facts, &read, column) {
        Some(used) => used.clone(),
        None => Operand::Column {
            read,
            row,
            column: column.clone(),
        },
    }
}

/// The operand `read` fixed for its key column `column`: what a point read
/// used for it, or what a primary-key range's prefix holds there when that
/// reads nothing — a prefix derived from a read is no operand the path can
/// compare with a key.
fn aliased_key_operand<'f>(
    schema: &Schema,
    facts: &'f PathFacts,
    read: &ReadRef,
    column: &ColumnName,
) -> Option<&'f Operand> {
    let (table, fixed, must_be_static) = match &facts.binding_of(read)?.kind {
        BindingKind::PointRead { table, key } => (table, key, false),
        BindingKind::PrimaryRange { table, prefix } => (table, prefix, true),
        BindingKind::IndexRange { .. } | BindingKind::UndeclaredIndexRange(_) => return None,
    };
    let position = schema.table(table)?.key_position(column)?;
    fixed
        .get(position)
        .filter(|operand| !must_be_static || operand.uses().is_empty())
}
