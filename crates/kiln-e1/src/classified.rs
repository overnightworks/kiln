use std::fmt;

use serde::{Deserialize, Serialize};

use crate::unit::UnitRecord;

/// A way a classified unit breaks the evidence contract of `docs/experiments.md`
/// ("Audits", evidence requirement) or `inputs.md` §3 (S4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractViolation {
    MissingCategory,
    MissingEvidence,
    BlankEvidencePath,
    MissingReason,
    ReasonOnProvenUnit,
    EvidenceOnUnknownUnit,
    ExclusionWithCategory,
    ExclusionWithOtherFields,
    IncompleteExclusion(&'static str),
    UnresolvedReceiver,
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCategory => {
                formatter.write_str("neither a category nor an S4 exclusion is given")
            }
            Self::MissingEvidence => {
                formatter.write_str("a static, one-hop or domain-bounded label needs evidence")
            }
            Self::BlankEvidencePath => formatter.write_str("an evidence IR node path is blank"),
            Self::MissingReason => formatter.write_str("an unknown label needs its reason"),
            Self::ReasonOnProvenUnit => {
                formatter.write_str("a refusal reason contradicts a static, one-hop or domain-bounded label")
            }
            Self::EvidenceOnUnknownUnit => {
                formatter.write_str("evidence contradicts an unknown label")
            }
            Self::ExclusionWithCategory => {
                formatter.write_str("an S4 exclusion contradicts a category")
            }
            Self::ExclusionWithOtherFields => {
                formatter.write_str("an S4 exclusion contradicts evidence or a refusal reason")
            }
            Self::IncompleteExclusion(field) => {
                write!(formatter, "S4 exclusion evidence lacks `{field}`")
            }
            Self::UnresolvedReceiver => formatter.write_str(
                "S4 exclusion evidence names an unresolved (`Any`) receiver, which is no affirmative evidence",
            ),
        }
    }
}

impl std::error::Error for ContractViolation {}

fn require_text(value: String, field: &'static str) -> Result<String, ContractViolation> {
    if value.trim().is_empty() {
        Err(ContractViolation::IncompleteExclusion(field))
    } else {
        Ok(value)
    }
}

/// IR node paths that justify a category; at least one, none blank.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Evidence(Vec<String>);

impl Evidence {
    pub fn new(paths: Vec<String>) -> Result<Self, ContractViolation> {
        if paths.is_empty() {
            return Err(ContractViolation::MissingEvidence);
        }
        if paths.iter().any(|path| path.trim().is_empty()) {
            return Err(ContractViolation::BlankEvidencePath);
        }
        Ok(Self(paths))
    }
}

/// Why a unit is unknown, for example the front end's declared refusal reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Reason(String);

impl Reason {
    pub fn new(text: impl Into<String>) -> Result<Self, ContractViolation> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(ContractViolation::MissingReason);
        }
        Ok(Self(text))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawS4Exclusion {
    receiver_type: String,
    called_method: String,
    no_orm_path_reason: String,
}

/// The affirmative evidence `inputs.md` §3 (S4) demands before a candidate
/// leaves the denominator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawS4Exclusion")]
pub struct S4Exclusion {
    receiver_type: String,
    called_method: String,
    no_orm_path_reason: String,
}

impl S4Exclusion {
    pub fn new(
        receiver_type: String,
        called_method: String,
        no_orm_path_reason: String,
    ) -> Result<Self, ContractViolation> {
        let receiver_type = require_text(receiver_type, "receiver_type")?;
        if matches!(receiver_type.trim(), "Any" | "typing.Any") {
            return Err(ContractViolation::UnresolvedReceiver);
        }
        Ok(Self {
            receiver_type,
            called_method: require_text(called_method, "called_method")?,
            no_orm_path_reason: require_text(no_orm_path_reason, "no_orm_path_reason")?,
        })
    }
}

impl TryFrom<RawS4Exclusion> for S4Exclusion {
    type Error = ContractViolation;

    fn try_from(raw: RawS4Exclusion) -> Result<Self, Self::Error> {
        Self::new(raw.receiver_type, raw.called_method, raw.no_orm_path_reason)
    }
}

/// A footprint category with checkable evidence (`docs/experiments.md`,
/// "Footprint categories").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenCategory {
    Static,
    OneHop,
    DomainBounded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Classification {
    Proven {
        category: ProvenCategory,
        evidence: Evidence,
    },
    Unknown(Reason),
    ExcludedNonDatabase(S4Exclusion),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WireCategory {
    Static,
    OneHop,
    DomainBounded,
    Unknown,
}

/// The JSONL line shape; [`ClassifiedUnit`] is what survives validating it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireClassifiedUnit {
    unit: UnitRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<WireCategory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refusal_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s4_exclusion: Option<S4Exclusion>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WireClassifiedUnit", into = "WireClassifiedUnit")]
pub struct ClassifiedUnit {
    unit: UnitRecord,
    classification: Classification,
}

impl ClassifiedUnit {
    pub fn new(unit: UnitRecord, classification: Classification) -> Self {
        Self {
            unit,
            classification,
        }
    }

    pub fn unit(&self) -> &UnitRecord {
        &self.unit
    }

    pub fn classification(&self) -> &Classification {
        &self.classification
    }
}

impl TryFrom<WireClassifiedUnit> for ClassifiedUnit {
    type Error = ContractViolation;

    fn try_from(wire: WireClassifiedUnit) -> Result<Self, Self::Error> {
        let classification = match (wire.category, wire.s4_exclusion) {
            (Some(_), Some(_)) => return Err(ContractViolation::ExclusionWithCategory),
            (None, None) => return Err(ContractViolation::MissingCategory),
            (None, Some(exclusion)) => {
                if !wire.evidence.is_empty() || wire.refusal_reason.is_some() {
                    return Err(ContractViolation::ExclusionWithOtherFields);
                }
                Classification::ExcludedNonDatabase(exclusion)
            }
            (Some(label), None) => match label.proven() {
                Some(category) => {
                    if wire.refusal_reason.is_some() {
                        return Err(ContractViolation::ReasonOnProvenUnit);
                    }
                    Classification::Proven {
                        category,
                        evidence: Evidence::new(wire.evidence)?,
                    }
                }
                None => {
                    if !wire.evidence.is_empty() {
                        return Err(ContractViolation::EvidenceOnUnknownUnit);
                    }
                    let reason = wire
                        .refusal_reason
                        .ok_or(ContractViolation::MissingReason)?;
                    Classification::Unknown(Reason::new(reason)?)
                }
            },
        };
        Ok(Self::new(wire.unit, classification))
    }
}

impl WireCategory {
    fn proven(self) -> Option<ProvenCategory> {
        match self {
            Self::Static => Some(ProvenCategory::Static),
            Self::OneHop => Some(ProvenCategory::OneHop),
            Self::DomainBounded => Some(ProvenCategory::DomainBounded),
            Self::Unknown => None,
        }
    }
}

impl From<ClassifiedUnit> for WireClassifiedUnit {
    fn from(classified: ClassifiedUnit) -> Self {
        let mut wire = Self {
            unit: classified.unit,
            category: None,
            evidence: Vec::new(),
            refusal_reason: None,
            s4_exclusion: None,
        };
        match classified.classification {
            Classification::Proven { category, evidence } => {
                wire.category = Some(match category {
                    ProvenCategory::Static => WireCategory::Static,
                    ProvenCategory::OneHop => WireCategory::OneHop,
                    ProvenCategory::DomainBounded => WireCategory::DomainBounded,
                });
                wire.evidence = evidence.0;
            }
            Classification::Unknown(reason) => {
                wire.category = Some(WireCategory::Unknown);
                wire.refusal_reason = Some(reason.0);
            }
            Classification::ExcludedNonDatabase(exclusion) => wire.s4_exclusion = Some(exclusion),
        }
        wire
    }
}
