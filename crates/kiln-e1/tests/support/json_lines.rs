//! JSONL lines for the reading boundary. A test target includes this file with
//! `#[path]` and uses all of it.

use kiln_e1::{parse_units, ClassifiedUnit};
use serde_json::{json, Value};

fn unit_json() -> Value {
    json!({
        "subject": {"application": "a/b"}, "file": "app/views.py", "line": 10, "column": 4,
        "rule": "S2", "parameters": ["order_id"],
        "absorbed_sites": [{"file": "app/views.py", "line": 12, "column": 8}],
    })
}

/// A JSONL line: the unit plus the given classification fields.
pub fn line_with(classification: Value) -> String {
    line_with_unit(unit_json(), classification)
}

/// A JSONL line whose unit has the given fields replaced.
pub fn line_with_unit_fields(unit_fields: Value, classification: Value) -> String {
    let mut unit = unit_json();
    unit.as_object_mut()
        .unwrap()
        .extend(unit_fields.as_object().unwrap().clone());
    line_with_unit(unit, classification)
}

fn line_with_unit(unit: Value, classification: Value) -> String {
    let mut line = json!({ "unit": unit });
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
