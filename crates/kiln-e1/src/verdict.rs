use std::fmt;

use crate::coverage::Percent;

/// `docs/experiments.md`, "Gating numerator and verdict": pass at least 70 %.
const PASS_AT_LEAST: f64 = 70.0;
/// Same section: fail below 50 %.
const FAIL_BELOW: f64 = 50.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Inconclusive,
}

impl Verdict {
    /// A share below the fail threshold fails whatever the other share is.
    pub fn from_shares(tpcc_weighted: Percent, application_median: Percent) -> Self {
        let shares = [tpcc_weighted.0, application_median.0];
        if shares.iter().any(|share| *share < FAIL_BELOW) {
            Self::Fail
        } else if shares.iter().all(|share| *share >= PASS_AT_LEAST) {
            Self::Pass
        } else {
            Self::Inconclusive
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Inconclusive => "INCONCLUSIVE",
        })
    }
}
