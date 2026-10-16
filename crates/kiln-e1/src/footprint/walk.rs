//! Enumerates the feasible paths of a function body (Q1) and records, along
//! each, the accesses and the reads their targets depend on.

use kiln_ir::{
    slots_released_by_iteration, Block, Condition, Effect, Expr, Function, IndexName, Schema, Slot,
    TableName, Terminal,
};

use super::carried::first_carried_slot;
use super::cause::UnknownCause;
use super::dependency::{track_access, AccessRole};
use super::facts::{Binding, BindingKind, PathFacts, RangeBounds, Target};
use super::node_path::NodePath;
use super::operand::{operand_of, Operand, ReadRef, Unresolvable};
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

/// Why walking a block's effects stopped short of its terminal.
enum Halt {
    /// The evaluator aborts at `at` with `ProjectionSourceAbsent`. Inside the
    /// body of an iteration whose range may hold no row, it aborts only when
    /// that range holds one: `unless_empty` is the read that bound the range.
    Aborts {
        at: NodePath,
        unless_empty: Option<ReadRef>,
    },
    Fault(ClassifyError),
}

impl Halt {
    fn at(node: &NodePath) -> impl Fn(Unresolvable) -> Self + '_ {
        move |unresolvable| match unresolvable {
            Unresolvable::ReleasedSource => Self::Aborts {
                at: node.clone(),
                unless_empty: None,
            },
            Unresolvable::Fault(error) => Self::Fault(error),
        }
    }

    /// The halt as the iteration over `rows` passes it on: an abort in its
    /// body happens only when `rows` holds a row, unless the path already
    /// knows it does or a nearer iteration's range already decides it.
    fn beyond_iteration_over(self, rows: &ReadRef, facts: &PathFacts) -> Self {
        match self {
            Self::Aborts {
                at,
                unless_empty: None,
            } if facts.presence_of(rows) != Some(true) => Self::Aborts {
                at,
                unless_empty: Some(rows.clone()),
            },
            halt => halt,
        }
    }
}

impl From<ClassifyError> for Halt {
    fn from(error: ClassifyError) -> Self {
        Self::Fault(error)
    }
}

struct Walker<'a> {
    schema: &'a Schema,
}

impl Walker<'_> {
    /// The paths through `block`. One that reaches an abort ends there, with
    /// the accesses before it (`KILN-IR-15`); an abort an iteration's body
    /// reaches forks on whether that range holds a row, and the path where it
    /// holds none skips the body.
    fn block(
        &self,
        block: &Block,
        at: &NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        let mut walked = facts.clone();
        match self.effects(&block.effects, at, &mut walked) {
            Ok(()) => self.terminal(block, at, walked),
            Err(Halt::Fault(error)) => Err(error),
            Err(Halt::Aborts {
                at: abort,
                unless_empty: None,
            }) => Ok(vec![FeasiblePath {
                facts: walked,
                terminal: abort,
            }]),
            Err(Halt::Aborts {
                at: abort,
                unless_empty: Some(rows),
            }) => {
                let mut paths = vec![FeasiblePath {
                    facts: walked.assuming(&rows, true),
                    terminal: abort,
                }];
                paths.extend(self.block(block, at, facts.assuming(&rows, false))?);
                Ok(paths)
            }
        }
    }

    fn terminal(
        &self,
        block: &Block,
        at: &NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
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
            match operand_of(self.schema, &facts, operand, &terminal) {
                Ok(operand) => data_selected |= !operand.uses().is_empty(),
                Err(Unresolvable::ReleasedSource) => {
                    return Ok(vec![FeasiblePath { facts, terminal }])
                }
                Err(Unresolvable::Fault(error)) => return Err(error),
            }
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
    ) -> Result<(), Halt> {
        for (index, effect) in effects.iter().enumerate() {
            self.effect(effect, at.effect(index), facts)?;
        }
        Ok(())
    }

    fn effect(&self, effect: &Effect, node: NodePath, facts: &mut PathFacts) -> Result<(), Halt> {
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
            Effect::Iterate { source, row, body } => self.iterate(*source, *row, body, node, facts),
        }
    }

    /// Walks the body once for the rows `source` bound, as their first
    /// iteration, so a body that carries a slot from one iteration to the next
    /// is refused at that slot's use. A body that would abort never runs over
    /// a range the path knows holds no row.
    fn iterate(
        &self,
        source: Slot,
        row: Slot,
        body: &[Effect],
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), Halt> {
        let rows = facts.read_of(source, &node)?;
        if let Some(carried) = first_carried_slot(row, body, &node.iteration_body()) {
            let cause = UnknownCause::SlotCarriedAcrossIterations { slot: carried.slot };
            facts.block(&carried.used_at, cause);
        }
        let mut inside = facts.clone();
        inside.enter_iteration(row, rows.clone(), &node);
        match self.effects(body, &node.iteration_body(), &mut inside) {
            Ok(()) => *facts = inside,
            Err(Halt::Aborts { .. }) if facts.presence_of(&rows) == Some(false) => {}
            Err(halt) => {
                *facts = inside;
                return Err(halt.beyond_iteration_over(&rows, facts));
            }
        }
        facts.end_iteration(slots_released_by_iteration(row, body));
        Ok(())
    }

    fn read(
        &self,
        table: &TableName,
        key: &[Expr],
        into: Slot,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), Halt> {
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
    ) -> Result<(), Halt> {
        let bounds = RangeBounds {
            start: self.operands_of(facts, range.start, &node)?,
            end: self.operands_of(facts, range.end, &node)?,
            bound: operand_of(self.schema, facts, range.bound, &node).map_err(Halt::at(&node))?,
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
    ) -> Result<KeyedAccess, Halt> {
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
    ) -> Result<Vec<Operand>, Halt> {
        expressions
            .iter()
            .map(|expression| operand_of(self.schema, facts, expression, at))
            .collect::<Result<_, _>>()
            .map_err(Halt::at(at))
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
