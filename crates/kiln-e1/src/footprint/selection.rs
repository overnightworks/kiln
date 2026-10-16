//! Data-selected branches (`inputs.md` §5, Q1 and Q3): a branch decided by a
//! read, or by a comparison over a read result, whose outcome changes which
//! targets the function accesses. A comparison over arguments and literals
//! alone selects by argument and never appears here. Such a read
//! determines targets without the identifiable index or foreign key Q3 asks
//! for, so the function is unknown.
//!
//! The paths are compared pairwise, never by the union of an arm's targets:
//! two paths that agree on every selector they both test except one, and reach
//! different targets, show that this selector changes the targets while the
//! others stay fixed. Argument-selected branches are not selectors, but the
//! paths must agree on them too: paths taking different argument arms are
//! different cases and are not compared.

use super::evidence::distinct;
use super::node_path::NodePath;
use super::walk::FeasiblePath;

/// A selector whose outcome changes the accessed targets, and the nodes that
/// show it.
pub struct DataSelection {
    pub selector: NodePath,
    pub witness: Vec<NodePath>,
}

pub fn data_selection(paths: &[FeasiblePath]) -> Option<DataSelection> {
    paths.iter().find_map(|first| {
        paths
            .iter()
            .find_map(|second| selection_between(first, second))
    })
}

fn selection_between(first: &FeasiblePath, second: &FeasiblePath) -> Option<DataSelection> {
    if !agree_on_arguments(first, second) {
        return None;
    }
    let selector = sole_differing_selector(first, second)?;
    if reach_same_targets(first, second) {
        return None;
    }
    let accesses = first.facts.accesses.iter().chain(&second.facts.accesses);
    let witness = distinct(std::iter::once(&selector).chain(accesses.map(|access| &access.node)));
    Some(DataSelection { selector, witness })
}

fn agree_on_arguments(first: &FeasiblePath, second: &FeasiblePath) -> bool {
    first
        .facts
        .argument_decisions()
        .iter()
        .all(|(branch, outcome)| {
            second
                .facts
                .argument_decisions()
                .iter()
                .all(|(other, other_outcome)| other != branch || other_outcome == outcome)
        })
}

/// The one selector both paths test and decide differently.
fn sole_differing_selector(first: &FeasiblePath, second: &FeasiblePath) -> Option<NodePath> {
    let decided_differently = |(selector, outcome): &&(NodePath, bool)| {
        second
            .facts
            .presence()
            .iter()
            .any(|(other, other_outcome)| other == selector && other_outcome != outcome)
    };
    let mut differing = first.facts.presence().iter().filter(decided_differently);
    let (sole, _) = differing.next()?;
    differing.next().is_none().then(|| sole.clone())
}

fn reach_same_targets(first: &FeasiblePath, second: &FeasiblePath) -> bool {
    let covers = |path: &FeasiblePath, other: &FeasiblePath| {
        other.facts.accesses.iter().all(|access| {
            path.facts
                .accesses
                .iter()
                .any(|own| own.target == access.target)
        })
    };
    covers(first, second) && covers(second, first)
}
