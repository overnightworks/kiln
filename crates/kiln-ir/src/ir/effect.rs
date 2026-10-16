use std::collections::BTreeMap;

use crate::identifiers::{ColumnName, IndexName, TableName};

use super::expression::{Expr, Slot};

/// One state-touching step of a function's body, executed in program order
/// before the body's [`Terminal`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Read one row by key, binding its presence or absence to `into` for a
    /// later [`Condition::RowPresent`] to test.
    Read {
        table: TableName,
        key: Vec<Expr>,
        into: Slot,
    },
    /// Insert a new row. Aborts if a row already exists at `key`. `columns`
    /// holds the row's non-key columns: the key columns come only through
    /// `key` (D8).
    Insert {
        table: TableName,
        key: Vec<Expr>,
        columns: BTreeMap<ColumnName, Expr>,
    },
    /// Overwrite the named columns of an existing row, leaving its other
    /// columns unchanged. Aborts if no row exists at `key`. A key column is
    /// never named: keys do not change (D8).
    Update {
        table: TableName,
        key: Vec<Expr>,
        columns: BTreeMap<ColumnName, Expr>,
    },
    /// Remove a row. Aborts if no row exists at `key`.
    Delete { table: TableName, key: Vec<Expr> },
    /// Read every row whose key holds `prefix` as its first components and,
    /// when `interval` is given, whose next component lies in it, in key order
    /// (keys compare component by component), binding the matched sequence to
    /// `into` — empty if the interval is reversed or no row's key matches
    /// (`docs/concept.md` §2.2 D6, "an empty range reads as an empty
    /// sequence"; D8). `prefix` has fewer components than the key; an empty one
    /// reads the whole table, and an omitted `interval` matches every value of
    /// the next component.
    ///
    /// Always bounded (§2.1, "never an open-ended loop"): if more rows
    /// match than `bound` resolves to, the call aborts with
    /// [`AbortReason::BoundExceeded`](crate::evaluator::AbortReason::BoundExceeded)
    /// instead of binding `into` at all.
    ReadRange {
        table: TableName,
        prefix: Vec<Expr>,
        interval: Option<KeyInterval>,
        bound: Expr,
        into: Slot,
    },
    /// Read every row of `index`'s table whose indexed values, taken in the
    /// index's column order, lie in `[start, end]` (one bound per indexed
    /// column, compared lexicographically), in index order with ties broken
    /// by primary key, binding the matched sequence to `into` exactly as
    /// [`Effect::ReadRange`] does: empty if `start` sorts after `end` or
    /// nothing matches, and aborting past `bound` (`docs/concept.md` §2.2).
    ///
    /// The log records one entry for the index read itself; rows reached
    /// from the result are read by ordinary [`Effect::Read`]s.
    ReadIndexRange {
        index: IndexName,
        start: Vec<Expr>,
        end: Vec<Expr>,
        bound: Expr,
        into: Slot,
    },
    /// Run `body` once for each row `source` bound, in the order the read bound
    /// them, binding
    /// the current row to `row` so the body's own reads, writes, and
    /// projections can use it (§2.1, "bounded iteration"; `docs/concept.md`
    /// §2.2 D6, "bounded iteration proceeds in key order").
    ///
    /// `source` must be a [`Slot`] an earlier [`Effect::ReadRange`] in this
    /// same body bound: that read already resolved the bound and checked it
    /// against the matched row count (`docs/concept.md` §2.1, "never an
    /// open-ended loop"), so `Iterate` re-reads nothing and checks no bound
    /// of its own. An abort raised by any effect in `body` aborts the whole
    /// call, discarding every effect applied so far, in this row's part of
    /// the iteration or an earlier one (`docs/concept.md` §2.2).
    Iterate {
        source: Slot,
        row: Slot,
        body: Vec<Effect>,
    },
}

/// An inclusive interval on the key component after a range's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInterval {
    pub from: Expr,
    pub to: Expr,
}

/// The slots an [`Effect::Iterate`] with row slot `row` and this `body`
/// releases when its loop ends: the row slot and every slot the body binds,
/// at any depth, so nothing an iteration read outlives it. After the loop a
/// projection or branch on them sees an absence, whether the range held rows
/// or not.
pub fn slots_released_by_iteration(row: Slot, body: &[Effect]) -> Vec<Slot> {
    let mut released = vec![row];
    for effect in body {
        match effect {
            Effect::Read { into, .. }
            | Effect::ReadRange { into, .. }
            | Effect::ReadIndexRange { into, .. } => released.push(*into),
            Effect::Iterate { row, body, .. } => {
                released.extend(slots_released_by_iteration(*row, body));
            }
            Effect::Insert { .. } | Effect::Update { .. } | Effect::Delete { .. } => {}
        }
    }
    released
}
