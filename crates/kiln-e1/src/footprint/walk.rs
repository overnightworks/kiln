//! Enumerates the feasible paths of a function body (Q1) and records, along
//! each, the accesses and the reads their targets depend on.

use std::collections::BTreeMap;

use kiln_ir::{
    slots_released_by_iteration, Block, ColumnName, Condition, Effect, Expr, Function, IndexName,
    KeyInterval, Schema, Slot, TableName, Terminal,
};

use super::abort::{Aborts, Evaluation, Resolved};
use super::carried::first_carried_slot;
use super::cause::UnknownCause;
use super::dependency::{track_access, AccessRole};
use super::facts::{Binding, BindingKind, Obstacle, PathFacts, PrimaryBounds, RangeBounds, Target};
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

struct PrimaryRangeRead<'e> {
    table: &'e TableName,
    prefix: &'e [Expr],
    interval: Option<&'e KeyInterval>,
    bound: &'e Expr,
}

struct IndexRangeRead<'e> {
    index: &'e IndexName,
    start: &'e [Expr],
    end: &'e [Expr],
    bound: &'e Expr,
}

/// Why walking a block stopped short of deciding its terminal.
enum Halt {
    /// The evaluator certainly aborts at `at`, which projects a slot holding
    /// no row or a column its key column cannot take.
    /// Inside the body of an iteration whose range may hold no row, it aborts
    /// only when that range holds one: `unless_empty` is the read that bound
    /// the range.
    Aborts {
        at: NodePath,
        unless_empty: Option<ReadRef>,
    },
    /// What the evaluator does at `at` depends on the data, in a way that
    /// hides the targets after it from the comparison of paths: the path
    /// ends there unknown (`KILN-E1-39`).
    Refuses {
        at: NodePath,
    },
    Fault(ClassifyError),
}

impl Halt {
    fn aborting_at(node: &NodePath) -> Self {
        Self::Aborts {
            at: node.clone(),
            unless_empty: None,
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

impl<'a> Walker<'a> {
    /// The paths through `block`: its effects, then what its terminal reads,
    /// then the terminal. One that reaches an abort ends there, with the
    /// accesses before it (`KILN-IR-15`); an abort an iteration's body reaches
    /// forks on whether that range holds a row, and the path where it holds
    /// none skips the body.
    fn block(
        &self,
        block: &Block,
        at: &NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        let mut walked = facts.clone();
        let terminal = at.terminal();
        let reached = self
            .effects(&block.effects, at, &mut walked)
            .and_then(|()| self.terminal_operands(&walked, &block.terminal, &terminal));
        match reached {
            Ok(operands) => self.terminal(&block.terminal, terminal, &operands, walked),
            Err(Halt::Fault(error)) => Err(error),
            Err(Halt::Refuses { at: conditional }) => {
                walked.obstacles.push(Obstacle {
                    node: conditional,
                    cause: UnknownCause::ConditionalAbortsOnOneArm,
                });
                Ok(vec![FeasiblePath {
                    facts: walked,
                    terminal,
                }])
            }
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

    /// Decides `terminal`, whose reads resolved to `operands`.
    fn terminal(
        &self,
        terminal: &Terminal,
        at: NodePath,
        operands: &[Operand],
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        match terminal {
            Terminal::Return(_) | Terminal::Abort(_) => Ok(vec![FeasiblePath {
                facts,
                terminal: at,
            }]),
            Terminal::Branch {
                condition,
                when_present,
                when_absent,
            } => {
                let arms = [&**when_present, &**when_absent];
                match condition {
                    Condition::RowPresent(slot) => self.branch(*slot, arms, at, facts),
                    Condition::Compare { .. } | Condition::Contains { .. } => {
                        self.comparison_branch(operands, arms, at, facts)
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
        operands: &[Operand],
        [when_holds, when_fails]: [&Block; 2],
        terminal: NodePath,
        facts: PathFacts,
    ) -> Result<Vec<FeasiblePath>, ClassifyError> {
        let data_selected = operands.iter().any(|operand| !operand.uses().is_empty());
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
            Effect::Insert {
                table,
                key,
                columns,
            } => {
                self.keyed_access(facts, AccessRole::InsertKey, table, key, &node)?;
                self.written_columns(facts, columns, &node)
            }
            Effect::Update {
                table,
                key,
                columns,
            } => {
                let role = AccessRole::Lookup(table.clone());
                self.keyed_access(facts, role, table, key, &node)?;
                self.written_columns(facts, columns, &node)
            }
            Effect::Delete { table, key } => {
                let role = AccessRole::Lookup(table.clone());
                self.keyed_access(facts, role, table, key, &node)?;
                Ok(())
            }
            Effect::ReadRange {
                table,
                prefix,
                interval,
                bound,
                into,
            } => {
                let range = PrimaryRangeRead {
                    table,
                    prefix,
                    interval: interval.as_ref(),
                    bound,
                };
                self.read_primary_range(range, *into, node, facts)
            }
            Effect::ReadIndexRange {
                index,
                start,
                end,
                bound,
                into,
            } => {
                let range = IndexRangeRead {
                    index,
                    start,
                    end,
                    bound,
                };
                self.read_index_range(range, *into, node, facts)
            }
            Effect::Iterate { source, row, body } => self.iterate(*source, *row, body, node, facts),
        }
    }

    /// Walks the body once for the rows `source` bound, as their first
    /// iteration, so a body that carries a slot from one iteration to the next
    /// is refused at that slot's use. A body never runs over a range the path
    /// knows holds no row, so it neither aborts nor refuses there.
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
            Err(Halt::Aborts { .. } | Halt::Refuses { .. })
                if facts.presence_of(&rows) == Some(false) => {}
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

    fn read_primary_range(
        &self,
        range: PrimaryRangeRead<'_>,
        into: Slot,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), Halt> {
        let interval_bounds: Vec<&Expr> = range
            .interval
            .iter()
            .flat_map(|interval| [&interval.from, &interval.to])
            .collect();
        let mut resolved = self.key_components(range.table, range.prefix);
        resolved.extend(
            interval_bounds
                .iter()
                .map(|bound| self.component_at(range.table, range.prefix.len(), bound)),
        );
        resolved.push((range.bound, None));
        self.passes(facts, resolved, &node)?;
        let interval = match interval_bounds[..] {
            [from, to] => Some((
                operand_of(self.schema, facts, from, &node)?,
                operand_of(self.schema, facts, to, &node)?,
            )),
            _ => None,
        };
        let bounds = PrimaryBounds {
            prefix: self.operands_of(facts, range.prefix, &node)?,
            interval,
            bound: operand_of(self.schema, facts, range.bound, &node)?,
        };
        let depends_on = track_access(
            facts,
            self.schema,
            &AccessRole::PrimaryRangeBound,
            bounds.all(),
            &node,
        );
        let kind = BindingKind::PrimaryRange {
            table: range.table.clone(),
            prefix: bounds.prefix.clone(),
        };
        let target = Target::PrimaryRange {
            table: range.table.clone(),
            bounds,
        };
        facts.record_access(&node, target);
        bind(facts, into, kind, node, depends_on);
        Ok(())
    }

    fn read_index_range(
        &self,
        range: IndexRangeRead<'_>,
        into: Slot,
        node: NodePath,
        facts: &mut PathFacts,
    ) -> Result<(), Halt> {
        let resolved = unkeyed(range.start.iter().chain(range.end).chain([range.bound]));
        self.passes(facts, resolved, &node)?;
        let bounds = RangeBounds {
            start: self.operands_of(facts, range.start, &node)?,
            end: self.operands_of(facts, range.end, &node)?,
            bound: operand_of(self.schema, facts, range.bound, &node)?,
        };
        let kind = self.index_binding(range.index);
        let depends_on = track_access(
            facts,
            self.schema,
            &AccessRole::IndexBound,
            bounds.all(),
            &node,
        );
        if let BindingKind::UndeclaredIndexRange(undeclared) = &kind {
            facts.block(&node, UnknownCause::UndeclaredIndex(undeclared.clone()));
        }
        let target = Target::IndexRange {
            index: range.index.clone(),
            bounds,
        };
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
        self.passes(facts, self.key_components(table, key), node)?;
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

    /// Each component of `key` with the key column of `table` it fills.
    fn key_components<'e>(&self, table: &TableName, key: &'e [Expr]) -> Vec<Resolved<'e>>
    where
        'a: 'e,
    {
        key.iter()
            .enumerate()
            .map(|(position, component)| self.component_at(table, position, component))
            .collect()
    }

    /// `component` with the key column of `table` at `position`, which it
    /// fills.
    fn component_at<'e>(
        &self,
        table: &TableName,
        position: usize,
        component: &'e Expr,
    ) -> Resolved<'e>
    where
        'a: 'e,
    {
        let declared = self.schema.table(table);
        let key_column =
            declared.and_then(|declared| declared.column(declared.key_columns().get(position)?));
        (component, key_column)
    }

    /// The evaluator logs a write before it resolves the written columns, so a
    /// path aborting at one keeps the write's access.
    fn written_columns(
        &self,
        facts: &PathFacts,
        columns: &BTreeMap<ColumnName, Expr>,
        node: &NodePath,
    ) -> Result<(), Halt> {
        self.passes(facts, unkeyed(columns.values()), node)?;
        self.operands_of(facts, columns.values(), node)?;
        Ok(())
    }

    fn terminal_operands(
        &self,
        facts: &PathFacts,
        terminal: &Terminal,
        at: &NodePath,
    ) -> Result<Vec<Operand>, Halt> {
        let evaluation = Evaluation {
            schema: self.schema,
            facts,
            at,
        };
        Self::halt_unless_past(evaluation.terminal_aborts(terminal)?, at)?;
        self.operands_of(facts, read_by(terminal), at)
    }

    /// Whether the evaluator gets past resolving `resolved` at `at`. A
    /// certain abort ends the path there; one the data decides refuses it
    /// (`KILN-E1-39`). Every place the evaluator reads a slot passes here
    /// before the walk asks what it read.
    fn passes<'e>(
        &self,
        facts: &PathFacts,
        resolved: impl IntoIterator<Item = Resolved<'e>>,
        at: &NodePath,
    ) -> Result<(), Halt> {
        let evaluation = Evaluation {
            schema: self.schema,
            facts,
            at,
        };
        Self::halt_unless_past(evaluation.aborts(resolved)?, at)
    }

    fn halt_unless_past(aborts: Aborts, at: &NodePath) -> Result<(), Halt> {
        match aborts {
            Aborts::Never => Ok(()),
            Aborts::Always => Err(Halt::aborting_at(at)),
            Aborts::OnOneArm => Err(Halt::Refuses { at: at.clone() }),
        }
    }

    /// What `expressions` are made of, which a place asks once it `passes`.
    fn operands_of<'e>(
        &self,
        facts: &PathFacts,
        expressions: impl IntoIterator<Item = &'e Expr>,
        at: &NodePath,
    ) -> Result<Vec<Operand>, Halt> {
        expressions
            .into_iter()
            .map(|expression| Ok(operand_of(self.schema, facts, expression, at)?))
            .collect()
    }
}

/// Expressions that fill no key column.
fn unkeyed<'e>(expressions: impl IntoIterator<Item = &'e Expr>) -> Vec<Resolved<'e>> {
    expressions
        .into_iter()
        .map(|expression| (expression, None))
        .collect()
}

/// What `terminal` reads before it returns or branches.
fn read_by(terminal: &Terminal) -> Vec<&Expr> {
    match terminal {
        Terminal::Return(response) => vec![response],
        Terminal::Branch { condition, .. } => condition.operands(),
        Terminal::Abort(_) => Vec::new(),
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
