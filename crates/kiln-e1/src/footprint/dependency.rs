//! How an access target depends on earlier reads, and whether the schema lets
//! the compiler identify that dependency ahead of time (Q2, Q3).

use kiln_ir::{ColumnName, IndexName, Schema, TableName};

use super::cause::UnknownCause;
use super::facts::{Binding, BindingKind, PathFacts, ResolutionRole};
use super::node_path::NodePath;
use super::operand::{Operand, ReadRef};

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
    match role {
        AccessRole::Lookup(table) => track_lookup(facts, schema, table, &operands, node),
        _ => operands
            .iter()
            .flat_map(|operand| operand.uses())
            .for_each(|read| settle(facts, role, &read, node)),
    }
    operands.iter().flat_map(|operand| operand.uses()).collect()
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
    match lookup_edge(facts, schema, table, key, &reads) {
        Edge::Resolves(role, read) => facts.resolve(&read, role, std::slice::from_ref(node)),
        Edge::Covered => {}
        Edge::Blocked(cause) => facts.block(node, cause),
    }
}

fn settle(facts: &mut PathFacts, role: &AccessRole, read: &ReadRef, node: &NodePath) {
    if let Some(index) = undeclared_index(facts, read) {
        facts.block(node, UnknownCause::UndeclaredIndex(index));
        return;
    }
    match role {
        AccessRole::InsertKey => {
            facts.resolve(read, ResolutionRole::BaseRows, std::slice::from_ref(node));
        }
        AccessRole::IndexBound => {
            facts.resolve(
                read,
                ResolutionRole::IndexBounds,
                std::slice::from_ref(node),
            );
        }
        AccessRole::PrimaryRangeBound => {
            facts.block(node, UnknownCause::PrimaryRangeBoundedByRead);
        }
        AccessRole::Lookup(_) => unreachable!("a lookup is judged as a whole key"),
    }
}

fn bound<'f>(facts: &'f PathFacts, read: &ReadRef) -> &'f Binding {
    facts.binding_of(read).expect("a used read is bound")
}

/// The index `read` reads rows of when the schema does not declare it.
fn undeclared_index(facts: &PathFacts, read: &ReadRef) -> Option<IndexName> {
    match &bound(facts, read).kind {
        BindingKind::UndeclaredIndexRange(index) => Some(index.clone()),
        _ => None,
    }
}

fn source_table(binding: &Binding) -> Option<&TableName> {
    match &binding.kind {
        BindingKind::PointRead { table, .. }
        | BindingKind::PrimaryRange { table }
        | BindingKind::IndexRange { table } => Some(table),
        BindingKind::UndeclaredIndexRange(_) => None,
    }
}

/// The first of containment, index-to-base resolution and a declared foreign
/// key that applies to a key with read-derived components; otherwise why none
/// does (Clarification rule 2 (b), (c), (d), (e)).
fn lookup_edge(
    facts: &PathFacts,
    schema: &Schema,
    target: &TableName,
    key: &[&Operand],
    reads: &[ReadRef],
) -> Edge {
    if let Some(index) = reads.iter().find_map(|read| undeclared_index(facts, read)) {
        return Edge::Blocked(UnknownCause::UndeclaredIndex(index));
    }
    if let Some(edge) = whole_key_of_one_row(facts, schema, target, key) {
        return edge;
    }
    match key {
        [Operand::Column { read, column }] => {
            single_column_edge(facts, schema, target, read, column)
        }
        _ => Edge::Blocked(UnknownCause::KeyNotIdentified {
            table: target.clone(),
        }),
    }
}

/// The key of `target` read whole from one row of a range over `target`: every
/// component projects the matching key column of that row.
fn whole_key_of_one_row(
    facts: &PathFacts,
    schema: &Schema,
    target: &TableName,
    key: &[&Operand],
) -> Option<Edge> {
    let key_columns = schema.table(target)?.key_columns();
    let [Operand::Column { read, .. }, ..] = key else {
        return None;
    };
    let matches_key_column = |(component, key_column): (&&Operand, &ColumnName)| matches!(component, Operand::Column { read: from, column } if from == read && column == key_column);
    if key.len() != key_columns.len() || !key.iter().zip(key_columns).all(matches_key_column) {
        return None;
    }
    match &bound(facts, read).kind {
        BindingKind::PrimaryRange { table } if table == target => Some(Edge::Covered),
        BindingKind::IndexRange { table } if table == target => {
            Some(Edge::Resolves(ResolutionRole::BaseRows, read.clone()))
        }
        _ => None,
    }
}

/// A key of one component that projects `column` of `read`'s row: one
/// resolution through a declared foreign key from the row's table onto
/// `target`, otherwise unknown.
fn single_column_edge(
    facts: &PathFacts,
    schema: &Schema,
    target: &TableName,
    read: &ReadRef,
    column: &ColumnName,
) -> Edge {
    let from = source_table(bound(facts, read)).expect("a declared read has a table");
    let declared = schema.foreign_keys.iter().any(|foreign_key| {
        &foreign_key.table == from
            && &foreign_key.column == column
            && &foreign_key.references == target
    });
    if declared {
        return Edge::Resolves(ResolutionRole::BaseRows, read.clone());
    }
    let is_key_column = schema
        .table(from)
        .is_some_and(|table| table.key_position(column).is_some());
    Edge::Blocked(if is_key_column {
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
    })
}
