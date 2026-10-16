use std::fmt;

use crate::ratio::Ratio;

/// `docs/experiments.md`, "Gating numerator and verdict": pass at least 70 %.
const PASS_AT_LEAST_PERCENT: u8 = 70;
/// Same section: fail below 50 %.
const FAIL_BELOW_PERCENT: u8 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Inconclusive,
}

impl Verdict {
    /// A share below the fail threshold fails whatever the other share is.
    pub fn from_shares(tpcc_weighted: Ratio, application_median: Ratio) -> Self {
        let shares = [tpcc_weighted, application_median];
        if shares
            .iter()
            .any(|share| *share < Ratio::from_percent(FAIL_BELOW_PERCENT))
        {
            Self::Fail
        } else if shares
            .iter()
            .all(|share| *share >= Ratio::from_percent(PASS_AT_LEAST_PERCENT))
        {
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
