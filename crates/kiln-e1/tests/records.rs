use kiln_e1::{parse_units, ApplicationId, Classification, Subject, TpccTransaction, UnitRule};
use serde_json::json;

#[path = "support/json_lines.rs"]
mod json_lines;
use json_lines::*;

mod kiln_e1_01 {
    use super::*;

    #[test]
    fn a_unit_line_reads_with_all_its_fields() {
        let unit = parse_one(json!({"category": "static", "evidence": ["read#0/key"]})).unwrap();
        assert_eq!(
            unit.unit().subject,
            Subject::Application(ApplicationId::new("a/b"))
        );
        assert_eq!(unit.unit().file, "app/views.py");
        assert_eq!((unit.unit().line, unit.unit().column), (10, 4));
        assert_eq!(unit.unit().rule, UnitRule::S2);
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
            let line = line_with_unit_fields(
                json!({"rule": rule}),
                json!({"category": "static", "evidence": ["x"]}),
            );
            assert_eq!(parse_units("fixture", &line).is_ok(), accepted, "{rule}");
        }
    }

    #[test]
    fn a_subject_is_a_manifest_application_or_a_tpcc_transaction_type() {
        let cases = [
            (
                json!({"application": "a/b"}),
                Some(Subject::Application(ApplicationId::new("a/b"))),
            ),
            (
                json!({"tpcc": "new-order"}),
                Some(Subject::Tpcc(TpccTransaction::NewOrder)),
            ),
            (
                json!({"tpcc": "stock-level"}),
                Some(Subject::Tpcc(TpccTransaction::StockLevel)),
            ),
            (json!({"tpcc": "refund"}), None),
            (json!({"tpcc": "New-Order"}), None),
            (json!("payment"), None),
            (json!({}), None),
            (json!({"application": "a/b", "tpcc": "payment"}), None),
        ];
        for (subject, expected) in cases {
            let line = line_with_unit_fields(
                json!({"subject": subject}),
                json!({"category": "static", "evidence": ["x"]}),
            );
            let subject_read = parse_units("fixture", &line)
                .ok()
                .map(|units| units[0].unit().subject.clone());
            assert_eq!(subject_read, expected, "{subject}");
        }
    }
}

mod kiln_e1_02 {
    use super::*;

    #[test]
    fn proven_labels_without_evidence_are_rejected() {
        for label in ["static", "one-hop", "domain-bounded"] {
            for evidence in [json!([]), json!([" "]), json!(["x", ""])] {
                let error = parse_one(json!({"category": label, "evidence": evidence}))
                    .expect_err(&format!("{label} {evidence}"));
                assert!(error.contains("evidence"), "{error}");
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
            Classification::Unknown { evidence: None, .. }
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
    fn an_unknown_label_may_carry_evidence_beside_its_reason() {
        let unit =
            parse_one(json!({"category": "unknown", "refusal_reason": "r", "evidence": ["x"]}))
                .unwrap();
        assert!(matches!(
            unit.classification(),
            Classification::Unknown {
                evidence: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn evidence_on_an_unknown_label_must_not_be_blank() {
        let result =
            parse_one(json!({"category": "unknown", "refusal_reason": "r", "evidence": [" "]}));
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
            (exclusion("", "get", "r"), "receiver_type"),
            (exclusion("QueryDict", " ", "r"), "called_method"),
            (exclusion("QueryDict", "get", ""), "no_orm_path_reason"),
            (exclusion("Any", "get", "r"), "unresolved"),
            (exclusion("typing.Any", "get", "r"), "unresolved"),
            (
                json!({"s4_exclusion": {"receiver_type": "QueryDict", "called_method": "get"}}),
                "no_orm_path_reason",
            ),
        ];
        for (case, expected) in cases {
            let error = parse_one(case.clone()).expect_err(&case.to_string());
            assert!(error.contains(expected), "{error}");
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
