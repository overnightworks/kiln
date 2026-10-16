use std::collections::HashMap;
use std::slice::from_ref;

use crate::identifiers::{IndexName, TableName};
use crate::ir::{Expr, KeyInterval, Slot, Value};
use crate::store::{KeyedRow, Store};

use super::super::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns, Binding,
    BoundRow, CallContext, Outcome,
};
use super::key::{resolve_components, resolve_key};
use super::resolve_or_abort;

/// Reads one row by key, binding its presence or absence to `into`
/// (`Effect::Read`).
pub(super) fn apply_read(
    table: &TableName,
    key: &[Expr],
    into: Slot,
    ctx: &CallContext,
    store: &Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let key = match resolve_key(table, key, ctx, bindings) {
        Ok(key) => key,
        Err(outcome) => return Some(outcome),
    };
    let row = store.row(table, &key).map(|row| BoundRow {
        table: table.clone(),
        keyed: KeyedRow {
            key: key.clone(),
            row: row.clone(),
        },
    });
    access_log.push(AccessLogEntry {
        table: table.clone(),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(key),
        columns: AccessedColumns::EntireRow,
        origin: AccessOrigin::TransactionLogic,
    });
    bindings.insert(into, Binding::Row(row));
    None
}

/// An [`Effect::ReadRange`]'s own fields, bundled so `apply_read_range`
/// takes one value instead of five — the same reason [`CallContext`]
/// bundles the call's constant context, applied to one effect's fields
/// instead.
pub(super) struct RangeRead<'a> {
    pub(super) table: &'a TableName,
    pub(super) prefix: &'a [Expr],
    pub(super) interval: Option<&'a KeyInterval>,
    pub(super) bound: &'a Expr,
    pub(super) into: Slot,
}

/// Reads every row of `range.table` whose key holds `range.prefix` and, given
/// an interval, whose next component lies in it, binding the matched
/// sequence, in key order, to `range.into` (`Effect::ReadRange`).
///
/// Aborts with [`AbortReason::BoundExceeded`] if more rows match than
/// `range.bound` resolves to, without binding `range.into` at all —
/// bounded iteration never runs open-ended (`docs/concept.md` §2.1, §2.2
/// D6).
pub(super) fn apply_read_range(
    range: RangeRead,
    ctx: &CallContext,
    store: &Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let prefix = match resolve_components(range.table, 0, range.prefix, ctx, bindings) {
        Ok(prefix) => prefix,
        Err(outcome) => return Some(outcome),
    };
    let interval = match resolve_interval(range.table, range.interval, prefix.len(), ctx, bindings)
    {
        Ok(interval) => interval,
        Err(outcome) => return Some(outcome),
    };
    let bound = match resolve_bound(range.bound, range.table, ctx, bindings) {
        Ok(bound) => bound,
        Err(outcome) => return Some(outcome),
    };
    let rows = store.rows_in_range(
        range.table,
        &prefix,
        interval.as_ref().map(|(from, to)| (from, to)),
    );
    access_log.push(AccessLogEntry {
        table: range.table.clone(),
        kind: AccessKind::RangeRead,
        target: AccessTarget::Range { prefix, interval },
        columns: AccessedColumns::EntireRow,
        origin: AccessOrigin::TransactionLogic,
    });
    bind_within_bound(range.table, bound, rows, range.into, bindings)
}

/// Resolves a range's interval bounds as the key components after its prefix.
fn resolve_interval(
    table: &TableName,
    interval: Option<&KeyInterval>,
    prefix_len: usize,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Option<(Value, Value)>, Outcome> {
    let Some(interval) = interval else {
        return Ok(None);
    };
    let component = |bound: &Expr| {
        let resolved = resolve_components(table, prefix_len, from_ref(bound), ctx, bindings)?;
        Ok(resolved
            .into_iter()
            .next()
            .expect("one bound, one component"))
    };
    let from = component(&interval.from)?;
    let to = component(&interval.to)?;
    Ok(Some((from, to)))
}

/// Resolves a range read's bound, which counts rows and so must be an
/// integer.
pub(super) fn resolve_bound(
    bound: &Expr,
    table: &TableName,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<i64, Outcome> {
    match resolve_or_abort(bound, ctx, bindings)? {
        Value::Integer(bound) => Ok(bound),
        Value::Text(_) => Err(Outcome::Aborted {
            reason: AbortReason::NonIntegerBound {
                table: table.clone(),
            },
        }),
    }
}

/// Binds the matched `rows` to `into`, or aborts with `BoundExceeded`
/// without binding anything if they outnumber `bound`.
pub(super) fn bind_within_bound(
    table: &TableName,
    bound: i64,
    rows: Vec<KeyedRow>,
    into: Slot,
    bindings: &mut HashMap<Slot, Binding>,
) -> Option<Outcome> {
    if usize::try_from(bound).map_or(true, |bound| rows.len() > bound) {
        return Some(Outcome::Aborted {
            reason: AbortReason::BoundExceeded {
                table: table.clone(),
                bound,
                matched: rows.len(),
            },
        });
    }
    bindings.insert(
        into,
        Binding::Range {
            table: table.clone(),
            rows,
        },
    );
    None
}

/// An [`Effect::ReadIndexRange`]'s own fields, bundled like [`RangeRead`].
pub(super) struct IndexRangeRead<'a> {
    pub(super) index: &'a IndexName,
    pub(super) start: &'a [Expr],
    pub(super) end: &'a [Expr],
    pub(super) bound: &'a Expr,
    pub(super) into: Slot,
}

/// Reads every row in `[start, end]` of a declared index, binding them in
/// index order to `read.into` (`Effect::ReadIndexRange`). The one log entry
/// names the index and its bounds; no base-row access is logged for the rows
/// it matches.
///
/// Panics if `read.index` is undeclared or a bound's length differs from the
/// index's column count: static faults of the compiled program, like an
/// unbound iteration source.
pub(super) fn apply_read_index_range(
    read: IndexRangeRead,
    ctx: &CallContext,
    store: &Store,
    bindings: &mut HashMap<Slot, Binding>,
    access_log: &mut Vec<AccessLogEntry>,
) -> Option<Outcome> {
    let index = ctx
        .schema
        .index(read.index)
        .unwrap_or_else(|| panic!("program declares no index named {:?}", read.index));
    assert!(
        read.start.len() == index.columns.len() && read.end.len() == index.columns.len(),
        "index {:?} has {} columns but its range read gives {} start and {} end bounds",
        read.index,
        index.columns.len(),
        read.start.len(),
        read.end.len(),
    );
    let (start, end) = match (
        resolve_all(read.start, ctx, bindings),
        resolve_all(read.end, ctx, bindings),
    ) {
        (Ok(start), Ok(end)) => (start, end),
        (Err(outcome), _) | (_, Err(outcome)) => return Some(outcome),
    };
    let bound = match resolve_bound(read.bound, &index.table, ctx, bindings) {
        Ok(bound) => bound,
        Err(outcome) => return Some(outcome),
    };
    access_log.push(AccessLogEntry {
        table: index.table.clone(),
        kind: AccessKind::IndexRangeRead,
        target: AccessTarget::IndexRange {
            index: index.name.clone(),
            start: start.clone(),
            end: end.clone(),
        },
        columns: AccessedColumns::Named(index.columns.clone()),
        origin: AccessOrigin::TransactionLogic,
    });
    let table = ctx
        .schema
        .table(&index.table)
        .unwrap_or_else(|| panic!("validated index names undeclared table {:?}", index.table));
    let rows = store.rows_in_index_range(table, &index.columns, &start, &end);
    bind_within_bound(&index.table, bound, rows, read.into, bindings)
}

pub(super) fn resolve_all(
    exprs: &[Expr],
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Vec<Value>, Outcome> {
    exprs
        .iter()
        .map(|expr| resolve_or_abort(expr, ctx, bindings))
        .collect()
}
