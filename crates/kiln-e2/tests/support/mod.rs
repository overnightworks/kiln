//! Replay plumbing shared by the criteria tests: a case states a program, an
//! initial state, calls and what each call yields.
#![allow(dead_code)]

use std::fmt::Debug;

use kiln_ir::{replay, AbortName, AbortReason, Call, Execution, Outcome, Program, Store, Value};

pub fn committed(response: i64) -> Outcome {
    Outcome::Committed {
        response: Value::Integer(response),
    }
}

pub fn aborted(name: &str) -> Outcome {
    Outcome::Aborted {
        reason: AbortReason::Declared(AbortName(name.to_owned())),
    }
}

pub fn replay_all<C: Copy + Into<Call>>(
    program: &Program,
    initial: Store,
    calls: &[C],
) -> Vec<Execution> {
    let calls: Vec<Call> = calls.iter().map(|call| (*call).into()).collect();
    replay(program, &calls, &initial).expect("the oracle's schema is valid")
}

/// One worked-example row: the call, its outcome, and the state after it.
pub type Row<C, S> = (C, Outcome, S);

/// Replays `rows` from `initial` and asserts, per call, its outcome and the
/// state it leaves.
pub fn assert_rows<C, S>(
    program: &Program,
    initial: Store,
    rows: &[Row<C, S>],
    state_of: fn(&Store) -> S,
) where
    C: Copy + Debug + Into<Call>,
    S: Debug + PartialEq,
{
    let calls: Vec<C> = rows.iter().map(|(call, _, _)| *call).collect();
    let executions = replay_all(program, initial, &calls);
    for (position, ((call, outcome, after), execution)) in rows.iter().zip(&executions).enumerate()
    {
        assert_eq!(
            &execution.outcome, outcome,
            "outcome of call {position}: {call:?}"
        );
        assert_eq!(
            &state_of(&execution.store),
            after,
            "state after call {position}: {call:?}"
        );
    }
}

pub fn outcomes_of<C: Copy + Into<Call>>(
    program: &Program,
    initial: Store,
    calls: &[C],
) -> Vec<Outcome> {
    replay_all(program, initial, calls)
        .into_iter()
        .map(|execution| execution.outcome)
        .collect()
}

pub fn final_store<C: Copy + Into<Call>>(program: &Program, initial: Store, calls: &[C]) -> Store {
    replay_all(program, initial.clone(), calls)
        .pop()
        .map_or(initial, |execution| execution.store)
}
