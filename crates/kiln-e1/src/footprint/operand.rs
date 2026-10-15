//! A key or bound as the classifier reasons about it: the IR expression with
//! every slot replaced by the read that bound it, so two accesses compare equal
//! exactly when they name the same data.

use kiln_ir::{ArithmeticOp, ColumnName, Expr, ParameterName, Slot, Value};

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
    Column {
        read: ReadRef,
        column: ColumnName,
    },
    /// The primary key of a row of a range or index read.
    KeyOf {
        read: ReadRef,
    },
    Arithmetic(ArithmeticOp, Box<Operand>, Box<Operand>),
}

/// One place an operand reads a read's result: a column, or the row's key.
pub struct Use {
    pub read: ReadRef,
    pub column: Option<ColumnName>,
}

impl Operand {
    pub fn uses(&self) -> Vec<Use> {
        match self {
            Self::Literal(_) | Self::Parameter(_) => Vec::new(),
            Self::Column { read, column } => vec![Use {
                read: read.clone(),
                column: Some(column.clone()),
            }],
            Self::KeyOf { read } => vec![Use {
                read: read.clone(),
                column: None,
            }],
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
/// The primary key of a point-read row is the key the read used.
pub fn operand_of(
    facts: &PathFacts,
    expression: &Expr,
    at: &NodePath,
) -> Result<Operand, ClassifyError> {
    Ok(match expression {
        Expr::Literal(value) => Operand::Literal(value.clone()),
        Expr::Parameter(parameter) => Operand::Parameter(parameter.clone()),
        Expr::Projection { slot, column } => Operand::Column {
            read: facts.read_of(*slot, at)?,
            column: column.clone(),
        },
        Expr::PrimaryKey { slot } => {
            let read = facts.read_of(*slot, at)?;
            match facts.binding_of(&read).map(|binding| &binding.kind) {
                Some(BindingKind::PointRead { key, .. }) => key.clone(),
                _ => Operand::KeyOf { read },
            }
        }
        Expr::Arithmetic(operator, left, right) => Operand::Arithmetic(
            *operator,
            Box::new(operand_of(facts, left, at)?),
            Box::new(operand_of(facts, right, at)?),
        ),
    })
}
