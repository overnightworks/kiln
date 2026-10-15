//! Serial replay of a call sequence (D4).

use crate::call::Call;
use crate::evaluator::{run, Execution};
use crate::ir::{Program, SchemaError};
use crate::store::Store;

/// Runs `calls` in order against `program`, each against the store the
/// previous call left. An aborted call leaves the store as it found it and
/// never stops the calls after it; every call's [`Execution`] sits at its
/// call's position. An invalid schema is rejected even for an empty
/// sequence.
pub fn replay(
    program: &Program,
    calls: &[Call],
    store: &Store,
) -> Result<Vec<Execution>, SchemaError> {
    program.schema.validate()?;
    let mut current = store.clone();
    calls
        .iter()
        .map(|call| {
            let execution = run(program, call, &current)?;
            current = execution.store.clone();
            Ok(execution)
        })
        .collect()
}
