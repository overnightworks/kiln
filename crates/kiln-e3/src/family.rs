use std::collections::BTreeSet;

use kiln_e2::quota::{self, QuotaAbort};
use kiln_ir::{
    AbortName, AbortReason, CheckConstraint, ColumnName, Comparison, Key, OverflowReason, Program,
    TableName, Value,
};

use crate::literal_pool;
use crate::sort::Sort;

/// A named state component and the kind of value it holds (grammar §1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateComponent {
    pub name: String,
    pub kind: Sort,
}

/// A category of `size` values, written `v0 … v(size-1)` (grammar §1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub name: String,
    pub size: usize,
}

/// The frozen state signature of a family (grammar §1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub components: Vec<StateComponent>,
    pub categories: Vec<Category>,
}

impl Signature {
    pub(crate) fn component(&self, name: &str) -> Option<&StateComponent> {
        self.components
            .iter()
            .find(|component| component.name == name)
    }

    pub(crate) fn category(&self, name: &str) -> Option<&Category> {
        self.categories
            .iter()
            .find(|category| category.name == name)
    }
}

/// One name of a family's frozen `Name` list (grammar §1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenName {
    pub reason: AbortReason,
    /// The width and check-violation names (†) occur only in checker form,
    /// never in a hole.
    pub checker_only: bool,
}

impl FrozenName {
    fn in_holes(reason: AbortReason) -> Self {
        Self {
            reason,
            checker_only: false,
        }
    }

    fn checker_only(reason: AbortReason) -> Self {
        Self {
            reason,
            checker_only: true,
        }
    }
}

/// The family context a checker form is validated against: the `Program`,
/// its signature, the literal pool read from the `Program` and the frozen
/// `Name` list.
#[derive(Debug, Clone)]
pub struct Family {
    program: Program,
    signature: Signature,
    literal_pool: BTreeSet<i64>,
    names: Vec<FrozenName>,
}

impl Family {
    /// `names` is the family's frozen list, copied from grammar §1.2; nothing
    /// is derived from the `Program`.
    pub fn new(program: Program, signature: Signature, names: Vec<FrozenName>) -> Self {
        Self {
            literal_pool: literal_pool::of(&program),
            program,
            signature,
            names,
        }
    }

    /// Quota allocation (grammar §1.1): one `Int` component, `remaining`.
    pub fn quota() -> Self {
        let signature = Signature {
            components: vec![StateComponent {
                name: "remaining".to_owned(),
                kind: Sort::Int,
            }],
            categories: Vec::new(),
        };
        Self::new(quota::program(), signature, quota_names())
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn literal_pool(&self) -> &BTreeSet<i64> {
        &self.literal_pool
    }

    pub fn names(&self) -> &[FrozenName] {
        &self.names
    }

    pub(crate) fn has_name(&self, reason: &AbortReason) -> bool {
        self.names.iter().any(|name| name.reason == *reason)
    }
}

/// Grammar §1.2, "Quota": the declared aborts, `Overflow(Arithmetic)`, and
/// the † width and check-violation names.
fn quota_names() -> Vec<FrozenName> {
    let quota_table = || TableName("quota".to_owned());
    let remaining = || ColumnName("remaining".to_owned());
    let hot_key = || Key(vec![Value::Integer(0)]);
    let check = |comparison, operand| AbortReason::CheckViolation {
        constraint: CheckConstraint {
            table: quota_table(),
            column: remaining(),
            comparison,
            operand: Value::Integer(operand),
        },
        key: hot_key(),
    };
    let declared = |abort: QuotaAbort| {
        FrozenName::in_holes(AbortReason::Declared(AbortName(abort.name().to_owned())))
    };
    vec![
        declared(QuotaAbort::InvalidQuantity),
        declared(QuotaAbort::QuotaExhausted),
        declared(QuotaAbort::QuotaFull),
        FrozenName::in_holes(AbortReason::Overflow(OverflowReason::Arithmetic)),
        FrozenName::checker_only(AbortReason::Overflow(OverflowReason::Width {
            table: quota_table(),
            column: remaining(),
            key: hot_key(),
        })),
        FrozenName::checker_only(check(Comparison::AtLeast, 0)),
        FrozenName::checker_only(check(Comparison::AtMost, 1000)),
    ]
}
