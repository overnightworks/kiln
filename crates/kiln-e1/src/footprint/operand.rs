//! A key or bound as the classifier reasons about it: the IR expression with
//! every slot replaced by the read that bound it, so two accesses compare equal
//! exactly when they name the same data.

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
    /// A condition of a conditional value: whether `left` stands in the
    /// relation to `right`.
    Comparison(Relation, Box<Operand>, Box<Operand>),
    /// The first operand is a condition; the value is the second when it
    /// holds and the third otherwise.
    Conditional(Box<Operand>, Box<Operand>, Box<Operand>),
    /// An arm that aborts the call when the condition chooses it, so no
    /// access follows it.
    Aborting,
}

impl Operand {
    /// The reads whose results the operand reads.
    pub fn uses(&self) -> Vec<ReadRef> {
        match self {
            Self::Literal(_) | Self::Parameter(_) | Self::Aborting => Vec::new(),
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
            Self::Literal(_) | Self::Parameter(_) | Self::Presence(_) | Self::Aborting => {
                Vec::new()
            }
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
            Self::Literal(_) | Self::Parameter(_) | Self::Presence(_) | Self::Aborting => {
                self.clone()
            }
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
    Ok(match decided_expression(facts, expression, at) {
        Expr::Literal(value) => Operand::Literal(value.clone()),
        Expr::Parameter(parameter) => Operand::Parameter(parameter.clone()),
        Expr::Projection { slot, .. } if facts.holds_no_row(*slot) => {
            return Err(Unresolvable::AbsentSource)
        }
        Expr::Projection { slot, column } => {
            let read = facts.read_of(*slot, at)?;
            match point_read_key_operand(schema, facts, &read, column) {
                Some(used) => used.clone(),
                None => Operand::Column {
                    row: facts.row_origin_of(*slot, at)?,
                    read,
                    column: column.clone(),
                },
            }
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
            Box::new(arm_operand(schema, facts, when_true, at)?),
            Box::new(arm_operand(schema, facts, when_false, at)?),
        ),
    })
}

fn condition_operand(
    schema: &Schema,
    facts: &PathFacts,
    condition: &Condition,
    at: &NodePath,
) -> Result<Operand, Unresolvable> {
    Ok(match condition {
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

/// An arm the condition does not decide on this path is reached only when the
/// data chooses it, so a projection that finds no row there aborts only then.
fn arm_operand(
    schema: &Schema,
    facts: &PathFacts,
    arm: &Expr,
    at: &NodePath,
) -> Result<Operand, Unresolvable> {
    match operand_of(schema, facts, arm, at) {
        Err(Unresolvable::AbsentSource) => Ok(Operand::Aborting),
        resolved => resolved,
    }
}

/// `expression` with every conditional the path decides statically replaced by
/// the arm it chooses: a condition that compares two literals, or tests a slot
/// whose presence the path knows (`KILN-E1-19`) or an ended iteration released.
pub fn decided_expression<'e>(facts: &PathFacts, expression: &'e Expr, at: &NodePath) -> &'e Expr {
    let Expr::Conditional {
        condition,
        when_true,
        when_false,
    } = expression
    else {
        return expression;
    };
    let holds = match &**condition {
        Condition::Compare {
            left: Expr::Literal(left),
            relation,
            right: Expr::Literal(right),
        } => relation.holds(left, right),
        Condition::Compare { .. } => None,
        Condition::RowPresent(slot) if facts.is_released(*slot) => Some(false),
        Condition::RowPresent(slot) => facts
            .read_of(*slot, at)
            .ok()
            .and_then(|read| facts.presence_of(&read)),
    };
    match holds {
        Some(true) => decided_expression(facts, when_true, at),
        Some(false) => decided_expression(facts, when_false, at),
        None => expression,
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
