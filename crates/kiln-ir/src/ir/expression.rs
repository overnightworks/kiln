use crate::identifiers::{ColumnName, ParameterName};

use super::value::Value;

/// A position in a function's body that a later [`Condition`] or [`Expr`]
/// can refer back to, bound by an [`Effect::Read`] in that same body.
///
/// The compiler that emits a [`Function`] is responsible for giving each
/// read a distinct slot and for never referring to one before the read that
/// binds it has run; `run` trusts this invariant rather than re-checking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot(pub u32);

/// A numeric operator over two checked-integer operands (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOp {
    Add,
    Subtract,
    Multiply,
}

/// A value computable without touching the store: one of the call's own
/// arguments, a constant written into the program, a column projected from
/// an earlier read's row, or an arithmetic operation over two such values.
///
/// [`Expr::Projection`] projects a column out of a row an earlier
/// [`Effect::Read`] or [`Effect::Iterate`] already bound to a [`Slot`] in
/// this same call. Used as another [`Effect::Read`]'s `key`, it makes that
/// read a dependent read: which row to read next is itself a value the
/// call computed, not a literal or argument (`docs/concept.md` §2.1, §2.2
/// D6). A projection out of a [`Slot`] an [`Effect::ReadRange`] bound has no
/// single current row to read — whether because that range matched no row
/// or because the slot holds the whole matched sequence rather than one row
/// — and a projection out of an absent point-read row has none either;
/// `run` reports both the same way, as the typed
/// [`AbortReason::ProjectionSourceAbsent`](crate::evaluator::AbortReason::ProjectionSourceAbsent),
/// since which row a compiled program ends up projecting from is a fact
/// about the data it ran against, not something `run` can rule out ahead of
/// time. A key column projects the matching component of the row's key: the
/// key a point read used, or the store key of a range or index row (D8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Value),
    Parameter(ParameterName),
    Projection {
        slot: Slot,
        column: ColumnName,
    },
    Arithmetic(ArithmeticOp, Box<Expr>, Box<Expr>),
    /// Evaluates `condition`, then only the chosen arm (`docs/concept.md`
    /// §2.2 D9, "Values"): a failing projection in the other arm never runs
    /// and so never aborts.
    Conditional {
        condition: Box<Condition>,
        when_true: Box<Expr>,
        when_false: Box<Expr>,
    },
}

/// How a [`Condition::Compare`] relates its left operand to its right one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Relation {
    /// Whether `left` stands in this relation to `right`; `None` when the two
    /// are not of one kind. Text compares by its bytes.
    pub fn holds(self, left: &Value, right: &Value) -> Option<bool> {
        let ordering = match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
            (Value::Text(left), Value::Text(right)) => left.cmp(right),
            _ => return None,
        };
        Some(match self {
            Relation::Equal => ordering.is_eq(),
            Relation::NotEqual => ordering.is_ne(),
            Relation::Less => ordering.is_lt(),
            Relation::LessOrEqual => ordering.is_le(),
            Relation::Greater => ordering.is_gt(),
            Relation::GreaterOrEqual => ordering.is_ge(),
        })
    }
}

/// A condition a [`Terminal::Branch`] decides on; a branch takes its
/// `when_present` arm when the condition holds and its `when_absent` arm
/// otherwise.
///
/// `RowPresent` tests one fact for either read kind that can bind a
/// [`Slot`]: a point read (`docs/concept.md` §2.2 D6, "an absent key is a
/// branchable value") is present when it found a row, and a range read
/// (§2.2 D6, "an empty range reads as an empty sequence") is present when
/// its matched sequence is non-empty — the same branchable-absence fact
/// under one name, since both ultimately ask "did this read find anything".
///
/// `Compare` holds when `left` stands in `relation` to `right` (§2.2 D6,
/// comparison conditions). Whether it depends on the store is visible from
/// its operands: one without a projection is decided by the call's arguments
/// alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    RowPresent(Slot),
    Compare {
        left: Expr,
        relation: Relation,
        right: Expr,
    },
}
