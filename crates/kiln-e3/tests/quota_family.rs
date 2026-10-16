mod support;

use kiln_e2::quota::QuotaAbort;
use kiln_e3::{validate, Family, Sort, Term};
use kiln_ir::{
    AbortReason, CheckConstraint, ColumnName, Comparison, Key, OverflowReason, TableName, Value,
};

use support::*;

mod kiln_e3_01 {
    use super::*;

    fn quota_check(comparison: Comparison, operand: i64) -> AbortReason {
        AbortReason::CheckViolation {
            constraint: CheckConstraint {
                table: TableName("quota".to_owned()),
                column: ColumnName("remaining".to_owned()),
                comparison,
                operand: Value::Integer(operand),
            },
            key: Key(vec![Value::Integer(0)]),
        }
    }

    #[test]
    fn quota_names_are_the_frozen_table_of_grammar_1_2_with_the_checker_only_ones_marked() {
        let names: Vec<(AbortReason, bool)> = Family::quota()
            .names()
            .iter()
            .map(|name| (name.reason.clone(), name.checker_only))
            .collect();

        let mut expected: Vec<(AbortReason, bool)> = [
            QuotaAbort::InvalidQuantity,
            QuotaAbort::QuotaExhausted,
            QuotaAbort::QuotaFull,
        ]
        .into_iter()
        .map(|abort| (declared(abort), false))
        .collect();
        expected.extend([
            (AbortReason::Overflow(OverflowReason::Arithmetic), false),
            (
                AbortReason::Overflow(OverflowReason::Width {
                    table: TableName("quota".to_owned()),
                    column: ColumnName("remaining".to_owned()),
                    key: Key(vec![Value::Integer(0)]),
                }),
                true,
            ),
            (quota_check(Comparison::AtLeast, 0), true),
            (quota_check(Comparison::AtMost, 1000), true),
        ]);
        assert_eq!(names.len(), expected.len());
        for name in expected {
            assert!(names.contains(&name), "missing {name:?}");
        }
    }

    #[test]
    fn quota_signature_is_one_int_component_named_remaining() {
        let signature = Family::quota().signature().clone();

        assert_eq!(signature.components.len(), 1);
        assert_eq!(signature.components[0].name, "remaining");
        assert_eq!(signature.components[0].kind, Sort::Int);
        assert!(signature.categories.is_empty());
    }

    #[test]
    fn quota_literal_pool_holds_program_literals_and_value_width_bounds_and_nothing_else() {
        let pool = Family::quota().literal_pool().clone();

        let expected: std::collections::BTreeSet<i64> =
            [0, 1, 50, 1000, i64::from(i32::MIN), i64::from(i32::MAX)]
                .into_iter()
                .collect();
        assert_eq!(pool, expected);
    }
}

mod kiln_e3_02 {
    use super::*;

    #[test]
    fn validates_a_quota_form_with_a_shift_component() {
        let form = quota_form(Variant::Shift);

        let valid = validate(&Family::quota(), &form).expect("the form is well-typed");

        assert_eq!(valid.form(), &form);
    }

    #[test]
    fn validates_a_quota_form_with_a_clamp_shift_component() {
        let form = quota_form(Variant::ClampShift);

        let valid = validate(&Family::quota(), &form).expect("the form is well-typed");

        assert_eq!(valid.form(), &form);
    }

    #[test]
    fn validates_a_case_with_one_arm_per_category_value() {
        let arms = vec![commit(lit(0)), commit(lit(1)), commit(lit(0))];
        let respond = Term::Case {
            scrutinee: boxed(Term::Of {
                category: "phase".to_owned(),
                code: boxed(lit(1)),
            }),
            arms,
        };

        let result = validate(&family_with_phase_category(), &with_respond(respond));

        assert!(result.is_ok());
    }
}
