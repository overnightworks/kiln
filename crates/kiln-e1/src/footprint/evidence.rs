//! The IR node paths that justify a label, over every feasible branch
//! (`inputs.md` §5, Q1 and Q2.4).

use super::facts::{BindingKind, ResolutionRole};
use super::node_path::NodePath;
use super::walk::FeasiblePath;
use crate::classified::ProvenCategory;

/// Whether an index read stops at the index, feeds another index's bounds, or
/// goes on to access rows (`inputs.md` §5, Q2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexReadKind {
    IndexOnly,
    FeedsIndexBounds,
    FollowedByRowAccesses,
}

/// A range read over a declared index on a feasible path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexRead {
    pub node: String,
    pub kind: IndexReadKind,
}

/// The nodes of `nodes` without repeats, in first-seen order.
pub fn distinct<'a>(nodes: impl IntoIterator<Item = &'a NodePath>) -> Vec<NodePath> {
    let mut distinct: Vec<NodePath> = Vec::new();
    for node in nodes {
        if !distinct.contains(node) {
            distinct.push(node.clone());
        }
    }
    distinct
}

/// Every access of every feasible path, and for a static function the
/// terminal of every path too.
pub fn proven_evidence(paths: &[FeasiblePath], category: ProvenCategory) -> Vec<NodePath> {
    let accesses = paths
        .iter()
        .flat_map(|path| path.facts.accesses.iter().map(|access| &access.node));
    let terminals = paths
        .iter()
        .map(|path| &path.terminal)
        .filter(|_| category == ProvenCategory::Static);
    distinct(accesses.chain(terminals))
}

/// Every index range read of every feasible path; a read that goes on to row
/// accesses on any path is reported as such.
pub fn index_reads(paths: &[FeasiblePath]) -> Vec<IndexRead> {
    let mut reads: Vec<IndexRead> = Vec::new();
    for path in paths {
        for (node, kind) in path_index_reads(path) {
            match reads.iter_mut().find(|read| read.node == node) {
                Some(known) => known.kind = known.kind.max(kind),
                None => reads.push(IndexRead { node, kind }),
            }
        }
    }
    reads
}

fn path_index_reads(path: &FeasiblePath) -> Vec<(String, IndexReadKind)> {
    path.facts
        .bindings()
        .filter(|binding| matches!(binding.kind, BindingKind::IndexRange { .. }))
        .map(|binding| {
            let roles = path
                .facts
                .resolutions
                .iter()
                .filter(|resolution| resolution.read.node == binding.node)
                .flat_map(|resolution| &resolution.roles);
            let kind = roles
                .filter_map(|role| match role {
                    ResolutionRole::BaseRows => Some(IndexReadKind::FollowedByRowAccesses),
                    ResolutionRole::IndexBounds => Some(IndexReadKind::FeedsIndexBounds),
                    ResolutionRole::Branch => None,
                })
                .max()
                .unwrap_or(IndexReadKind::IndexOnly);
            (binding.node.to_string(), kind)
        })
        .collect()
}
