//! The element-wise comparator: two runs of one batch, call by call and on
//! the final state. A difference is a typed [`Divergence`], never a boolean.

use crate::run::{Abort, CallOutcome, Run};

/// What differs at the first diverging call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallDifference {
    /// One committed and the other aborted.
    OutcomeKind,
    /// Both committed, with different responses.
    Response,
    /// Both aborted, with different declared aborts (or a declared and a
    /// structural one).
    AbortReason,
    /// Both aborted structurally, with different payloads.
    AbortPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Divergence<N, S> {
    /// The runs agree on every call both have, and one has more calls.
    CallCount { expected: usize, actual: usize },
    Call {
        index: usize,
        difference: CallDifference,
        expected: Box<CallOutcome<N>>,
        actual: Box<CallOutcome<N>>,
    },
    /// Every call agrees; the states after the last call do not.
    FinalState { expected: S, actual: S },
}

/// Compares `actual` with `expected`, reporting the first divergence.
pub fn compare<N: PartialEq + Clone, S: PartialEq + Clone>(
    expected: &Run<N, S>,
    actual: &Run<N, S>,
) -> Result<(), Divergence<N, S>> {
    let first_difference = expected
        .outcomes
        .iter()
        .zip(&actual.outcomes)
        .enumerate()
        .find_map(|(index, (want, got))| {
            difference(want, got).map(|difference| Divergence::Call {
                index,
                difference,
                expected: Box::new(want.clone()),
                actual: Box::new(got.clone()),
            })
        });
    if let Some(divergence) = first_difference {
        return Err(divergence);
    }
    if expected.outcomes.len() != actual.outcomes.len() {
        return Err(Divergence::CallCount {
            expected: expected.outcomes.len(),
            actual: actual.outcomes.len(),
        });
    }
    if expected.final_state != actual.final_state {
        return Err(Divergence::FinalState {
            expected: expected.final_state.clone(),
            actual: actual.final_state.clone(),
        });
    }
    Ok(())
}

fn difference<N: PartialEq>(
    expected: &CallOutcome<N>,
    actual: &CallOutcome<N>,
) -> Option<CallDifference> {
    if expected == actual {
        return None;
    }
    Some(match (expected, actual) {
        (CallOutcome::Committed(_), CallOutcome::Committed(_)) => CallDifference::Response,
        (
            CallOutcome::Aborted(Abort::Structural(_)),
            CallOutcome::Aborted(Abort::Structural(_)),
        ) => CallDifference::AbortPayload,
        (CallOutcome::Aborted(_), CallOutcome::Aborted(_)) => CallDifference::AbortReason,
        _ => CallDifference::OutcomeKind,
    })
}
