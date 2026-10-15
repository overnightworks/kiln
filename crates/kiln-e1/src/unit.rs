use std::fmt;

use serde::{Deserialize, Serialize};

use crate::tpcc::TpccTransaction;

/// The application's `owner_repo` in `corpus-manifest.csv`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum UnitRule {
    S2,
    S3,
    S5,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SiteLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// What a unit belongs to: a manifest application, or one of TPC-C's
/// transaction types (`inputs.md` §1). The JSON boundary tells them apart, so
/// no later step parses a name.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Subject {
    Application(ApplicationId),
    Tpcc(TpccTransaction),
}

/// One enumerated transaction function, as an extractor reports it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRecord {
    pub subject: Subject,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub rule: UnitRule,
    pub parameters: Vec<String>,
    pub absorbed_sites: Vec<SiteLocation>,
}
