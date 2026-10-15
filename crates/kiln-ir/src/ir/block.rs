use crate::identifiers::AbortName;

use super::effect::Effect;
use super::expression::{Condition, Expr};

/// How a block of [`Effect`]s ends: every path through a function's body
/// reaches exactly one `Terminal`, so a function always either returns or
/// aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminal {
    /// Return a response to the caller, committing every effect applied so
    /// far.
    Return(Expr),
    /// Reject the call by name. Every effect this call applied, in either
    /// branch, is discarded (`docs/concept.md` §2.2).
    Abort(AbortName),
    /// Take `when_present` if `condition` holds, `when_absent` otherwise.
    Branch {
        condition: Condition,
        when_present: Box<Block>,
        when_absent: Box<Block>,
    },
}

/// A sequence of effects followed by the terminal that ends them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub effects: Vec<Effect>,
    pub terminal: Terminal,
}
