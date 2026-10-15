mod support;

use kiln_e1::{parse_units, render_units, Classification, ProvenCategory};
use serde_json::json;
use support::*;

mod kiln_e1_01 {
    use super::*;

    #[test]
    fn classified_units_survive_a_write_and_read() {
        let units = vec![
            proven("a/b", ProvenCategory::OneHop),
            unknown("a/b"),
            excluded("c/d"),
            gating("e/f"),
        ];

        assert_eq!(
            parse_units("fixture", &render_units(&units)).unwrap(),
            units
        );
    }

    #[test]
    fn a_unit_line_reads_with_all_its_fields() {
        let unit = parse_one(json!({"category": "static", "evidence": ["read#0/key"]})).unwrap();
        assert_eq!(unit.unit().rule, kiln_e1::UnitRule::S2);
        assert_eq!(unit.unit().parameters, ["order_id"]);
        assert_eq!(unit.unit().absorbed_sites.len(), 1);
        assert!(matches!(
            unit.classification(),
            Classification::Proven { .. }
        ));
    }

    #[test]
    fn only_s2_s3_and_s5_are_unit_rules() {
        for (rule, accepted) in [
            ("S2", true),
            ("S3", true),
            ("S5", true),
            ("S1", false),
            ("S4", false),
            ("s2", false),
        ] {
            let mut line = serde_json::from_str::<serde_json::Value>(&line_with(
                json!({"category": "static", "evidence": ["x"]}),
            ))
            .unwrap();
            line["unit"]["rule"] = json!(rule);
            assert_eq!(
                parse_units("fixture", &line.to_string()).is_ok(),
                accepted,
                "{rule}"
            );
        }
    }
}

mod kiln_e1_02 {
    use super::*;

    #[test]
    fn proven_labels_without_evidence_are_rejected() {
        for label in ["static", "one-hop", "domain-bounded"] {
            for evidence in [json!([]), json!([" "]), json!(["x", ""])] {
                let result = parse_one(json!({"category": label, "evidence": evidence}));
                assert!(result.is_err(), "{label} {evidence}");
            }
        }
    }

    #[test]
    fn an_all_static_stub_without_evidence_yields_no_units() {
        let stub: String = (0..3)
            .map(|_| line_with(json!({"category": "static"})) + "\n")
            .collect();
        assert!(parse_units("stub.jsonl", &stub).is_err());
    }

    #[test]
    fn proven_labels_with_evidence_are_accepted() {
        for label in ["static", "one-hop", "domain-bounded"] {
            assert!(parse_one(json!({"category": label, "evidence": ["read#0"]})).is_ok());
        }
    }
}

mod kiln_e1_03 {
    use super::*;

    #[test]
    fn unknown_needs_its_reason() {
        for fields in [
            json!({"category": "unknown"}),
            json!({"category": "unknown", "refusal_reason": "  "}),
        ] {
            assert!(parse_one(fields).is_err());
        }
        let unit = parse_one(json!({"category": "unknown", "refusal_reason": "external I/O"}));
        assert!(matches!(
            unit.unwrap().classification(),
            Classification::Unknown(_)
        ));
    }

    #[test]
    fn a_refusal_reason_contradicts_a_proven_label() {
        let result = parse_one(
            json!({"category": "static", "evidence": ["x"], "refusal_reason": "refused"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn evidence_contradicts_an_unknown_label() {
        let result =
            parse_one(json!({"category": "unknown", "refusal_reason": "r", "evidence": ["x"]}));
        assert!(result.is_err());
    }
}

mod kiln_e1_04 {
    use super::*;

    fn exclusion(receiver: &str, method: &str, reason: &str) -> serde_json::Value {
        json!({"s4_exclusion": {
            "receiver_type": receiver, "called_method": method, "no_orm_path_reason": reason,
        }})
    }

    #[test]
    fn affirmative_evidence_is_accepted() {
        let unit = parse_one(exclusion("QueryDict", "get", "dict-like lookup")).unwrap();
        assert!(matches!(
            unit.classification(),
            Classification::ExcludedNonDatabase(_)
        ));
    }

    #[test]
    fn missing_blank_or_unresolved_evidence_is_rejected() {
        let cases = [
            exclusion("", "get", "r"),
            exclusion("QueryDict", " ", "r"),
            exclusion("QueryDict", "get", ""),
            exclusion("Any", "get", "r"),
            exclusion("typing.Any", "get", "r"),
            json!({"s4_exclusion": {"receiver_type": "QueryDict", "called_method": "get"}}),
        ];
        for case in cases {
            assert!(parse_one(case.clone()).is_err(), "{case}");
        }
    }

    #[test]
    fn an_exclusion_with_a_category_or_other_fields_is_rejected() {
        let base = exclusion("QueryDict", "get", "r");
        for extra in [
            json!({"category": "static"}),
            json!({"category": "unknown"}),
            json!({"evidence": ["x"]}),
            json!({"refusal_reason": "r"}),
        ] {
            let mut combined = base.clone();
            combined
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(parse_one(combined).is_err(), "{extra}");
        }
    }

    #[test]
    fn a_unit_with_neither_category_nor_exclusion_is_rejected() {
        assert!(parse_one(json!({})).is_err());
    }
}

mod kiln_e1_05 {
    use super::*;

    #[test]
    fn a_malformed_line_names_source_and_position() {
        let good = line_with(json!({"category": "static", "evidence": ["x"]}));
        let text = format!("{good}\n{good}\n{{not json\n{good}\n");
        let error = parse_units("units.jsonl", &text).unwrap_err().to_string();
        assert!(error.starts_with("units.jsonl:3:"), "{error}");
    }

    #[test]
    fn a_contract_violation_on_a_line_names_its_position_too() {
        let good = line_with(json!({"category": "static", "evidence": ["x"]}));
        let stub = line_with(json!({"category": "static"}));
        let error = parse_units("units.jsonl", &format!("{good}\n{stub}\n")).unwrap_err();
        assert!(error.to_string().starts_with("units.jsonl:2:"), "{error}");
    }

    #[test]
    fn a_blank_line_is_malformed_and_unknown_fields_are_rejected() {
        let good = line_with(json!({"category": "static", "evidence": ["x"]}));
        assert!(parse_units("f", &format!("{good}\n\n{good}\n")).is_err());
        assert!(parse_one(json!({"category": "static", "evidence": ["x"], "extra": 1})).is_err());
    }
}
