use std::collections::HashMap;

use crate::identifiers::TableName;
use crate::ir::{Expr, Key, Slot, Value};

use super::super::{AbortReason, Binding, CallContext, Outcome};
use super::resolve_or_abort;

/// Resolves a key expression of `table` to the key it names, checking each
/// component against its key column (D8): the wrong kind or a value outside
/// the column's declared width aborts and is never clamped (D3).
///
/// Panics if `table` is undeclared or keyless, or `exprs` has another length
/// than its key: `Program::validate` rejected such a program before the call
/// ran.
pub(super) fn resolve_key(
    table: &TableName,
    exprs: &[Expr],
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Key, Outcome> {
    resolve_components(table, 0, exprs, ctx, bindings).map(Key)
}

/// Resolves `exprs` as the components of `table`'s key from `first_position`
/// on, checking each like [`resolve_key`]: a range prefix starts at 0, its
/// interval at the position after the prefix.
pub(super) fn resolve_components(
    table: &TableName,
    first_position: usize,
    exprs: &[Expr],
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Vec<Value>, Outcome> {
    let declared = ctx
        .schema
        .table(table)
        .unwrap_or_else(|| panic!("validated program names undeclared table {table:?}"));
    let mut components = Vec::with_capacity(exprs.len());
    for (name, expr) in declared.key_columns()[first_position..].iter().zip(exprs) {
        let value = resolve_or_abort(expr, ctx, bindings)?;
        let column = declared
            .column(name)
            .expect("a validated key names declared columns");
        if let Some(fault) = column.key_component_fault(&value) {
            return Err(Outcome::Aborted {
                reason: AbortReason::KeyComponentRejected {
                    table: table.clone(),
                    column: name.clone(),
                    fault,
                },
            });
        }
        components.push(value);
    }
    Ok(components)
}
