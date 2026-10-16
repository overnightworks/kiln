//! How an access target depends on earlier reads, and whether the schema lets
//! the compiler identify that dependency ahead of time (Q2, Q3).

use kiln_ir::{ColumnName, Schema, TableName};

use super::cause::UnknownCause;
use super::facts::{Binding, BindingKind, PathFacts, ResolutionRole};
use super::node_path::NodePath;
use super::operand::{column_operand, Operand, ReadRef};

/// What an operand is used for.
pub enum AccessRole {
    /// The key of a row of this table that is read, updated or deleted.
    Lookup(TableName),
    /// The key of a row to insert.
    InsertKey,
    /// A bound of a range over a declared index.
    IndexBound,
    /// A bound of a primary-key range.
    PrimaryRangeBound,
}

enum Edge {
    Resolves(ResolutionRole, ReadRef),
    Covered,
    Blocked(UnknownCause),
}

/// Records which earlier reads `operands` depend on, as resolutions or
/// obstacles, and returns the reads depended on. The operands of a lookup are
/// the components of one key and are judged together (`inputs.md` §5,
/// Clarification rule 2).
pub fn track_access<'a>(
    facts: &mut PathFacts,
    schema: &Schema,
    role: &AccessRole,
    operands: impl IntoIterator<Item = &'a Operand>,
    node: &NodePath,
) -> Vec<ReadRef> {
    let operands: Vec<&Operand> = operands.into_iter().collect();
    let reads: Vec<ReadRef> = operands.iter().flat_map(|operand| operand.uses()).collect();
    let resolving = |resolution_role| {
        reads
            .iter()
            .map(|read| (read, resolution_role))
            .collect::<Vec<_>>()
    };
    match role {
        AccessRole::Lookup(table) => track_lookup(facts, schema, table, &operands, node),
        AccessRole::InsertKey => resolve_each(facts, resolving(ResolutionRole::BaseRows), node),
        AccessRole::IndexBound => {
            resolve_each(facts, resolving(ResolutionRole::IndexBounds), node);
        }
        AccessRole::PrimaryRangeBound if !reads.is_empty() => {
            facts.block(node, UnknownCause::PrimaryRangeBoundedByRead);
        }
        AccessRole::PrimaryRangeBound => {}
    }
    reads
}

fn resolve_each(facts: &mut PathFacts, reads: Vec<(&ReadRef, ResolutionRole)>, node: &NodePath) {
    for (read, resolution_role) in reads {
        facts.resolve(read, resolution_role, std::slice::from_ref(node));
    }
}

fn track_lookup(
    facts: &mut PathFacts,
    schema: &Schema,
    table: &TableName,
    key: &[&Operand],
    node: &NodePath,
) {
    if key
        .iter()
        .any(|component| component.is_computed_from_reads())
    {
        facts.block(
            node,
            UnknownCause::KeyComputedFromRead {
                table: table.clone(),
            },
        );
        return;
    }
    let reads: Vec<ReadRef> = key.iter().flat_map(|component| component.uses()).collect();
    if reads.is_empty() {
        return;
    }
    match lookup_edge(facts, schema, table, key) {
        Edge::Resolves(role, read) => facts.resolve(&read, role, std::slice::from_ref(node)),
        Edge::Covered => {}
        Edge::Blocked(cause) => facts.block(node, cause),
    }
}

fn bound<'f>(facts: &'f PathFacts, read: &ReadRef) -> &'f Binding {
    facts.binding_of(read).expect("a used read is bound")
}

fn source_table(binding: &Binding) -> Option<&TableName> {
    match &binding.kind {
        BindingKind::PointRead { table, .. }
        | BindingKind::PrimaryRange { table, .. }
        | BindingKind::IndexRange { table } => Some(table),
        BindingKind::UndeclaredIndexRange(_) => None,
    }
}

/// The first of containment, index-to-base resolution and a declared foreign
/// key that applies to a key with read-derived components; otherwise why none
/// does (Clarification rule 2 (b), (c), (d), (e)).
fn lookup_edge(facts: &PathFacts, schema: &Schema, target: &TableName, key: &[&Operand]) -> Edge {
    if let Some(edge) = whole_key_of_one_row(facts, schema, target, key) {
        return edge;
    }
    foreign_key_edge(facts, schema, target, key)
}

/// The key of `target` read whole from one row of a range over `target`: every
/// component is the matching key column of the same row binding, or what the
/// range's prefix fixes for it, so the key is that row's and no mix of two
/// rows of the range.
fn whole_key_of_one_row(
    facts: &PathFacts,
    schema: &Schema,
    target: &TableName,
    key: &[&Operand],
) -> Option<Edge> {
    let key_columns = schema.table(target)?.key_columns();
    let Some(Operand::Column { read, row, .. }) = key
        .iter()
        .copied()
        .find(|component| matches!(component, Operand::Column { .. }))
    else {
        return None;
    };
    let is_key_column_of_row = |(component, key_column): (&&Operand, &ColumnName)| {
        **component == column_operand(schema, facts, read.clone(), row.clone(), key_column)
    };
    if key.len() != key_columns.len() || !key.iter().zip(key_columns).all(is_key_column_of_row) {
        return None;
    }
    match &bound(facts, read).kind {
        BindingKind::PrimaryRange { table, .. } if table == target => Some(Edge::Covered),
        BindingKind::IndexRange { table } if table == target => {
            Some(Edge::Resolves(ResolutionRole::BaseRows, read.clone()))
        }
        _ => None,
    }
}

/// A key whose components are, in order, the source columns of one declared
/// foreign key onto `target`'s whole key, each read from the row of the first
/// read-derived component (or equal to it by an alias, `column_operand`): one
/// resolution through that foreign key. Otherwise unknown, and a partial match
/// is no match.
fn foreign_key_edge(
    facts: &PathFacts,
    schema: &Schema,
    target: &TableName,
    key: &[&Operand],
) -> Edge {
    let Some(Operand::Column { read, row, .. }) = key
        .iter()
        .copied()
        .find(|component| matches!(component, Operand::Column { .. }))
    else {
        return not_identified(target);
    };
    let Some(from) = source_table(bound(facts, read)) else {
        return Edge::Covered;
    };
    let projects_source_column = |(component, column): (&&Operand, &ColumnName)| {
        **component == column_operand(schema, facts, read.clone(), row.clone(), column)
    };
    let declared = schema.foreign_keys.iter().any(|foreign_key| {
        &foreign_key.table == from
            && &foreign_key.references == target
            && foreign_key.columns.len() == key.len()
            && key
                .iter()
                .zip(&foreign_key.columns)
                .all(projects_source_column)
    });
    if declared {
        return Edge::Resolves(ResolutionRole::BaseRows, read.clone());
    }
    match key {
        [Operand::Column { column, .. }] => {
            Edge::Blocked(missing_foreign_key(schema, from, target, column))
        }
        _ => not_identified(target),
    }
}

fn not_identified(target: &TableName) -> Edge {
    Edge::Blocked(UnknownCause::KeyNotIdentified {
        table: target.clone(),
    })
}

/// Why a key of one component, `column` of a row of `from`, resolves through
/// no foreign key into `target`.
fn missing_foreign_key(
    schema: &Schema,
    from: &TableName,
    target: &TableName,
    column: &ColumnName,
) -> UnknownCause {
    let is_key_column = schema
        .table(from)
        .is_some_and(|table| table.key_position(column).is_some());
    if is_key_column {
        UnknownCause::KeyOfAnotherTable {
            from: from.clone(),
            to: target.clone(),
        }
    } else {
        UnknownCause::NoDeclaredForeignKey {
            from: from.clone(),
            column: column.clone(),
            to: target.clone(),
        }
    }
}
