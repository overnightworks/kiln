//! The footprint classifier: a pure function from a `kiln-ir` function to the
//! frozen category of `docs/experiments.md` ("Footprint categories") with
//! IR node paths as evidence, applying `inputs.md` §5 (Q1-Q4). The observable
//! behaviour is owned by `specs/kiln-e1.spec.md` (`KILN-E1-18` to `KILN-E1-29`).

mod carried;
mod cause;
mod dependency;
mod evidence;
mod facts;
mod judge;
mod node_path;
mod operand;
mod selection;
mod walk;

use std::fmt;

use kiln_ir::{FunctionName, Program, ProgramError, Slot};

pub use cause::UnknownCause;
pub use evidence::{IndexRead, IndexReadKind};

use crate::classified::{Classification, ContractViolation, Evidence, ProvenCategory, Reason};
use evidence::{index_reads, proven_evidence};
use judge::{judge_function, Judgement};
use node_path::NodePath;
use walk::feasible_paths;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintClassification {
    pub classification: Classification,
    pub index_reads: Vec<IndexRead>,
}

/// Why a function could not be classified at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassifyError {
    /// The program is one `kiln-ir` rejects before any call (D8, D9), so its
    /// keys cannot be read component by component.
    InvalidProgram(ProgramError),
    UnknownFunction(FunctionName),
    /// A place that reads a slot names one no earlier read bound, or an
    /// iteration ranges over one that an ended iteration released.
    UnboundSlot {
        slot: Slot,
        at: String,
    },
    Evidence(ContractViolation),
}

impl fmt::Display for ClassifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProgram(error) => write!(formatter, "the program is invalid: {error:?}"),
            Self::UnknownFunction(name) => {
                write!(formatter, "the program declares no function {}", name.0)
            }
            Self::UnboundSlot { slot, at } => {
                write!(
                    formatter,
                    "slot {} is read at {at} where no read binds it",
                    slot.0
                )
            }
            Self::Evidence(violation) => write!(formatter, "evidence rejected: {violation}"),
        }
    }
}

impl std::error::Error for ClassifyError {}

impl From<ProgramError> for ClassifyError {
    fn from(error: ProgramError) -> Self {
        Self::InvalidProgram(error)
    }
}

impl From<ContractViolation> for ClassifyError {
    fn from(violation: ContractViolation) -> Self {
        Self::Evidence(violation)
    }
}

pub fn classify_function(
    program: &Program,
    name: &FunctionName,
) -> Result<FootprintClassification, ClassifyError> {
    program.validate()?;
    let function = program
        .functions
        .iter()
        .find(|function| &function.name == name)
        .ok_or_else(|| ClassifyError::UnknownFunction(name.clone()))?;
    let paths = feasible_paths(&program.schema, function)?;
    let proven = |category| -> Result<Classification, ContractViolation> {
        Ok(Classification::Proven {
            category,
            evidence: evidence_of(proven_evidence(&paths, category))?,
        })
    };
    let classification = match judge_function(&paths) {
        Judgement::Static => proven(ProvenCategory::Static)?,
        Judgement::OneHop => proven(ProvenCategory::OneHop)?,
        Judgement::Unknown { cause, witness } => Classification::Unknown {
            reason: Reason::new(cause.to_string())?,
            evidence: Some(evidence_of(witness)?),
        },
    };
    Ok(FootprintClassification {
        classification,
        index_reads: index_reads(&paths),
    })
}

fn evidence_of(nodes: Vec<NodePath>) -> Result<Evidence, ContractViolation> {
    Evidence::new(nodes.iter().map(NodePath::to_string).collect())
}
