#![allow(dead_code)]

use kiln_e1::{
    parse_units, ApplicationId, Classification, ClassifiedUnit, Evidence, Manifest, ProvenCategory,
    Reason, S4Exclusion, SiteLocation, TpccTransaction, UnitRecord, UnitRule,
};
use serde_json::{json, Value};

pub fn record(application: &str) -> UnitRecord {
    UnitRecord {
        application: ApplicationId::new(application),
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

pub fn proven(application: &str, category: ProvenCategory) -> ClassifiedUnit {
    let evidence = Evidence::new(vec!["read#0/key".to_owned()]).unwrap();
    ClassifiedUnit::new(
        record(application),
        Classification::Proven { category, evidence },
    )
}

pub fn gating(application: &str) -> ClassifiedUnit {
    proven(application, ProvenCategory::Static)
}

pub fn unknown(application: &str) -> ClassifiedUnit {
    let reason = Reason::new("front end refused: arbitrary external I/O").unwrap();
    ClassifiedUnit::new(record(application), Classification::Unknown(reason))
}

pub fn excluded(application: &str) -> ClassifiedUnit {
    let exclusion = S4Exclusion::new(
        "django.http.request.QueryDict".to_owned(),
        "get".to_owned(),
        "dict-like lookup, no ORM path".to_owned(),
    )
    .unwrap();
    ClassifiedUnit::new(
        record(application),
        Classification::ExcludedNonDatabase(exclusion),
    )
}

pub fn repeated(count: usize, unit: ClassifiedUnit) -> Vec<ClassifiedUnit> {
    vec![unit; count]
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
                unknown(transaction.name())
            } else {
                gating(transaction.name())
            }
        })
        .collect()
}

pub fn unit_json(application: &str) -> Value {
    json!({
        "application": application, "file": "app/views.py", "line": 10, "column": 4,
        "rule": "S2", "parameters": ["order_id"],
        "absorbed_sites": [{"file": "app/views.py", "line": 12, "column": 8}],
    })
}

/// A JSONL line: the unit plus the given classification fields.
pub fn line_with(classification: Value) -> String {
    let mut line = json!({ "unit": unit_json("a/b") });
    line.as_object_mut()
        .unwrap()
        .extend(classification.as_object().unwrap().clone());
    line.to_string()
}

pub fn parse_one(classification: Value) -> Result<ClassifiedUnit, String> {
    parse_units("fixture", &line_with(classification))
        .map(|mut units| units.remove(0))
        .map_err(|error| error.to_string())
}
