use std::fmt;

use crate::classified::{Classification, ClassifiedUnit, ProvenCategory};

/// A share in percent, 0 to 100.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Percent(pub f64);

impl fmt::Display for Percent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:.4} %", self.0)
    }
}

/// One group of units counted by category; S4 exclusions are counted apart
/// because they leave the denominator (`inputs.md` §3, S4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    gating: u32,
    domain_bounded: u32,
    unknown: u32,
    excluded: u32,
}

impl Tally {
    pub fn count(&mut self, unit: &ClassifiedUnit) {
        match unit.classification() {
            Classification::Proven {
                category: ProvenCategory::Static | ProvenCategory::OneHop,
                ..
            } => self.gating += 1,
            Classification::Proven {
                category: ProvenCategory::DomainBounded,
                ..
            } => self.domain_bounded += 1,
            Classification::Unknown(_) => self.unknown += 1,
            Classification::ExcludedNonDatabase(_) => self.excluded += 1,
        }
    }

    pub fn denominator(&self) -> u32 {
        self.gating + self.domain_bounded + self.unknown
    }

    pub fn excluded(&self) -> u32 {
        self.excluded
    }

    /// Static or one-hop units over the denominator; `None` when it is empty.
    pub fn gating_share(&self) -> Option<Percent> {
        self.share_of(self.gating)
    }

    /// Reported next to the gating share and never gating
    /// (`docs/experiments.md`, "Gating numerator and verdict").
    pub fn domain_bounded_share(&self) -> Option<Percent> {
        self.share_of(self.domain_bounded)
    }

    fn share_of(&self, units: u32) -> Option<Percent> {
        match self.denominator() {
            0 => None,
            denominator => Some(Percent(100.0 * f64::from(units) / f64::from(denominator))),
        }
    }
}

/// The middle share, or the mean of the two middle shares for an even count;
/// `None` for no shares.
pub fn median(shares: &[Percent]) -> Option<Percent> {
    let mut sorted: Vec<f64> = shares.iter().map(|share| share.0).collect();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        length if length % 2 == 1 => Some(Percent(sorted[middle])),
        _ => Some(Percent((sorted[middle - 1] + sorted[middle]) / 2.0)),
    }
}
