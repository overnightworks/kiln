//! Enumerates the feasible paths of a function body (Q1) and records, along
//! each, the accesses and the reads their targets depend on.

use kiln_ir::{
    slots_released_by_iteration, Block, Condition, Effect, Expr, Function, IndexName, Schema, Slot,
    TableName, Terminal,
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
                condition,
                when_present,
                when_absent,
            } => {
                let arms = [&**when_present, &**when_absent];
                match condition {
                    Condition::RowPresent(slot) => self.branch(*slot, arms, terminal, facts),
                    Condition::Compare { left, right, .. } => {
                        self.comparison_branch([left, right], arms, terminal, facts)
                    }
                }
            }
        }
    }

    /// Explores the arms an enclosing test has not already ruled out. Each arm
    /// stays a separate path, so `selection` can compare them (Q1, Q3). A slot
    /// an ended iteration released holds an absent row, so only the absent arm
    /// runs.
    fn branch(
        &self,
        selector: Slot,
        [when_present, when_absent]: [&Block; 2],
        terminal: NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        if facts.is_released(selector) {
            return self.block(when_absent, &terminal.when_absent(), facts);
        }
        let selector = facts.read_of(selector, &terminal)?;
        match facts.presence_of(&selector) {
            Some(true) => return self.block(when_present, &terminal.when_present(), facts),
            Some(false) => return self.block(when_absent, &terminal.when_absent(), facts),
            None => {}
        }
        let mut paths = self.block(
            when_present,
            &terminal.when_present(),
            facts.assuming(&selector, true),
        )?;
        paths.extend(self.block(
            when_absent,
            &terminal.when_absent(),
            facts.assuming(&selector, false),
        )?);
        Ok(paths)
    }

    /// An argument-selected comparison (no operand involves a read result)
    /// forks into arms that classify on their own (Q1); one that does is
    /// data-selected, with the branch itself as selector (Q3).
    fn comparison_branch(
        &self,
        operands: [&Expr; 2],
        [when_holds, when_fails]: [&Block; 2],
        terminal: NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        let mut data_selected = false;
        for operand in operands {
            let operand = operand_of(self.schema, &facts, operand, &terminal)?;
            data_selected |= !operand.uses().is_empty();
        }
        let arm = |holds: bool| {
            if data_selected {
                facts.deciding(&terminal, holds)
            } else {
                facts.deciding_by_argument(&terminal, holds)
            }
        };
        let mut paths = self.block(when_holds, &terminal.when_present(), arm(true))?;
        paths.extend(self.block(when_fails, &terminal.when_absent(), arm(false))?);
        Ok(paths)
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
                    start,
                    end,
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
                facts.enter_iteration(*row, *source, &node)?;
                self.effects(body, &node.iteration_body(), facts)?;
                facts.end_iteration(slots_released_by_iteration(*row, body));
                Ok(())
            }
        }
    }

    fn read(
        &self,
        table: &TableName,
        key: &[Expr],
        into: Slot,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), ClassifyError> {
        let role = AccessRole::Lookup(table.clone());
        let key = self.keyed_access(facts, role, table, key, &node)?;
        let kind = BindingKind::PointRead {
            table: table.clone(),
            key: key.operands,
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
            start: self.operands_of(facts, range.start, &node)?,
            end: self.operands_of(facts, range.end, &node)?,
            bound: operand_of(self.schema, facts, range.bound, &node)?,
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
        key: &[Expr],
        node: &NodePath,
    ) -> Result<KeyedAccess, ClassifyError> {
        let operands = self.operands_of(facts, key, node)?;
        let depends_on = track_access(facts, self.schema, &role, &operands, node);
        let target = Target::Row {
            table: table.clone(),
            key: operands.clone(),
        };
        facts.record_access(node, target);
        Ok(KeyedAccess {
            operands,
            depends_on,
        })
    }

    fn operands_of(
        &self,
        facts: &PathFacts,
        expressions: &[Expr],
        at: &NodePath,
    ) -> Result<Vec<Operand>, ClassifyError> {
        expressions
            .iter()
            .map(|expression| operand_of(self.schema, facts, expression, at))
            .collect()
    }
}

struct KeyedAccess {
    operands: Vec<Operand>,
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
