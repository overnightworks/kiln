//! The footprint classifier: a pure function from a `kiln-ir` function to the
//! frozen category of `docs/experiments.md` ("Footprint categories") with
//! IR node paths as evidence, applying `inputs.md` §5 (Q1-Q4). The observable
//! behaviour is owned by `specs/kiln-e1.spec.md` (`KILN-E1-18` to `KILN-E1-27`).

mod cause;
mod dependency;
mod facts;
mod judge;
mod node_path;
mod walk;

use std::fmt;

use kiln_ir::{FunctionName, Program, Slot};

pub use cause::UnknownCause;

use crate::classified::{Classification, ContractViolation, Evidence, Reason};
use facts::BindingKind;
use judge::{most_demanding, Judgement, PathVerdict};
use node_path::NodePath;
use walk::{feasible_paths, FeasiblePath};

/// Whether an index read stops at the index or goes on to access rows
/// (`inputs.md` §5, Q2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexReadKind {
    IndexOnly,
    FollowedByRowAccesses,
}

/// A range read over a declared index on the most demanding path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexRead {
    pub node: String,
    pub kind: IndexReadKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintClassification {
    pub classification: Classification,
    pub index_reads: Vec<IndexRead>,
}

/// Why a function could not be classified at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassifyError {
    UnknownFunction(FunctionName),
    /// A key or bound refers to a slot no earlier read bound.
    UnboundSlot {
        slot: Slot,
        at: String,
    },
    Evidence(ContractViolation),
}

impl fmt::Display for ClassifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFunction(name) => {
                write!(formatter, "the program declares no function {}", name.0)
            }
            Self::UnboundSlot { slot, at } => {
                write!(
                    formatter,
                    "slot {} is read at {at} before any read binds it",
                    slot.0
                )
            }
            Self::Evidence(violation) => write!(formatter, "evidence rejected: {violation}"),
        }
    }
}

impl std::error::Error for ClassifyError {}

impl From<ContractViolation> for ClassifyError {
    fn from(violation: ContractViolation) -> Self {
        Self::Evidence(violation)
    }
}

pub fn classify_function(
    program: &Program,
    name: &FunctionName,
) -> Result<FootprintClassification, ClassifyError> {
    let function = program
        .functions
        .iter()
        .find(|function| &function.name == name)
        .ok_or_else(|| ClassifyError::UnknownFunction(name.clone()))?;
    let paths = feasible_paths(&program.schema, function)?;
    let (path, verdict) =
        most_demanding(&paths).expect("a function body has at least one feasible path");
    Ok(FootprintClassification {
        index_reads: index_reads(path),
        classification: classification_of(verdict)?,
    })
}

fn classification_of(verdict: PathVerdict) -> Result<Classification, ClassifyError> {
    let evidence = Evidence::new(verdict.evidence.iter().map(NodePath::to_string).collect())?;
    Ok(match verdict.judgement {
        Judgement::Proven(category) => Classification::Proven { category, evidence },
        Judgement::Unknown(cause) => Classification::Unknown {
            reason: Reason::new(cause.to_string())?,
            evidence: Some(evidence),
        },
    })
}

fn index_reads(path: &FeasiblePath) -> Vec<IndexRead> {
    path.facts
        .bindings()
        .filter(|binding| matches!(binding.kind, BindingKind::IndexRange { .. }))
        .map(|binding| {
            let resolves_rows = path
                .facts
                .resolutions
                .iter()
                .any(|resolution| resolution.read_node == binding.node);
            IndexRead {
                node: binding.node.to_string(),
                kind: if resolves_rows {
                    IndexReadKind::FollowedByRowAccesses
                } else {
                    IndexReadKind::IndexOnly
                },
            }
        })
        .collect()
}
