//! The classified TPC-C units: each profile refused in full is an unknown
//! unit carrying its refusal (`inputs.md` §5, Q4.1).

use crate::classified::{Classification, ClassifiedUnit, Reason};
use crate::unit::{Subject, UnitRecord, UnitRule};

use super::lowering_refusal::LoweringRefusal;
use super::profile::Profile;
use super::TpccTransaction;

/// The specification PDF the profiles are lowered from.
const SPECIFICATION_FILE: &str = "tpc-c_v5.11.0.pdf";

/// The gating units and, apart, the sensitivity units that never gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TpccUnits {
    pub gating: Vec<ClassifiedUnit>,
    pub sensitivity: Vec<ClassifiedUnit>,
}

pub fn tpcc_units() -> TpccUnits {
    TpccUnits {
        gating: TpccTransaction::ALL
            .map(Profile::complete)
            .iter()
            .map(unit_of)
            .collect(),
        sensitivity: Profile::sensitivity().iter().map(unit_of).collect(),
    }
}

/// The position is the profile's clause in the specification and the printed
/// page it starts on; every transaction is one explicit database transaction
/// (Clauses 2.4.2.1, 2.5.2.1, 2.6.2.1, 2.7.4.1, 2.8.2.1), so rule S2.
fn unit_of(profile: &Profile) -> ClassifiedUnit {
    let refusal = LoweringRefusal::of(profile).unwrap_or_else(|| {
        panic!(
            "every step of the {} profile is expressible, so it needs its lowering written",
            profile.transaction
        )
    });
    let record = UnitRecord {
        subject: Subject::Tpcc(profile.transaction),
        file: format!("{SPECIFICATION_FILE}#clause-{}", profile.clause),
        line: profile.page,
        column: 1,
        rule: UnitRule::S2,
        parameters: profile
            .parameters
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        absorbed_sites: Vec::new(),
    };
    let reason = Reason::new(refusal.to_string()).expect("a refusal states its steps");
    ClassifiedUnit::new(
        record,
        Classification::Unknown {
            reason,
            evidence: None,
        },
    )
}
