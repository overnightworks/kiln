mod support;

use kiln_e3::{
    validate, Binding, CheckerForm, Expected, Family, Malformed, Operand, Path, Signature, Sort,
    Step, Term,
};
use kiln_ir::{AbortName, AbortReason};
use support::*;

mod kiln_e3_04 {
    use super::*;

    fn respond_path(child_steps: &[usize]) -> Path {
        let mut steps = vec![Step::Function(0), Step::Respond];
        steps.extend(child_steps.iter().copied().map(Step::Child));
        Path(steps)
    }

    #[test]
    fn rejects_adding_an_int_to_a_bool() {
        let form = with_respond(commit(add(lit(1), Term::True)));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: respond_path(&[0, 1]),
                expected: Expected::Sort(Sort::Int),
                found: Sort::Bool
            }
        );
    }

    #[test]
    fn rejects_a_parameter_the_function_does_not_declare() {
        let form = with_respond(commit(Term::Param("amount".to_owned())));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::UnknownParameter {
                path: respond_path(&[0]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn rejects_a_state_component_the_signature_does_not_declare() {
        let form = with_respond(commit(Term::State("balance".to_owned())));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::UnknownStateComponent {
                path: respond_path(&[0]),
                name: "balance".to_owned()
            }
        );
    }

    #[test]
    fn rejects_a_field_the_carrier_does_not_declare() {
        let form = with_form(|form| {
            form.components[0].apply = Some(add(Term::Applied, field(Operand::Summary, "z")));
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::UnknownField {
                path: path(&[Step::Component(0), Step::Apply, Step::Child(1)]),
                name: "z".to_owned()
            }
        );
    }

    fn unknown_name() -> AbortReason {
        AbortReason::Declared(AbortName("quota_closed".to_owned()))
    }

    #[test]
    fn rejects_a_name_outside_the_name_set() {
        let form = with_respond(Term::Name(unknown_name()));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::UnknownName {
                path: respond_path(&[]),
                name: Box::new(unknown_name())
            }
        );
    }

    #[test]
    fn rejects_abort_of_a_name_outside_the_name_set() {
        let form = with_respond(abort(unknown_name()));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::AbortOutsideNameSet {
                path: respond_path(&[]),
                name: Box::new(unknown_name())
            }
        );
    }

    #[test]
    fn rejects_abort_of_a_term_that_is_not_a_name() {
        let form = with_respond(Term::Abort(boxed(lit(3))));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: respond_path(&[0]),
                expected: Expected::Sort(Sort::Name),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn accepts_abort_of_every_name_of_the_family() {
        let family = Family::quota();
        for name in family.names() {
            let result = validate(&family, &with_respond(abort(name.reason.clone())));

            assert!(result.is_ok(), "{name:?}");
        }
    }

    #[test]
    fn rejects_a_case_with_a_missing_arm() {
        let respond = Term::Case {
            scrutinee: boxed(Term::Of {
                category: "phase".to_owned(),
                code: boxed(lit(1)),
            }),
            arms: vec![commit(lit(0)), commit(lit(1))],
        };

        assert_eq!(
            rejection(&family_with_phase_category(), &with_respond(respond)),
            Malformed::ArmCountMismatch {
                path: respond_path(&[]),
                expected: 3,
                found: 2
            }
        );
    }

    #[test]
    fn rejects_a_case_arm_that_is_not_an_outcome() {
        let respond = Term::Case {
            scrutinee: boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 0,
            }),
            arms: vec![commit(lit(0)), lit(1), commit(lit(0))],
        };

        assert_eq!(
            rejection(&family_with_phase_category(), &with_respond(respond)),
            Malformed::TypeMismatch {
                path: respond_path(&[2]),
                expected: Expected::Sort(Sort::Outcome),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_case_on_something_that_is_not_a_category_value() {
        let respond = Term::Case {
            scrutinee: boxed(lit(0)),
            arms: Vec::new(),
        };

        assert_eq!(
            rejection(&Family::quota(), &with_respond(respond)),
            Malformed::TypeMismatch {
                path: respond_path(&[0]),
                expected: Expected::AnyCategory,
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_category_value_beyond_the_category() {
        let respond = Term::Case {
            scrutinee: boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 3,
            }),
            arms: Vec::new(),
        };

        assert_eq!(
            rejection(&family_with_phase_category(), &with_respond(respond)),
            Malformed::CategoryValueOutOfRange {
                path: respond_path(&[0]),
                category: "phase".to_owned(),
                code: 3
            }
        );
    }

    #[test]
    fn rejects_a_category_the_signature_does_not_declare() {
        let respond = Term::Case {
            scrutinee: boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 0,
            }),
            arms: Vec::new(),
        };

        assert_eq!(
            rejection(&Family::quota(), &with_respond(respond)),
            Malformed::UnknownCategory {
                path: respond_path(&[0]),
                name: "phase".to_owned()
            }
        );
    }

    #[test]
    fn rejects_a_condition_that_is_not_a_bool() {
        let form = with_respond(ite(lit(1), commit(lit(0)), commit(lit(1))));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: respond_path(&[0]),
                expected: Expected::Sort(Sort::Bool),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_an_ite_whose_arms_differ_in_sort() {
        let form = with_respond(ite(Term::True, commit(lit(0)), lit(1)));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: respond_path(&[2]),
                expected: Expected::Sort(Sort::Outcome),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_respond_that_is_not_an_outcome() {
        let form = with_respond(lit(1));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: respond_path(&[]),
                expected: Expected::Sort(Sort::Outcome),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_valid_that_is_not_a_bool() {
        let form = with_form(|form| form.functions[0].valid = Some(lit(1)));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: path(&[Step::Function(0), Step::Valid]),
                expected: Expected::Sort(Sort::Bool),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_an_apply_of_the_wrong_sort() {
        let form = with_form(|form| form.components[0].apply = Some(Term::True));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: path(&[Step::Component(0), Step::Apply]),
                expected: Expected::Sort(Sort::Int),
                found: Sort::Bool
            }
        );
    }

    #[test]
    fn rejects_a_combine_that_is_not_a_carrier() {
        let form = with_form(|form| form.components[0].combine = Some(lit(1)));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: path(&[Step::Component(0), Step::Combine]),
                expected: Expected::Sort(Sort::Carrier),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_combine_tuple_of_the_wrong_arity() {
        let form = with_form(|form| {
            form.components[0].combine = Some(Term::Tuple(vec![lit(0)]));
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TupleArityMismatch {
                path: path(&[Step::Component(0), Step::Combine]),
                expected: 3,
                found: 1
            }
        );
    }

    #[test]
    fn rejects_a_combine_field_of_the_wrong_sort() {
        let form = with_form(|form| {
            form.components[0].combine = Some(Term::Tuple(vec![lit(0), Term::True, lit(0)]));
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: path(&[Step::Component(0), Step::Combine, Step::Child(1)]),
                expected: Expected::Sort(Sort::Int),
                found: Sort::Bool
            }
        );
    }

    #[test]
    fn rejects_an_atom_that_is_not_a_carrier() {
        let form = with_form(|form| form.functions[0].atom = vec![lit(1)]);

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::TypeMismatch {
                path: path(&[Step::Function(0), Step::Atom(0)]),
                expected: Expected::Sort(Sort::Carrier),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn validates_carrier_choices_in_an_atom_and_a_combine() {
        let selected = |left: Term, right: Term| {
            ite(
                Term::True,
                Term::Tuple(vec![left.clone(), lit(0), lit(0)]),
                Term::Tuple(vec![right.clone(), lit(0), lit(0)]),
            )
        };
        let form = with_form(|form| {
            form.components[0].combine = Some(ite(
                Term::True,
                Term::Whole(Operand::Left),
                Term::Whole(Operand::Right),
            ));
            form.functions[0].atom = vec![selected(lit(1), lit(2))];
        });

        assert!(validate(&Family::quota(), &form).is_ok());
    }

    #[test]
    fn rejects_the_summary_operand_as_a_whole_carrier() {
        let form = with_form(|form| {
            form.components[0].combine = Some(Term::Whole(Operand::Summary));
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::NotBound {
                path: path(&[Step::Component(0), Step::Combine]),
                term: Binding::SummaryOperand
            }
        );
    }

    fn not_bound(form: CheckerForm, at: &[Step], term: Binding) {
        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::NotBound {
                path: path(at),
                term
            }
        );
    }

    #[test]
    fn rejects_the_applied_value_outside_an_apply() {
        let form = with_respond(commit(Term::Applied));

        not_bound(
            form,
            &[Step::Function(0), Step::Respond, Step::Child(0)],
            Binding::Applied,
        );
    }

    #[test]
    fn rejects_a_key_outside_an_entry() {
        let form = with_respond(commit(Term::Key));

        not_bound(
            form,
            &[Step::Function(0), Step::Respond, Step::Child(0)],
            Binding::Key,
        );
    }

    #[test]
    fn rejects_state_inside_a_combine() {
        let form = with_form(|form| {
            form.components[0].combine = Some(Term::Tuple(vec![remaining(), lit(0), lit(0)]));
        });

        not_bound(
            form,
            &[Step::Component(0), Step::Combine, Step::Child(0)],
            Binding::State,
        );
    }

    #[test]
    fn rejects_the_left_operand_inside_an_apply() {
        let form = with_form(|form| {
            form.components[0].apply = Some(field(Operand::Left, "a"));
        });

        not_bound(
            form,
            &[Step::Component(0), Step::Apply],
            Binding::LeftOperand,
        );
    }

    #[test]
    fn rejects_the_summary_operand_inside_a_combine() {
        let form = with_form(|form| {
            form.components[0].combine = Some(Term::Tuple(vec![
                field(Operand::Summary, "a"),
                lit(0),
                lit(0),
            ]));
        });

        not_bound(
            form,
            &[Step::Component(0), Step::Combine, Step::Child(0)],
            Binding::SummaryOperand,
        );
    }

    #[test]
    fn rejects_the_right_operand_inside_a_function() {
        let form = with_respond(commit(field(Operand::Right, "a")));

        not_bound(
            form,
            &[Step::Function(0), Step::Respond, Step::Child(0)],
            Binding::RightOperand,
        );
    }

    #[test]
    fn rejects_a_parameter_inside_a_component() {
        let family = Family::quota();
        let form = with_form(|form| {
            form.components[0].apply = Some(quantity(&family));
        });

        assert!(matches!(
            rejection(&family, &form),
            Malformed::UnknownParameter { .. }
        ));
    }

    #[test]
    fn rejects_a_non_positive_modulus() {
        let form = with_respond(commit(Term::Mod(boxed(lit(5)), 0)));

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::ModulusNotPositive {
                path: respond_path(&[0]),
                modulus: 0
            }
        );
    }

    #[test]
    fn validates_a_positive_modulus_and_the_other_int_productions() {
        let ints = [
            Term::Mod(boxed(lit(5)), 3),
            Term::Max(boxed(lit(1)), boxed(lit(2))),
            Term::Get(boxed(Term::Held(boxed(lit(1)))), boxed(lit(0))),
            Term::Get(boxed(Term::Absent), boxed(lit(0))),
            Term::Code(boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 2,
            })),
        ];
        for int in ints {
            let result = validate(&family_with_phase_category(), &with_respond(commit(int)));

            assert!(result.is_ok());
        }
    }

    #[test]
    fn validates_the_bool_productions() {
        let phase = |code| Term::CategoryValue {
            category: "phase".to_owned(),
            code,
        };
        let bools = [
            Term::False,
            Term::Or(boxed(Term::True), boxed(Term::False)),
            Term::CategoryEqual(boxed(phase(0)), boxed(phase(1))),
            Term::IsPresent(boxed(Term::Held(boxed(lit(1))))),
        ];
        for condition in bools {
            let respond = ite(condition, commit(lit(0)), commit(lit(1)));

            let result = validate(&family_with_phase_category(), &with_respond(respond));

            assert!(result.is_ok());
        }
    }

    #[test]
    fn rejects_comparing_values_of_two_categories() {
        let family = {
            let mut signature: Signature = family_with_phase_category().signature().clone();
            signature.categories.push(category("stage", 2));
            family_with_signature(signature)
        };
        let compared = Term::CategoryEqual(
            boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 0,
            }),
            boxed(Term::CategoryValue {
                category: "stage".to_owned(),
                code: 0,
            }),
        );
        let form = with_respond(ite(compared, commit(lit(0)), commit(lit(1))));

        assert_eq!(
            rejection(&family, &form),
            Malformed::TypeMismatch {
                path: respond_path(&[0, 1]),
                expected: Expected::Sort(Sort::Cat("phase".to_owned())),
                found: Sort::Cat("stage".to_owned())
            }
        );
    }

    #[test]
    fn rejects_a_table_with_the_wrong_number_of_entries() {
        let table = Term::Table {
            category: "phase".to_owned(),
            entries: Vec::new(),
        };
        let lookup = Term::Lookup(
            boxed(table),
            boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 0,
            }),
        );
        let form = with_respond(ite(
            Term::CategoryEqual(boxed(lookup.clone()), boxed(lookup)),
            commit(lit(0)),
            commit(lit(1)),
        ));

        assert_eq!(
            rejection(&family_with_phase_category(), &form),
            Malformed::TableEntryCountMismatch {
                path: respond_path(&[0, 0, 0]),
                expected: 3,
                found: 0
            }
        );
    }

    #[test]
    fn validates_a_complete_table_and_its_lookup() {
        let phase = |code| Term::CategoryValue {
            category: "phase".to_owned(),
            code,
        };
        let table = Term::Table {
            category: "phase".to_owned(),
            entries: vec![phase(1), phase(2), phase(0)],
        };
        let lookup = Term::Lookup(boxed(table), boxed(phase(0)));
        let respond = ite(
            Term::CategoryEqual(boxed(lookup), boxed(phase(1))),
            commit(lit(0)),
            commit(lit(1)),
        );

        assert!(validate(&family_with_phase_category(), &with_respond(respond)).is_ok());
    }

    #[test]
    fn rejects_a_lookup_in_something_that_is_not_a_table() {
        let respond = commit(Term::Code(boxed(Term::Lookup(
            boxed(lit(1)),
            boxed(Term::CategoryValue {
                category: "phase".to_owned(),
                code: 0,
            }),
        ))));

        assert_eq!(
            rejection(&family_with_phase_category(), &with_respond(respond)),
            Malformed::TypeMismatch {
                path: respond_path(&[0, 0, 0]),
                expected: Expected::AnyMap,
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_tuple_where_a_value_is_wanted_and_checks_its_parts() {
        let bad_part = Term::Tuple(vec![lit(0), Term::Param("amount".to_owned())]);

        assert_eq!(
            rejection(&Family::quota(), &with_respond(commit(bad_part))),
            Malformed::UnknownParameter {
                path: respond_path(&[0, 1]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn rejects_an_entry_where_a_value_is_wanted_and_checks_its_parts() {
        let bad_part = Term::When(
            boxed(Term::True),
            vec![lit(0), Term::Param("amount".to_owned())],
        );

        assert_eq!(
            rejection(&Family::quota(), &with_respond(commit(bad_part))),
            Malformed::UnknownParameter {
                path: respond_path(&[0, 2]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn a_carrier_is_not_an_outcome() {
        assert_eq!(
            rejection(&Family::quota(), &with_respond(Term::Tuple(vec![lit(0)]))),
            Malformed::TypeMismatch {
                path: respond_path(&[]),
                expected: Expected::Sort(Sort::Outcome),
                found: Sort::Carrier
            }
        );
    }
}
