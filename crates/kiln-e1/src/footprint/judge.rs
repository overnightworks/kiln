//! Turns one feasible path's facts into a category or an unknown cause, and
//! picks the most demanding path of a function (Q1, Q3).

use super::cause::UnknownCause;
use super::facts::{Confinement, PathFacts};
use super::node_path::NodePath;
use super::walk::FeasiblePath;
use crate::classified::ProvenCategory;

pub enum Judgement {
    Proven(ProvenCategory),
    Unknown(UnknownCause),
}

pub struct PathVerdict {
    pub judgement: Judgement,
    pub evidence: Vec<NodePath>,
}

impl PathVerdict {
    /// Higher is less favourable.
    fn demand(&self) -> u8 {
        match self.judgement {
            Judgement::Proven(ProvenCategory::Static) => 0,
            Judgement::Proven(ProvenCategory::OneHop) => 1,
            Judgement::Proven(ProvenCategory::DomainBounded) => 2,
            Judgement::Unknown(_) => 3,
        }
    }
}

/// The verdict of the most demanding path; the first of equally demanding ones.
pub fn most_demanding(paths: &[FeasiblePath]) -> Option<(&FeasiblePath, PathVerdict)> {
    paths
        .iter()
        .map(|path| (path, judge(path)))
        .reduce(|best, candidate| {
            if candidate.1.demand() > best.1.demand() {
                candidate
            } else {
                best
            }
        })
}

fn judge(path: &FeasiblePath) -> PathVerdict {
    let facts = &path.facts;
    if let Some(obstacle) = facts.obstacles.first() {
        return PathVerdict {
            judgement: Judgement::Unknown(obstacle.cause.clone()),
            evidence: vec![obstacle.node.clone()],
        };
    }
    match facts.resolutions.len() {
        0 => {
            let mut evidence = access_nodes(facts);
            evidence.push(path.terminal.clone());
            PathVerdict {
                judgement: Judgement::Proven(ProvenCategory::Static),
                evidence,
            }
        }
        1 => PathVerdict {
            judgement: Judgement::Proven(ProvenCategory::OneHop),
            evidence: resolution_nodes(facts),
        },
        _ if is_confined_to_one_partition(facts) => PathVerdict {
            judgement: Judgement::Proven(ProvenCategory::DomainBounded),
            evidence: access_nodes(facts),
        },
        count => PathVerdict {
            judgement: Judgement::Unknown(unconfined_cause(facts, count)),
            evidence: resolution_nodes(facts),
        },
    }
}

fn access_nodes(facts: &PathFacts) -> Vec<NodePath> {
    facts
        .accesses
        .iter()
        .map(|access| access.node.clone())
        .collect()
}

fn resolution_nodes(facts: &PathFacts) -> Vec<NodePath> {
    let mut nodes: Vec<NodePath> = Vec::new();
    for resolution in &facts.resolutions {
        for node in std::iter::once(&resolution.read_node).chain(&resolution.access_nodes) {
            if !nodes.contains(node) {
                nodes.push(node.clone());
            }
        }
    }
    nodes
}

fn is_confined_to_one_partition(facts: &PathFacts) -> bool {
    let mut confinements = facts.accesses.iter().map(|access| &access.confinement);
    match confinements.next() {
        Some(first @ Confinement::Partition(_)) => confinements.all(|next| next == first),
        _ => false,
    }
}

fn unconfined_cause(facts: &PathFacts, count: usize) -> UnknownCause {
    let resolving: Vec<_> = facts.resolutions.iter().map(|r| r.read).collect();
    let is_chain = facts.resolutions.iter().any(|resolution| {
        facts.binding(resolution.read).is_some_and(|binding| {
            binding
                .depends_on
                .iter()
                .any(|origin| *origin != resolution.read && resolving.contains(origin))
        })
    });
    if is_chain {
        UnknownCause::ChainedResolutions { count }
    } else {
        UnknownCause::IndependentResolutions { count }
    }
}
