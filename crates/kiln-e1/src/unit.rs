use std::fmt;

use serde::{Deserialize, Serialize};

/// The application's `owner_repo` in `corpus-manifest.csv`; for TPC-C units the
/// TPC-C transaction type's name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApplicationId(String);

impl ApplicationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApplicationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The rules of `inputs.md` §3 that make a unit; S1 reports sites and S4
/// excludes candidates, so neither makes a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitRule {
    S2,
    S3,
    S5,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// One enumerated transaction function, as an extractor reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRecord {
    pub application: ApplicationId,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub rule: UnitRule,
    pub parameters: Vec<String>,
    pub absorbed_sites: Vec<SiteLocation>,
}
