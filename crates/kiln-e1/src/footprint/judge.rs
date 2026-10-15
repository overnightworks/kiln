//! Turns one feasible path's facts into a category or an unknown cause, and
//! picks the most demanding path of a function (Q1, Q3).

use super::cause::UnknownCause;
use super::evidence::distinct;
use super::facts::PathFacts;
use super::node_path::NodePath;
use super::selection::data_selection;
use super::walk::FeasiblePath;

pub enum Judgement {
    Static,
    OneHop,
    /// `witness` are the nodes that make the function unknown.
    Unknown {
        cause: UnknownCause,
        witness: Vec<NodePath>,
    },
}

impl Judgement {
    /// Higher is less favourable.
    fn demand(&self) -> u8 {
        match self {
            Self::Static => 0,
            Self::OneHop => 1,
            Self::Unknown { .. } => 2,
        }
    }
}

/// The judgement of the most demanding path, the first of equally demanding
/// ones; a function no path makes unknown is still unknown when a data-selected
/// branch changes the targets it accesses.
pub fn judge_function(paths: &[FeasiblePath]) -> Judgement {
    let most_demanding = paths
        .iter()
        .map(judge_path)
        .reduce(|best, candidate| {
            if candidate.demand() > best.demand() {
                candidate
            } else {
                best
            }
        })
        .expect("a function body has at least one feasible path");
    if matches!(most_demanding, Judgement::Unknown { .. }) {
        return most_demanding;
    }
    match data_selection(paths) {
        None => most_demanding,
        Some(selection) => Judgement::Unknown {
            cause: UnknownCause::DataSelectedTargets {
                selector: selection.selector.to_string(),
            },
            witness: selection.witness,
        },
    }
}

fn judge_path(path: &FeasiblePath) -> Judgement {
    let facts = &path.facts;
    if let Some(obstacle) = facts.obstacles.first() {
        return Judgement::Unknown {
            cause: obstacle.cause.clone(),
            witness: vec![obstacle.node.clone()],
        };
    }
    match facts.resolutions.len() {
        0 => Judgement::Static,
        1 => Judgement::OneHop,
        count => Judgement::Unknown {
            cause: resolution_cause(facts, count),
            witness: resolution_nodes(facts),
        },
    }
}

fn resolution_nodes(facts: &PathFacts) -> Vec<NodePath> {
    distinct(facts.resolutions.iter().flat_map(|resolution| {
        std::iter::once(&resolution.read.node).chain(&resolution.access_nodes)
    }))
}

fn resolution_cause(facts: &PathFacts, count: usize) -> UnknownCause {
    let resolving: Vec<_> = facts.resolutions.iter().map(|r| &r.read).collect();
    let is_chain = facts.resolutions.iter().any(|resolution| {
        facts.binding_of(&resolution.read).is_some_and(|binding| {
            binding
                .depends_on
                .iter()
                .any(|origin| *origin != resolution.read && resolving.contains(&origin))
        })
    });
    if is_chain {
        UnknownCause::ChainedResolutions { count }
    } else {
        UnknownCause::IndependentResolutions { count }
    }
}
