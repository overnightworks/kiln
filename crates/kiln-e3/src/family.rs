use std::collections::BTreeSet;

use kiln_e2::quota::{self, QuotaState};
use kiln_ir::{AbortReason, Program, Store};

use crate::scan::scan;
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

/// The family context a checker form is validated against: the `Program`,
/// its signature, the literal pool and the `Name` set, the last two read from
/// the `Program`.
#[derive(Debug, Clone)]
pub struct Family {
    program: Program,
    signature: Signature,
    literal_pool: BTreeSet<i64>,
    names: Vec<AbortReason>,
}

impl Family {
    /// `store` is any store of the family; its keys decide which width
    /// overflows the `Name` set holds.
    pub fn new(program: Program, signature: Signature, store: &Store) -> Self {
        let facts = scan(&program, store);
        Self {
            program,
            signature,
            literal_pool: facts.literal_pool,
            names: facts.names,
        }
    }

    /// Quota allocation (grammar §1.1): one `Int` component, `remaining`.
    ///
    /// The `Name` set is the three declared aborts and the width overflow of
    /// `quota.remaining`, as the item fixes it; grammar §1.2 also lists
    /// `Overflow(Arithmetic)` and `CheckViolation`, which a later slice owns.
    pub fn quota() -> Self {
        let signature = Signature {
            components: vec![StateComponent {
                name: "remaining".to_owned(),
                kind: Sort::Int,
            }],
            categories: Vec::new(),
        };
        Self::new(
            quota::program(),
            signature,
            &QuotaState { remaining: 0 }.to_store(),
        )
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

    pub fn names(&self) -> &[AbortReason] {
        &self.names
    }
}
