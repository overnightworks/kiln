//! How an access target depends on earlier reads, and whether the schema lets
//! the compiler identify that dependency ahead of time (Q2, Q3).

use kiln_ir::{Schema, TableName};

use super::cause::UnknownCause;
use super::facts::{Binding, BindingKind, PathFacts, ResolutionRole};
use super::node_path::NodePath;
use super::operand::{Operand, ReadRef, Use};

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
    Resolves(ResolutionRole),
    Covered,
    Blocked(UnknownCause),
}

/// Records which earlier reads `operands` depend on, as resolutions or
/// obstacles, and returns the reads depended on.
pub fn track_access<'a>(
    facts: &mut PathFacts,
    schema: &Schema,
    role: &AccessRole,
    operands: impl IntoIterator<Item = &'a Operand>,
    node: &NodePath,
) -> Vec<ReadRef> {
    let mut origins = Vec::new();
    for operand in operands {
        let uses = operand.uses();
        match role {
            AccessRole::Lookup(table) if operand.is_computed_from_reads() => facts.block(
                node,
                UnknownCause::KeyComputedFromRead {
                    table: table.clone(),
                },
            ),
            _ => uses
                .iter()
                .for_each(|used| settle(facts, schema, role, used, node)),
        }
        origins.extend(uses.into_iter().map(|used| used.read));
    }
    origins
}

fn settle(facts: &mut PathFacts, schema: &Schema, role: &AccessRole, used: &Use, node: &NodePath) {
    let binding = facts.binding_of(&used.read).cloned();
    let binding = binding.expect("a used read is bound");
    match edge(&binding, schema, role, used) {
        Edge::Resolves(resolution_role) => {
            facts.resolve(&used.read, resolution_role, std::slice::from_ref(node));
        }
        Edge::Covered => {}
        Edge::Blocked(cause) => facts.block(node, cause),
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

fn edge(binding: &Binding, schema: &Schema, role: &AccessRole, used: &Use) -> Edge {
    if let BindingKind::UndeclaredIndexRange(index) = &binding.kind {
        return Edge::Blocked(UnknownCause::UndeclaredIndex(index.clone()));
    }
    match role {
        AccessRole::InsertKey => Edge::Resolves(ResolutionRole::BaseRows),
        AccessRole::IndexBound => Edge::Resolves(ResolutionRole::IndexBounds),
        AccessRole::PrimaryRangeBound => Edge::Blocked(UnknownCause::PrimaryRangeBoundedByRead),
        AccessRole::Lookup(target) => lookup_edge(binding, schema, target, used),
    }
}

fn lookup_edge(binding: &Binding, schema: &Schema, target: &TableName, used: &Use) -> Edge {
    let from = source_table(binding).expect("a declared read has a table");
    match &used.column {
        Some(column) => {
            let declared = schema.foreign_keys.iter().any(|foreign_key| {
                &foreign_key.table == from
                    && &foreign_key.column == column
                    && &foreign_key.references == target
            });
            if declared {
                Edge::Resolves(ResolutionRole::BaseRows)
            } else {
                Edge::Blocked(UnknownCause::NoDeclaredForeignKey {
                    from: from.clone(),
                    column: column.clone(),
                    to: target.clone(),
                })
            }
        }
        None if from != target => Edge::Blocked(UnknownCause::KeyOfAnotherTable {
            from: from.clone(),
            to: target.clone(),
        }),
        None if matches!(binding.kind, BindingKind::PrimaryRange { .. }) => Edge::Covered,
        None => Edge::Resolves(ResolutionRole::BaseRows),
    }
}
