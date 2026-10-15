//! Builders for classified units and manifests. A test target includes this
//! file with `#[path]` and uses all of it.

use kiln_e1::{
    ApplicationId, Classification, ClassifiedUnit, Evidence, Manifest, ProvenCategory, Reason,
    S4Exclusion, SiteLocation, Subject, TpccTransaction, UnitRecord, UnitRule,
};

/// The subject of a unit: a manifest application by id, or a TPC-C transaction.
pub trait IntoSubject {
    fn into_subject(self) -> Subject;
}

impl IntoSubject for &str {
    fn into_subject(self) -> Subject {
        Subject::Application(ApplicationId::new(self))
    }
}

impl IntoSubject for TpccTransaction {
    fn into_subject(self) -> Subject {
        Subject::Tpcc(self)
    }
}

fn record(subject: impl IntoSubject) -> UnitRecord {
    UnitRecord {
        subject: subject.into_subject(),
        file: "app/views.py".to_owned(),
        line: 10,
        column: 4,
        rule: UnitRule::S3,
        parameters: vec!["order_id".to_owned()],
        absorbed_sites: vec![SiteLocation {
            file: "app/views.py".to_owned(),
            line: 12,
            column: 8,
        }],
    }
}

pub fn proven(subject: impl IntoSubject, category: ProvenCategory) -> ClassifiedUnit {
    let evidence = Evidence::new(vec!["read#0/key".to_owned()]).unwrap();
    ClassifiedUnit::new(
        record(subject),
        Classification::Proven { category, evidence },
    )
}

pub fn gating(subject: impl IntoSubject) -> ClassifiedUnit {
    proven(subject, ProvenCategory::Static)
}

pub fn unknown(subject: impl IntoSubject) -> ClassifiedUnit {
    let reason = Reason::new("front end refused: arbitrary external I/O").unwrap();
    ClassifiedUnit::new(
        record(subject),
        Classification::Unknown {
            reason,
            evidence: None,
        },
    )
}

pub fn excluded(subject: impl IntoSubject) -> ClassifiedUnit {
    let exclusion = S4Exclusion::new(
        "django.http.request.QueryDict".to_owned(),
        "get".to_owned(),
        "dict-like lookup, no ORM path".to_owned(),
    )
    .unwrap();
    ClassifiedUnit::new(
        record(subject),
        Classification::ExcludedNonDatabase(exclusion),
    )
}

/// `total` units of the subject, `gating` of them static and the rest unknown.
pub fn units_with_share(
    subject: impl IntoSubject + Clone,
    gating_units: usize,
    total: usize,
) -> Vec<ClassifiedUnit> {
    let mut units = vec![gating(subject.clone()); gating_units];
    units.resize(total, unknown(subject));
    units
}

pub fn manifest(applications: &[&str]) -> Manifest {
    let rows: String = applications
        .iter()
        .map(|id| format!("Django,{id}\n"))
        .collect();
    Manifest::parse(&format!("orm,owner_repo\n{rows}")).unwrap()
}

/// One static unit per TPC-C transaction type, except the listed ones, which are unknown.
pub fn tpcc_units(unknown_transactions: &[TpccTransaction]) -> Vec<ClassifiedUnit> {
    TpccTransaction::ALL
        .into_iter()
        .map(|transaction| {
            if unknown_transactions.contains(&transaction) {
                unknown(transaction)
            } else {
                gating(transaction)
            }
        })
        .collect()
}

/// Every TPC-C transaction type with its own `(static units, units)` share, in
/// the order of `TpccTransaction::ALL`.
pub fn tpcc_units_with_shares(shares: [(usize, usize); 5]) -> Vec<ClassifiedUnit> {
    TpccTransaction::ALL
        .into_iter()
        .zip(shares)
        .flat_map(|(transaction, (gating_units, total))| {
            units_with_share(transaction, gating_units, total)
        })
        .collect()
}
