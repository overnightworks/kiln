use std::collections::BTreeSet;

use kiln_ir::{IndexName, Slot, TableName};

use super::cause::UnknownCause;
use super::node_path::NodePath;
use super::operand::{Operand, ReadRef};
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

/// Everything learned about one feasible path through a function's body.
#[derive(Clone, Debug, Default)]
pub struct PathFacts {
    bindings: Vec<Binding>,
    aliases: Vec<(Slot, Slot)>,
    presence: Vec<(NodePath, bool)>,
    argument_decisions: Vec<(NodePath, bool)>,
    pub accesses: Vec<Access>,
    pub resolutions: Vec<Resolution>,
    pub obstacles: Vec<Obstacle>,
}

impl PathFacts {
    pub fn bind(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    /// `row` is the current row of the sequence `source` bound.
    pub fn alias(&mut self, row: Slot, source: Slot, at: &NodePath) -> Result<(), ClassifyError> {
        let origin = self.read_of(source, at)?.slot;
        self.aliases.push((row, origin));
        Ok(())
    }

    /// The read that produced `slot`'s rows.
    pub fn read_of(&self, slot: Slot, at: &NodePath) -> Result<ReadRef, ClassifyError> {
        let origin = self
            .aliases
            .iter()
            .rev()
            .find(|(row, _)| *row == slot)
            .map_or(slot, |(_, source)| *source);
        self.bindings
            .iter()
            .rev()
            .find(|binding| binding.slot == origin)
            .map(|binding| ReadRef {
                slot: origin,
                node: binding.node.clone(),
            })
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
