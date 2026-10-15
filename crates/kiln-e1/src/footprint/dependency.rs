//! How an access target depends on earlier reads, and whether the schema lets
//! the compiler identify that dependency ahead of time (Q2, Q3).

use kiln_ir::{ColumnName, Expr, Schema, Slot, TableName};

use super::cause::UnknownCause;
use super::facts::{Binding, BindingKind, PathFacts};
use super::node_path::NodePath;
use super::ClassifyError;

/// What an expression is used for.
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

struct Use {
    origin: Slot,
    column: Option<ColumnName>,
}

enum Edge {
    Resolves,
    Covered,
    Blocked(UnknownCause),
}

/// Records which earlier reads `expressions` depend on, as resolutions or
/// obstacles, and returns the reads depended on.
pub fn track_access<'a>(
    facts: &mut PathFacts,
    schema: &Schema,
    role: &AccessRole,
    expressions: impl IntoIterator<Item = &'a Expr>,
    node: &NodePath,
) -> Result<Vec<Slot>, ClassifyError> {
    let mut origins = Vec::new();
    for expression in expressions {
        for used in uses_of(facts, expression, node)? {
            let binding = facts.binding(used.origin).cloned();
            let binding = binding.expect("a used origin is bound");
            match edge(&binding, schema, role, &used) {
                Edge::Resolves => facts.resolve(used.origin, &binding.node, node),
                Edge::Covered => {}
                Edge::Blocked(cause) => facts.block(node, cause),
            }
            origins.push(used.origin);
        }
    }
    Ok(origins)
}

fn uses_of(facts: &PathFacts, expression: &Expr, at: &NodePath) -> Result<Vec<Use>, ClassifyError> {
    match expression {
        Expr::Literal(_) | Expr::Parameter(_) => Ok(Vec::new()),
        Expr::Arithmetic(_, left, right) => {
            let mut uses = uses_of(facts, left, at)?;
            uses.extend(uses_of(facts, right, at)?);
            Ok(uses)
        }
        Expr::Projection { slot, column } => Ok(vec![Use {
            origin: facts.origin(*slot, at)?,
            column: Some(column.clone()),
        }]),
        Expr::PrimaryKey { slot } => {
            let origin = facts.origin(*slot, at)?;
            match facts.binding(origin).map(|binding| &binding.kind) {
                Some(BindingKind::PointRead { key, .. }) => uses_of(facts, key, at),
                _ => Ok(vec![Use {
                    origin,
                    column: None,
                }]),
            }
        }
    }
}

fn source_table(binding: &Binding) -> Option<&TableName> {
    match &binding.kind {
        BindingKind::PointRead { table, .. }
        | BindingKind::PrimaryRange { table }
        | BindingKind::IndexRange { table, .. } => Some(table),
        BindingKind::UndeclaredIndexRange(_) => None,
    }
}

fn edge(binding: &Binding, schema: &Schema, role: &AccessRole, used: &Use) -> Edge {
    if let BindingKind::UndeclaredIndexRange(index) = &binding.kind {
        return Edge::Blocked(UnknownCause::UndeclaredIndex(index.clone()));
    }
    match role {
        AccessRole::InsertKey | AccessRole::IndexBound => Edge::Resolves,
        AccessRole::PrimaryRangeBound => Edge::Blocked(UnknownCause::PrimaryRangeBoundedByRead),
        AccessRole::Lookup(target) => lookup_edge(binding, schema, target, used),
    }
}

fn lookup_edge(binding: &Binding, schema: &Schema, target: &TableName, used: &Use) -> Edge {
    let from = source_table(binding).expect("a declared read has a table");
    match (&used.column, &binding.kind) {
        (Some(column), _) => {
            let declared = schema.foreign_keys.iter().any(|foreign_key| {
                &foreign_key.table == from
                    && &foreign_key.column == column
                    && &foreign_key.references == target
            });
            if declared {
                Edge::Resolves
            } else {
                Edge::Blocked(UnknownCause::NoDeclaredForeignKey {
                    from: from.clone(),
                    column: column.clone(),
                    to: target.clone(),
                })
            }
        }
        (None, _) if from != target => Edge::Blocked(UnknownCause::KeyOfAnotherTable {
            from: from.clone(),
            to: target.clone(),
        }),
        (None, BindingKind::PrimaryRange { .. }) => Edge::Covered,
        (None, _) => Edge::Resolves,
    }
}
