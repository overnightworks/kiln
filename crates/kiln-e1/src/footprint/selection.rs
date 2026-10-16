//! Data-selected branches (`inputs.md` §5, Q1 and Q3, amendment "which targets
//! versus whether"): a branch decided by a read, or by a comparison over a read
//! result, whose outcome changes *which* targets the function accesses. A
//! comparison over arguments and literals alone selects by argument and never
//! appears here. Such a read determines targets without the identifiable index
//! or foreign key Q3 asks for, so the function is unknown.
//!
//! A selector that only decides *whether* an access happens leaves the target
//! set a function of the arguments: when one path's targets include the
//! other's, nothing is selected and each path classifies on its own. Only
//! incomparable target sets, each with a target the other lacks, are *which*.
//!
//! The paths are compared pairwise, never by the union of an arm's targets:
//! two paths that agree on every selector they both test except one, and reach
//! incomparable targets, show that this selector changes the targets while the
//! others stay fixed. Argument-selected branches are not selectors, but the
//! paths must agree on them too: paths taking different argument arms are
//! different cases and are not compared.
//!
//! Two targets of two paths are the same when they agree up to one consistent
//! renaming of row occurrences across the whole of both paths: each path's
//! occurrences are numbered in the order they first appear in its targets, and
//! an occurrence of one path stands for the occurrence of the other with the
//! same number. Which occurrence feeds which component of which target is kept,
//! so two arms that each access every row of a range by its full key reach the
//! same targets, while arms that pair the components of two rows differently
//! do not. Paths whose occurrences first appear in a different order count as
//! reaching different targets, even when the sets agree: the comparison errs
//! towards unknown.

use super::evidence::distinct;
use super::node_path::NodePath;
use super::operand::{Operand, RowOrigin};
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
    if !reach_incomparable_targets(first, second) {
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

fn reach_incomparable_targets(first: &FeasiblePath, second: &FeasiblePath) -> bool {
    reaches_target_missing_from(first, second) && reaches_target_missing_from(second, first)
}

fn reaches_target_missing_from(path: &FeasiblePath, other: &FeasiblePath) -> bool {
    let numbered = occurrences_in_order(path);
    let others = occurrences_in_order(other);
    let as_in_other = |origin: &RowOrigin| {
        let number = numbered.iter().position(|own| *own == origin)?;
        others.get(number).map(|&counterpart| counterpart.clone())
    };
    path.facts.accesses.iter().any(|access| {
        access
            .target
            .renamed(&as_in_other)
            .is_none_or(|renamed| other.facts.accesses.iter().all(|own| own.target != renamed))
    })
}

/// The path's row occurrences, each once, in the order its targets name them.
fn occurrences_in_order(path: &FeasiblePath) -> Vec<&RowOrigin> {
    let mut occurrences: Vec<&RowOrigin> = Vec::new();
    let named = path
        .facts
        .accesses
        .iter()
        .flat_map(|access| access.target.operands())
        .flat_map(Operand::origins);
    for origin in named {
        if !occurrences.contains(&origin) {
            occurrences.push(origin);
        }
    }
    occurrences
}
