//! The oracle side of the comparison: a batch replayed through
//! `kiln_ir::replay`, converted into the typed [`Run`] the folds produce.

use kiln_ir::{replay, Call, Program, ProgramError, Store};

use crate::run::{CallOutcome, DeclaredAbort, Run};

/// A family's generated batch, as far as the oracle needs to know it.
pub trait FamilyBatch {
    type Abort: DeclaredAbort;
    type State: Copy;

    fn program() -> Program;
    fn initial_state(&self) -> Self::State;
    fn store_of(state: Self::State) -> Store;
    fn state_of(store: &Store) -> Self::State;
    fn calls(&self) -> Vec<Call>;
}

/// The batch replayed on the oracle.
pub fn oracle_run<B: FamilyBatch>(batch: &B) -> Result<Run<B::Abort, B::State>, ProgramError> {
    let initial = batch.initial_state();
    let executions = replay(&B::program(), &batch.calls(), &B::store_of(initial))?;
    Ok(Run {
        final_state: executions
            .last()
            .map_or(initial, |execution| B::state_of(&execution.store)),
        outcomes: executions
            .iter()
            .map(|execution| CallOutcome::from(&execution.outcome))
            .collect(),
    })
}

pub(crate) fn calls_of<C: Copy + Into<Call>>(calls: &[C]) -> Vec<Call> {
    calls.iter().map(|call| (*call).into()).collect()
}
