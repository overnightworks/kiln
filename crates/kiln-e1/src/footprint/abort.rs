//! Whether the evaluator certainly aborts where a path resolves expressions
//! (`KILN-E1-27`, `KILN-E1-39`). Reachability comes first: a decided condition
//! runs its chosen arm alone, so nothing in the other arm is asked. What an
//! expression is made of is the operand's question, never this one's.

use kiln_ir::{Column, ColumnName, Condition, Expr, Schema};

use super::facts::PathFacts;
use super::node_path::NodePath;
use super::operand::ReadRef;
use super::ClassifyError;

/// An expression the evaluator resolves, with the key column it fills when it
/// is a key component.
pub type Resolved<'e> = (&'e Expr, Option<&'e Column>);

/// Whether resolving expressions on a path aborts the call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Aborts {
    Never,
    Always,
    /// A conditional the path does not decide certainly aborts on one arm
    /// but not on the other, so the data decides whether the call goes on.
    OnOneArm,
}

impl Aborts {
    /// The outcome of resolving one expression and then another: the call
    /// goes on only past both.
    fn then(self, next: Self) -> Self {
        match (self, next) {
            (Self::Always, _) | (_, Self::Always) => Self::Always,
            (Self::OnOneArm, _) | (_, Self::OnOneArm) => Self::OnOneArm,
            (Self::Never, Self::Never) => Self::Never,
        }
    }

    /// The outcome of a conditional whose condition the path does not decide:
    /// certain only where both arms agree.
    fn of_either_arm(when_true: Self, when_false: Self) -> Self {
        match (when_true, when_false) {
            (Self::Always, Self::Always) => Self::Always,
            (Self::Never, Self::Never) => Self::Never,
            _ => Self::OnOneArm,
        }
    }
}

/// A place where a path resolves expressions: what the schema declares and
/// what the path knows there.
pub struct Evaluation<'a> {
    pub schema: &'a Schema,
    pub facts: &'a PathFacts,
    pub at: &'a NodePath,
}

impl Evaluation<'_> {
    /// Whether resolving `resolved` in order certainly aborts. Every cause
    /// counts alike, whichever arm holds it: a projection of a slot holding
    /// no row, and a key component of a kind its key column cannot take
    /// (`KILN-IR-82`). Nothing after a certain abort is logged; an index
    /// range resolves both bounds first, but resolving logs nothing.
    pub fn aborts<'e>(
        &self,
        resolved: impl IntoIterator<Item = Resolved<'e>>,
    ) -> Result<Aborts, ClassifyError> {
        let mut aborts = Aborts::Never;
        for (expression, key_column) in resolved {
            aborts = aborts.then(self.expression_aborts(expression, key_column)?);
            if aborts == Aborts::Always {
                break;
            }
        }
        Ok(aborts)
    }

    fn expression_aborts(
        &self,
        expression: &Expr,
        key_column: Option<&Column>,
    ) -> Result<Aborts, ClassifyError> {
        Ok(match expression {
            Expr::Literal(_) | Expr::Parameter(_) => Aborts::Never,
            Expr::Projection { slot, .. } if self.facts.holds_no_row(*slot) => Aborts::Always,
            Expr::Projection { slot, column } => {
                let read = self.facts.read_of(*slot, self.at)?;
                match key_column {
                    Some(key_column) if self.misfits(key_column, &read, column) => Aborts::Always,
                    _ => Aborts::Never,
                }
            }
            Expr::Arithmetic(_, left, right) => self.aborts([(&**left, None), (&**right, None)])?,
            Expr::Conditional {
                condition,
                when_true,
                when_false,
            } => match self.condition_aborts(condition)? {
                Aborts::Always => Aborts::Always,
                deciding => deciding.then(self.chosen_arm_aborts(
                    condition,
                    [when_true, when_false],
                    key_column,
                )?),
            },
        })
    }

    fn condition_aborts(&self, condition: &Condition) -> Result<Aborts, ClassifyError> {
        match condition {
            Condition::RowPresent(_) => Ok(Aborts::Never),
            Condition::Compare { left, right, .. } => self.aborts([(left, None), (right, None)]),
        }
    }

    /// The arm a decided condition chooses runs alone; an undecided one may
    /// run either.
    fn chosen_arm_aborts(
        &self,
        condition: &Condition,
        [when_true, when_false]: [&Expr; 2],
        key_column: Option<&Column>,
    ) -> Result<Aborts, ClassifyError> {
        Ok(match decided_arm(self.facts, condition, self.at) {
            Some(true) => self.expression_aborts(when_true, key_column)?,
            Some(false) => self.expression_aborts(when_false, key_column)?,
            None => Aborts::of_either_arm(
                self.expression_aborts(when_true, key_column)?,
                self.expression_aborts(when_false, key_column)?,
            ),
        })
    }

    /// Whether `column` of a row `read` produced is of a kind `key_column`
    /// cannot take: on this path the row's table is the one `read` read.
    fn misfits(&self, key_column: &Column, read: &ReadRef, column: &ColumnName) -> bool {
        let source = self
            .facts
            .binding_of(read)
            .and_then(|binding| binding.kind.table())
            .and_then(|table| self.schema.column(table, column));
        source.is_some_and(|source| key_column.key_component_fault_from(source).is_some())
    }
}

/// The arm `condition` chooses on this path, if it chooses one statically: a
/// condition that compares two literals, or tests a slot whose presence the
/// path knows (`KILN-E1-19`) or an ended iteration released.
fn decided_arm(facts: &PathFacts, condition: &Condition, at: &NodePath) -> Option<bool> {
    match condition {
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
    }
}
