//! Applying a function body's effects and terminal against the store:
//! point and range reads, bounded iteration, insert, update, delete, and
//! the branch/return/abort decision a block ends on.

mod iterate;
mod key;
mod read;
mod terminal;
mod write;

use std::collections::HashMap;

use crate::ir::{Block, Effect, Expr, Slot, Value};
use crate::store::Store;

use iterate::apply_iterate;
use read::{apply_read, apply_read_index_range, apply_read_range, IndexRangeRead, RangeRead};
use terminal::execute_terminal;
use write::{apply_delete, apply_insert, apply_update};

use super::resolve::resolve;
use super::{AccessLogEntry, Binding, CallContext, Outcome};

/// Applies a block's effects in program order, then its terminal. Returns
/// as soon as an effect aborts, before any later effect runs.
pub(super) fn execute_block(
    block: &Block,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Outcome {
    for effect in &block.effects {
        if let Some(outcome) = apply_effect(effect, ctx, store, bindings, access_log) {
            return outcome;
        }
    }
    execute_terminal(&block.terminal, ctx, store, bindings, access_log)
}

/// Applies one effect by dispatching to the function that owns its kind.
/// Returns `Some(outcome)` if the effect itself aborted the call, `None` if
/// execution should continue with the next effect.
fn apply_effect(
    effect: &Effect,
    ctx: &CallContext,
    store: &mut Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    match effect {
        Effect::Read { table, key, into } => {
            apply_read(table, key, *into, ctx, store, bindings, access_log)
        }
        Effect::Insert {
            table,
            key,
            columns,
        } => apply_insert(table, key, columns, ctx, store, bindings, access_log),
        Effect::Update {
            table,
            key,
            columns,
        } => apply_update(table, key, columns, ctx, store, bindings, access_log),
        Effect::Delete { table, key } => apply_delete(table, key, ctx, store, bindings, access_log),
        Effect::ReadRange {
            table,
            prefix,
            interval,
            bound,
            into,
        } => apply_read_range(
            RangeRead {
                table,
                prefix,
                interval: interval.as_ref(),
                bound,
                into: *into,
            },
            ctx,
            store,
            bindings,
            access_log,
        ),
        Effect::ReadIndexRange {
            index,
            start,
            end,
            bound,
            into,
        } => apply_read_index_range(
            IndexRangeRead {
                index,
                start,
                end,
                bound,
                into: *into,
            },
            ctx,
            store,
            bindings,
            access_log,
        ),
        Effect::Iterate { source, row, body } => {
            apply_iterate(*source, *row, body, ctx, store, bindings, access_log)
        }
    }
}

/// Resolves an effect's expression, turning any failure to resolve it into
/// the aborted [`Outcome`] every `apply_*` function below returns on error —
/// shared so every effect kind reports a failed key, a range's start, end,
/// or bound, or any other expression the same way.
fn resolve_or_abort(
    expr: &Expr,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Value, Outcome> {
    resolve(expr, ctx, bindings).map_err(|error| Outcome::Aborted {
        reason: error.into(),
    })
}
