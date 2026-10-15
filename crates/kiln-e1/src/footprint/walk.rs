//! Enumerates the feasible paths of a function body (Q1) and records, along
//! each, the accesses and the reads their targets depend on.

use kiln_ir::{
    Block, Condition, Effect, Expr, Function, IndexName, Schema, Slot, TableName, Terminal,
};

use super::cause::UnknownCause;
use super::dependency::{track_access, AccessRole};
use super::facts::{Binding, BindingKind, PathFacts, RangeBounds, Target};
use super::node_path::NodePath;
use super::operand::{operand_of, Operand, ReadRef};
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

enum RangeOver<'e> {
    PrimaryKeys(&'e TableName),
    Index(&'e IndexName),
}

struct RangeRead<'e> {
    over: RangeOver<'e>,
    start: &'e [Expr],
    end: &'e [Expr],
    bound: &'e Expr,
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
                condition: Condition::RowPresent(slot),
                when_present,
                when_absent,
            } => self.branch(*slot, [when_present, when_absent], terminal, facts),
        }
    }

    /// Explores the arms an enclosing test has not already ruled out. When
    /// both are feasible and reach different targets, the read deciding
    /// between them is a resolution (Q1.3, Q3.2).
    fn branch(
        &self,
        selector: Slot,
        [when_present, when_absent]: [&Block; 2],
        terminal: NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        let selector = facts.read_of(selector, &terminal)?;
        match facts.presence_of(&selector) {
            Some(true) => return self.block(when_present, &terminal.when_present(), facts),
            Some(false) => return self.block(when_absent, &terminal.when_absent(), facts),
            None => {}
        }
        let prefix = facts.accesses.len();
        let mut present = self.block(
            when_present,
            &terminal.when_present(),
            facts.assuming(&selector, true),
        )?;
        let mut absent = self.block(
            when_absent,
            &terminal.when_absent(),
            facts.assuming(&selector, false),
        )?;
        if reach_different_targets(&present, &absent, prefix) {
            for path in present.iter_mut().chain(&mut absent) {
                path.facts.resolve_selection(&selector, prefix);
            }
        }
        present.extend(absent);
        Ok(present)
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
            Effect::Read { table, key, into } => self.read(table, key, *into, node, facts),
            Effect::Insert { table, key, .. } => {
                self.keyed_access(facts, AccessRole::InsertKey, table, key, &node)?;
                Ok(())
            }
            Effect::Update { table, key, .. } | Effect::Delete { table, key } => {
                let role = AccessRole::Lookup(table.clone());
                self.keyed_access(facts, role, table, key, &node)?;
                Ok(())
            }
            Effect::ReadRange {
                table,
                start,
                end,
                bound,
                into,
            } => {
                let range = RangeRead {
                    over: RangeOver::PrimaryKeys(table),
                    start: std::slice::from_ref(start),
                    end: std::slice::from_ref(end),
                    bound,
                };
                self.read_range(range, *into, node, facts)
            }
            Effect::ReadIndexRange {
                index,
                start,
                end,
                bound,
                into,
            } => {
                let range = RangeRead {
                    over: RangeOver::Index(index),
                    start,
                    end,
                    bound,
                };
                self.read_range(range, *into, node, facts)
            }
            Effect::Iterate { source, row, body } => {
                facts.alias(*row, *source, &node)?;
                self.effects(body, &node.iteration_body(), facts)
            }
        }
    }

    fn read(
        &self,
        table: &TableName,
        key: &Expr,
        into: Slot,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), ClassifyError> {
        let role = AccessRole::Lookup(table.clone());
        let key = self.keyed_access(facts, role, table, key, &node)?;
        let kind = BindingKind::PointRead {
            table: table.clone(),
            key: key.operand,
        };
        bind(facts, into, kind, node, key.depends_on);
        Ok(())
    }

    fn read_range(
        &self,
        range: RangeRead<'_>,
        into: Slot,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), ClassifyError> {
        let bounds = RangeBounds {
            start: operands_of(facts, range.start, &node)?,
            end: operands_of(facts, range.end, &node)?,
            bound: operand_of(facts, range.bound, &node)?,
        };
        let (role, kind, target) = match range.over {
            RangeOver::PrimaryKeys(table) => (
                AccessRole::PrimaryRangeBound,
                BindingKind::PrimaryRange {
                    table: table.clone(),
                },
                Target::PrimaryRange {
                    table: table.clone(),
                    bounds: bounds.clone(),
                },
            ),
            RangeOver::Index(index) => (
                AccessRole::IndexBound,
                self.index_binding(index),
                Target::IndexRange {
                    index: index.clone(),
                    bounds: bounds.clone(),
                },
            ),
        };
        let depends_on = track_access(facts, self.schema, &role, bounds.all(), &node);
        if let BindingKind::UndeclaredIndexRange(undeclared) = &kind {
            facts.block(&node, UnknownCause::UndeclaredIndex(undeclared.clone()));
        }
        facts.record_access(&node, target);
        bind(facts, into, kind, node, depends_on);
        Ok(())
    }

    fn index_binding(&self, index: &IndexName) -> BindingKind {
        match self.schema.index(index) {
            Some(declared) => BindingKind::IndexRange {
                table: declared.table.clone(),
            },
            None => BindingKind::UndeclaredIndexRange(index.clone()),
        }
    }

    fn keyed_access(
        &self,
        facts: &mut PathFacts,
        role: AccessRole,
        table: &TableName,
        key: &Expr,
        node: &NodePath,
    ) -> Result<KeyedAccess, ClassifyError> {
        let operand = operand_of(facts, key, node)?;
        let depends_on = track_access(facts, self.schema, &role, [&operand], node);
        let target = Target::Row {
            table: table.clone(),
            key: operand.clone(),
        };
        facts.record_access(node, target);
        Ok(KeyedAccess {
            operand,
            depends_on,
        })
    }
}

struct KeyedAccess {
    operand: Operand,
    depends_on: Vec<ReadRef>,
}

fn bind(
    facts: &mut PathFacts,
    slot: Slot,
    kind: BindingKind,
    node: NodePath,
    depends_on: Vec<ReadRef>,
) {
    facts.bind(Binding {
        slot,
        kind,
        node,
        depends_on,
    });
}

fn operands_of(
    facts: &PathFacts,
    expressions: &[Expr],
    at: &NodePath,
) -> Result<Vec<Operand>, ClassifyError> {
    expressions
        .iter()
        .map(|expression| operand_of(facts, expression, at))
        .collect()
}

/// Whether the arms access different sets of targets after the `prefix`
/// accesses they share.
fn reach_different_targets(
    present: &[FeasiblePath],
    absent: &[FeasiblePath],
    prefix: usize,
) -> bool {
    let targets_of = |paths: &'_ [FeasiblePath]| -> Vec<Target> {
        paths
            .iter()
            .flat_map(|path| path.facts.targets_since(prefix).cloned())
            .collect()
    };
    let (present, absent) = (targets_of(present), targets_of(absent));
    let covers = |targets: &[Target], others: &[Target]| others.iter().all(|t| targets.contains(t));
    !(covers(&present, &absent) && covers(&absent, &present))
}
