//! Enumerates the feasible paths of a function body (Q1) and records, along
//! each, the accesses and the reads their targets depend on.

use kiln_ir::{Block, Effect, Expr, Function, ParameterName, Schema, Terminal};

use super::cause::UnknownCause;
use super::dependency::{track_access, AccessRole};
use super::facts::{Binding, BindingKind, Confinement, PathFacts};
use super::node_path::NodePath;
use super::ClassifyError;

pub struct FeasiblePath {
    pub facts: PathFacts,
    pub terminal: NodePath,
}

pub fn feasible_paths(
    schema: &Schema,
    function: &Function,
) -> Result<Vec<FeasiblePath>, ClassifyError> {
    let body = NodePath::body_of(&function.name);
    Walker { schema }.block(&function.body, &body, PathFacts::default())
}

struct Walker<'a> {
    schema: &'a Schema,
}

impl Walker<'_> {
    fn block(
        &self,
        block: &Block,
        at: &NodePath,
        mut facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        self.effects(&block.effects, at, &mut facts)?;
        let terminal = at.terminal();
        match &block.terminal {
            Terminal::Return(_) | Terminal::Abort(_) => Ok(vec![FeasiblePath { facts, terminal }]),
            Terminal::Branch {
                when_present,
                when_absent,
                ..
            } => {
                let mut paths =
                    self.block(when_present, &terminal.when_present(), facts.clone())?;
                paths.extend(self.block(when_absent, &terminal.when_absent(), facts)?);
                Ok(paths)
            }
        }
    }

    fn effects(
        &self,
        effects: &[Effect],
        at: &NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), ClassifyError> {
        for (index, effect) in effects.iter().enumerate() {
            self.effect(effect, at.effect(index), facts)?;
        }
        Ok(())
    }

    fn effect(
        &self,
        effect: &Effect,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), ClassifyError> {
        match effect {
            Effect::Read { table, key, into } => {
                let role = AccessRole::Lookup(table.clone());
                let depends_on = self.keyed_access(facts, role, key, &node)?;
                let kind = BindingKind::PointRead {
                    table: table.clone(),
                    key: key.clone(),
                };
                facts.bind(
                    *into,
                    Binding {
                        kind,
                        node,
                        depends_on,
                    },
                );
            }
            Effect::Insert { key, .. } => {
                self.keyed_access(facts, AccessRole::InsertKey, key, &node)?;
            }
            Effect::Update { table, key, .. } | Effect::Delete { table, key } => {
                let role = AccessRole::Lookup(table.clone());
                self.keyed_access(facts, role, key, &node)?;
            }
            Effect::ReadRange {
                table,
                start,
                end,
                bound,
                into,
            } => {
                let bounds = [start, end, bound];
                let role = AccessRole::PrimaryRangeBound;
                let depends_on = track_access(facts, self.schema, &role, bounds, &node)?;
                facts.record_access(&node, Confinement::Outside);
                let kind = BindingKind::PrimaryRange {
                    table: table.clone(),
                };
                facts.bind(
                    *into,
                    Binding {
                        kind,
                        node,
                        depends_on,
                    },
                );
            }
            Effect::ReadIndexRange {
                index,
                start,
                end,
                bound,
                into,
            } => {
                let bounds = start.iter().chain(end).chain([bound]);
                let kind = match self.schema.index(index) {
                    Some(declared) => BindingKind::IndexRange {
                        table: declared.table.clone(),
                        partition: pinned_partition(start, end),
                    },
                    None => BindingKind::UndeclaredIndexRange(index.clone()),
                };
                let depends_on =
                    track_access(facts, self.schema, &AccessRole::IndexBound, bounds, &node)?;
                if let BindingKind::UndeclaredIndexRange(undeclared) = &kind {
                    facts.block(&node, UnknownCause::UndeclaredIndex(undeclared.clone()));
                }
                facts.record_access(&node, confinement_of_range(&kind));
                facts.bind(
                    *into,
                    Binding {
                        kind,
                        node,
                        depends_on,
                    },
                );
            }
            Effect::Iterate { source, row, body } => {
                facts.alias(*row, *source, &node)?;
                self.effects(body, &node.iteration_body(), facts)?;
            }
        }
        Ok(())
    }

    fn keyed_access(
        &self,
        facts: &mut PathFacts,
        role: AccessRole,
        key: &Expr,
        node: &NodePath,
    ) -> Result<Vec<kiln_ir::Slot>, ClassifyError> {
        let confinement = confinement_of_key(facts, key, node)?;
        let origins = track_access(facts, self.schema, &role, [key], node)?;
        facts.record_access(node, confinement);
        Ok(origins)
    }
}

/// The parameter a range pins on both ends of its leading index column.
fn pinned_partition(start: &[Expr], end: &[Expr]) -> Option<ParameterName> {
    match (start.first(), end.first()) {
        (Some(Expr::Parameter(low)), Some(Expr::Parameter(high))) if low == high => {
            Some(low.clone())
        }
        _ => None,
    }
}

fn confinement_of_range(kind: &BindingKind) -> Confinement {
    match kind {
        BindingKind::IndexRange {
            partition: Some(parameter),
            ..
        } => Confinement::Partition(parameter.clone()),
        _ => Confinement::Outside,
    }
}

/// A key of another table than the index's is an obstacle, so it needs no
/// check here.
fn confinement_of_key(
    facts: &PathFacts,
    key: &Expr,
    at: &NodePath,
) -> Result<Confinement, ClassifyError> {
    match key {
        Expr::Parameter(parameter) => Ok(Confinement::Partition(parameter.clone())),
        Expr::PrimaryKey { slot } => {
            let origin = facts.origin(*slot, at)?;
            Ok(match facts.binding(origin).map(|binding| &binding.kind) {
                Some(BindingKind::IndexRange {
                    partition: Some(parameter),
                    ..
                }) => Confinement::Partition(parameter.clone()),
                _ => Confinement::Outside,
            })
        }
        _ => Ok(Confinement::Outside),
    }
}
