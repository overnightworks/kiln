use std::num::NonZeroU128;

use crate::classified::{Classification, ClassifiedUnit, ProvenCategory};
use crate::ratio::{Overflow, Ratio};

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
            Classification::Unknown { .. } => self.unknown += 1,
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
    pub fn gating_share(&self) -> Option<Ratio> {
        self.share_of(self.gating)
    }

    /// Reported next to the gating share and never gating
    /// (`docs/experiments.md`, "Gating numerator and verdict").
    pub fn domain_bounded_share(&self) -> Option<Ratio> {
        self.share_of(self.domain_bounded)
    }

    fn share_of(&self, units: u32) -> Option<Ratio> {
        NonZeroU128::new(self.denominator().into())
            .map(|denominator| Ratio::new(units.into(), denominator))
    }
}

/// The middle share, or the mean of the two middle shares for an even count;
/// `None` for no shares.
pub fn median(shares: &[Ratio]) -> Result<Option<Ratio>, Overflow> {
    let mut sorted = shares.to_vec();
    sorted.sort();
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => Ok(None),
        length if length % 2 == 1 => Ok(Some(sorted[middle])),
        _ => sorted[middle - 1]
            .checked_add(sorted[middle])?
            .checked_half()
            .map(Some),
    }
}
