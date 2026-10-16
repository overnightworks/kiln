//! A profile whose lowering the IR refuses in full (`inputs.md` §5, Q4.1).

use std::fmt;

use super::profile::{Profile, Requirement};

/// One step of a profile that the IR cannot express.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocker {
    pub clause: &'static str,
    pub summary: &'static str,
    pub requirement: Requirement,
    pub why: String,
}

/// Every step of a profile that blocks its lowering; at least one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringRefusal {
    blockers: Vec<Blocker>,
}

impl LoweringRefusal {
    /// The refusal of `profile`, or `None` when the IR expresses every step.
    pub fn of(profile: &Profile) -> Option<Self> {
        let blockers: Vec<Blocker> = profile
            .steps
            .iter()
            .flat_map(|step| {
                step.requires.iter().filter_map(|requirement| {
                    requirement.why_inexpressible().map(|why| Blocker {
                        clause: step.clause,
                        summary: step.summary,
                        requirement: *requirement,
                        why,
                    })
                })
            })
            .collect();
        (!blockers.is_empty()).then_some(Self { blockers })
    }

    pub fn blockers(&self) -> &[Blocker] {
        &self.blockers
    }
}

impl fmt::Display for LoweringRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "refused in full: this lowering is blocked at {} steps that are not redone onto the IR yet or that the IR cannot express yet",
            self.blockers.len()
        )?;
        for blocker in &self.blockers {
            write!(
                formatter,
                "; Clause {} ({}): {}",
                blocker.clause, blocker.summary, blocker.why
            )?;
        }
        Ok(())
    }
}
