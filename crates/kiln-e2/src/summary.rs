//! The interface every family's summary algebra implements: the atomic
//! summary of one call, `combine`, `apply`, the call's response at a given
//! pre-state, and the representation's size. The operations take an
//! [`OperationCounter`] so the executed work can be counted where it runs.

use crate::run::CallOutcome;

/// Counts one executed operation each time [`count`](Self::count) runs.
pub trait OperationCounter {
    fn count(&mut self);
}

/// Counts nothing. A zero-sized type whose `count` is empty: the operations
/// are generic over the counter, so the uncounted instantiation compiles to
/// the same code as an implementation without counting.
#[derive(Debug, Default, Clone, Copy)]
pub struct Uncounted;

impl OperationCounter for Uncounted {
    #[inline(always)]
    fn count(&mut self) {}
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OperationCount(u64);

impl OperationCount {
    pub fn total(self) -> u64 {
        self.0
    }
}

impl OperationCounter for OperationCount {
    fn count(&mut self) {
        self.0 += 1;
    }
}

pub trait Summary: Sized {
    type Call: Copy;
    type State: Copy;
    type Abort;

    fn atomic(call: Self::Call) -> Self;

    /// `self` is the earlier summary, `later` the one applied after it.
    fn combine<C: OperationCounter>(&self, later: &Self, counter: &mut C) -> Self;

    fn apply<C: OperationCounter>(&self, state: Self::State, counter: &mut C) -> Self::State;

    /// What `call` yields when it runs on `pre_state`.
    fn response(call: Self::Call, pre_state: Self::State) -> CallOutcome<Self::Abort>;

    /// The size of the frozen representation, not of the in-memory layout.
    fn size_in_bytes(&self) -> usize;
}
