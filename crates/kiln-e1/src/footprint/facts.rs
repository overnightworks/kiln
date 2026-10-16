use std::collections::{BTreeSet, HashMap, HashSet};

use kiln_ir::{IndexName, Slot, TableName};

use super::cause::UnknownCause;
use super::node_path::NodePath;
use super::operand::{Operand, ReadRef, RowOrigin};
use super::ClassifyError;

#[derive(Clone, Debug)]
pub enum BindingKind {
    PointRead { table: TableName, key: Vec<Operand> },
    PrimaryRange { table: TableName },
    IndexRange { table: TableName },
    UndeclaredIndexRange(IndexName),
}

/// A slot bound by a read: what it holds, where, and which earlier reads its
/// own target depends on.
#[derive(Clone, Debug)]
pub struct Binding {
    pub slot: Slot,
    pub kind: BindingKind,
    pub node: NodePath,
    pub depends_on: Vec<ReadRef>,
}

/// The bounds of a range read, in the order the IR gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeBounds {
    pub start: Vec<Operand>,
    pub end: Vec<Operand>,
    pub bound: Operand,
}

impl RangeBounds {
    pub fn all(&self) -> impl Iterator<Item = &Operand> {
        self.start.iter().chain(&self.end).chain([&self.bound])
    }

    fn renamed(&self, rename: &impl Fn(&RowOrigin) -> Option<RowOrigin>) -> Option<Self> {
        Some(Self {
            start: renamed_all(&self.start, rename)?,
            end: renamed_all(&self.end, rename)?,
            bound: self.bound.renamed(rename)?,
        })
    }
}

fn renamed_all(
    operands: &[Operand],
    rename: &impl Fn(&RowOrigin) -> Option<RowOrigin>,
) -> Option<Vec<Operand>> {
    operands
        .iter()
        .map(|operand| operand.renamed(rename))
        .collect()
}

/// What an access touches, independent of whether it reads or writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Row {
        table: TableName,
        key: Vec<Operand>,
    },
    PrimaryRange {
        table: TableName,
        bounds: RangeBounds,
    },
    IndexRange {
        index: IndexName,
        bounds: RangeBounds,
    },
}

impl Target {
    /// The key or bounds that say what the target is, in reading order.
    pub fn operands(&self) -> Vec<&Operand> {
        match self {
            Self::Row { key, .. } => key.iter().collect(),
            Self::PrimaryRange { bounds, .. } | Self::IndexRange { bounds, .. } => {
                bounds.all().collect()
            }
        }
    }

    /// The target with every row occurrence renamed by `rename`, or `None`
    /// when `rename` has no name for one of them.
    pub fn renamed(&self, rename: &impl Fn(&RowOrigin) -> Option<RowOrigin>) -> Option<Self> {
        Some(match self {
            Self::Row { table, key } => Self::Row {
                table: table.clone(),
                key: renamed_all(key, rename)?,
            },
            Self::PrimaryRange { table, bounds } => Self::PrimaryRange {
                table: table.clone(),
                bounds: bounds.renamed(rename)?,
            },
            Self::IndexRange { index, bounds } => Self::IndexRange {
                index: index.clone(),
                bounds: bounds.renamed(rename)?,
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct Access {
    pub node: NodePath,
    pub target: Target,
}

/// Why a read counts as a resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolutionRole {
    /// Its result names rows to access or insert.
    BaseRows,
    /// Its result bounds another index range.
    IndexBounds,
}

/// A read whose result determines the targets of the accesses at `access_nodes`.
#[derive(Clone, Debug)]
pub struct Resolution {
    pub read: ReadRef,
    pub access_nodes: Vec<NodePath>,
    pub roles: BTreeSet<ResolutionRole>,
}

#[derive(Clone, Debug)]
pub struct Obstacle {
    pub node: NodePath,
    pub cause: UnknownCause,
}

/// What a bound slot holds at a point of the path, as the evaluator binds it.
/// A slot holds nothing before a read binds it and after the iteration that
/// bound it ends.
#[derive(Clone, Debug)]
enum SlotContent {
    /// The row or rows `read` bound.
    Read(ReadRef),
    /// The current row of the rows `source` bound, as the iteration at
    /// `iteration` yields it.
    IterationRow {
        source: ReadRef,
        iteration: NodePath,
    },
}

/// Everything learned about one feasible path through a function's body.
#[derive(Clone, Debug, Default)]
pub struct PathFacts {
    bindings: Vec<Binding>,
    slots: HashMap<Slot, SlotContent>,
    released: HashSet<Slot>,
    presence: Vec<(NodePath, bool)>,
    argument_decisions: Vec<(NodePath, bool)>,
    pub accesses: Vec<Access>,
    pub resolutions: Vec<Resolution>,
    pub obstacles: Vec<Obstacle>,
}

impl PathFacts {
    /// Binds `binding.slot` to the read, ending whatever it held before.
    pub fn bind(&mut self, binding: Binding) {
        let read = ReadRef {
            slot: binding.slot,
            node: binding.node.clone(),
        };
        self.slots.insert(binding.slot, SlotContent::Read(read));
        self.released.remove(&binding.slot);
        self.bindings.push(binding);
    }

    /// Binds `row` to the current row of the rows `source` bound, as the
    /// iteration at `iteration` yields it.
    pub fn enter_iteration(&mut self, row: Slot, source: ReadRef, iteration: &NodePath) {
        let content = SlotContent::IterationRow {
            source,
            iteration: iteration.clone(),
        };
        self.slots.insert(row, content);
        self.released.remove(&row);
    }

    /// Empties the slots the evaluator releases when an iteration ends: they
    /// hold an absent row until a read binds them again.
    pub fn end_iteration(&mut self, released: impl IntoIterator<Item = Slot>) {
        for slot in released {
            self.slots.remove(&slot);
            self.released.insert(slot);
        }
    }

    /// Whether an ended iteration released `slot`, so a presence test on it
    /// finds no row and a projection of it aborts.
    pub fn is_released(&self, slot: Slot) -> bool {
        self.released.contains(&slot)
    }

    /// The read that produced `slot`'s rows.
    pub fn read_of(&self, slot: Slot, at: &NodePath) -> Result<ReadRef, ClassifyError> {
        match self.content_of(slot, at)? {
            SlotContent::Read(read) | SlotContent::IterationRow { source: read, .. } => {
                Ok(read.clone())
            }
        }
    }

    /// Which binding occurrence `slot` stands for: the iteration that yielded
    /// the row, or the read that bound the slot itself. A slot name may be
    /// bound again; the occurrence is what tells the rows apart.
    pub fn row_origin_of(&self, slot: Slot, at: &NodePath) -> Result<RowOrigin, ClassifyError> {
        Ok(RowOrigin::new(match self.content_of(slot, at)? {
            SlotContent::Read(read) => read.node.clone(),
            SlotContent::IterationRow { iteration, .. } => iteration.clone(),
        }))
    }

    fn content_of(&self, slot: Slot, at: &NodePath) -> Result<&SlotContent, ClassifyError> {
        self.slots
            .get(&slot)
            .ok_or_else(|| ClassifyError::UnboundSlot {
                slot,
                at: at.to_string(),
            })
    }

    pub fn binding_of(&self, read: &ReadRef) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|binding| binding.node == read.node)
    }

    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.bindings.iter()
    }

    /// Whether an enclosing test already settled if `read` found a row.
    pub fn presence_of(&self, read: &ReadRef) -> Option<bool> {
        self.presence
            .iter()
            .find(|(node, _)| *node == read.node)
            .map(|(_, present)| *present)
    }

    /// The reads whose presence the path settled, with the outcome it took.
    pub fn presence(&self) -> &[(NodePath, bool)] {
        &self.presence
    }

    pub fn assuming(&self, read: &ReadRef, present: bool) -> Self {
        self.deciding(&read.node, present)
    }

    /// The path after the selector at `node` took `outcome`.
    pub fn deciding(&self, node: &NodePath, outcome: bool) -> Self {
        let mut facts = self.clone();
        facts.presence.push((node.clone(), outcome));
        facts
    }

    /// The argument-selected branches the path took, with the outcome. They
    /// constrain which paths are comparable; they are never data selectors.
    pub fn argument_decisions(&self) -> &[(NodePath, bool)] {
        &self.argument_decisions
    }

    /// The path after the argument-selected branch at `node` took `outcome`.
    pub fn deciding_by_argument(&self, node: &NodePath, outcome: bool) -> Self {
        let mut facts = self.clone();
        facts.argument_decisions.push((node.clone(), outcome));
        facts
    }

    pub fn record_access(&mut self, node: &NodePath, target: Target) {
        self.accesses.push(Access {
            node: node.clone(),
            target,
        });
    }

    pub fn block(&mut self, node: &NodePath, cause: UnknownCause) {
        self.obstacles.push(Obstacle {
            node: node.clone(),
            cause,
        });
    }

    /// Records that `read` determines the targets at `nodes`.
    pub fn resolve(&mut self, read: &ReadRef, role: ResolutionRole, nodes: &[NodePath]) {
        let position = self.resolutions.iter().position(|r| r.read == *read);
        let resolution = match position {
            Some(position) => &mut self.resolutions[position],
            None => {
                self.resolutions.push(Resolution {
                    read: read.clone(),
                    access_nodes: Vec::new(),
                    roles: BTreeSet::new(),
                });
                self.resolutions.last_mut().expect("just pushed")
            }
        };
        resolution.roles.insert(role);
        for node in nodes {
            if !resolution.access_nodes.contains(node) {
                resolution.access_nodes.push(node.clone());
            }
        }
    }
}
