use kiln_ir::{Expr, ParameterName, Slot, TableName};

use super::cause::UnknownCause;
use super::node_path::NodePath;
use super::ClassifyError;

#[derive(Clone, Debug)]
pub enum BindingKind {
    PointRead {
        table: TableName,
        key: Expr,
    },
    PrimaryRange {
        table: TableName,
    },
    IndexRange {
        table: TableName,
        partition: Option<ParameterName>,
    },
    UndeclaredIndexRange(kiln_ir::IndexName),
}

/// A slot bound by a read: what it holds, where, and which earlier reads its
/// own target depends on.
#[derive(Clone, Debug)]
pub struct Binding {
    pub kind: BindingKind,
    pub node: NodePath,
    pub depends_on: Vec<Slot>,
}

/// Which partition an access is confined to, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confinement {
    Partition(ParameterName),
    Outside,
}

#[derive(Clone, Debug)]
pub struct Access {
    pub node: NodePath,
    pub confinement: Confinement,
}

/// A read whose result determines the target of the accesses at `access_nodes`.
#[derive(Clone, Debug)]
pub struct Resolution {
    pub read: Slot,
    pub read_node: NodePath,
    pub access_nodes: Vec<NodePath>,
}

#[derive(Clone, Debug)]
pub struct Obstacle {
    pub node: NodePath,
    pub cause: UnknownCause,
}

/// Everything learned about one feasible path through a function's body.
#[derive(Clone, Debug, Default)]
pub struct PathFacts {
    bindings: Vec<(Slot, Binding)>,
    aliases: Vec<(Slot, Slot)>,
    pub accesses: Vec<Access>,
    pub resolutions: Vec<Resolution>,
    pub obstacles: Vec<Obstacle>,
}

impl PathFacts {
    pub fn bind(&mut self, slot: Slot, binding: Binding) {
        self.bindings.push((slot, binding));
    }

    /// `row` is the current row of the sequence `source` bound.
    pub fn alias(&mut self, row: Slot, source: Slot, at: &NodePath) -> Result<(), ClassifyError> {
        let origin = self.origin(source, at)?;
        self.aliases.push((row, origin));
        Ok(())
    }

    /// The slot whose read produced `slot`'s rows.
    pub fn origin(&self, slot: Slot, at: &NodePath) -> Result<Slot, ClassifyError> {
        let origin = self
            .aliases
            .iter()
            .rev()
            .find(|(row, _)| *row == slot)
            .map_or(slot, |(_, source)| *source);
        self.binding(origin)
            .map(|_| origin)
            .ok_or_else(|| ClassifyError::UnboundSlot {
                slot,
                at: at.to_string(),
            })
    }

    pub fn binding(&self, origin: Slot) -> Option<&Binding> {
        self.bindings
            .iter()
            .rev()
            .find(|(slot, _)| *slot == origin)
            .map(|(_, binding)| binding)
    }

    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.bindings.iter().map(|(_, binding)| binding)
    }

    pub fn record_access(&mut self, node: &NodePath, confinement: Confinement) {
        self.accesses.push(Access {
            node: node.clone(),
            confinement,
        });
    }

    pub fn block(&mut self, node: &NodePath, cause: UnknownCause) {
        self.obstacles.push(Obstacle {
            node: node.clone(),
            cause,
        });
    }

    /// Records that the read bound to `origin` at `read_node` determines the
    /// target at `node`.
    pub fn resolve(&mut self, origin: Slot, read_node: &NodePath, node: &NodePath) {
        let position = self.resolutions.iter().position(|r| r.read == origin);
        let resolution = match position {
            Some(position) => &mut self.resolutions[position],
            None => {
                self.resolutions.push(Resolution {
                    read: origin,
                    read_node: read_node.clone(),
                    access_nodes: Vec::new(),
                });
                self.resolutions.last_mut().expect("just pushed")
            }
        };
        if !resolution.access_nodes.contains(node) {
            resolution.access_nodes.push(node.clone());
        }
    }
}
