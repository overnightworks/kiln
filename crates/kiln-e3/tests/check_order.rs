mod support;

use kiln_e3::{ComponentShape, Expected, Malformed, Sort, Step, Term};
use support::*;

fn unknown_parameter() -> Term {
    Term::Param("amount".to_owned())
}

fn ill_typed() -> Term {
    commit(add(lit(1), Term::True))
}

fn three_way_case(arms: Vec<Term>) -> Term {
    Term::Case {
        scrutinee: boxed(phase(0)),
        arms,
    }
}

fn respond_of(function: usize, child_steps: &[usize]) -> kiln_e3::Path {
    let mut steps = vec![Step::Function(function), Step::Respond];
    steps.extend(child_steps.iter().copied().map(Step::Child));
    path(&steps)
}

fn rejected_responds(first: Term, second: Term) -> Malformed {
    let form = with_form(|form| {
        form.functions[0].respond = Some(first);
        form.functions[1].respond = Some(second);
    });
    rejection(&family_with_phase_category(), &form)
}

mod kiln_e3_04 {
    use super::*;

    #[test]
    fn reports_a_later_undeclared_reference_before_an_earlier_type_error() {
        assert_eq!(
            rejected_responds(ill_typed(), commit(unknown_parameter())),
            Malformed::UnknownParameter {
                path: respond_of(1, &[0]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn reports_a_later_type_error_before_an_earlier_missing_case_arm() {
        let missing_arm = three_way_case(vec![commit(lit(0))]);

        assert_eq!(
            rejected_responds(missing_arm, ill_typed()),
            Malformed::TypeMismatch {
                path: respond_of(1, &[0, 1]),
                expected: Expected::Sort(Sort::Int),
                found: Sort::Bool
            }
        );
    }

    #[test]
    fn reports_an_ill_typed_arm_before_the_missing_arm_of_its_own_case() {
        let case = three_way_case(vec![commit(lit(0)), lit(1)]);

        assert_eq!(
            rejected_responds(case, commit(lit(0))),
            Malformed::TypeMismatch {
                path: respond_of(0, &[2]),
                expected: Expected::Sort(Sort::Outcome),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn reports_the_first_non_total_operation_when_nothing_is_ill_typed() {
        let first = three_way_case(vec![commit(lit(0))]);
        let second = three_way_case(Vec::new());

        assert_eq!(
            rejected_responds(first, second),
            Malformed::ArmCountMismatch {
                path: respond_of(0, &[]),
                expected: 3,
                found: 1
            }
        );
    }

    #[test]
    fn reports_a_missing_table_entry_after_a_type_error_in_a_later_component() {
        let incomplete_table = Term::Table {
            category: "phase".to_owned(),
            entries: vec![phase(0)],
        };
        let lookup = Term::Lookup(boxed(incomplete_table), boxed(phase(0)));
        let first = ite(
            Term::CategoryEqual(boxed(lookup), boxed(phase(0))),
            commit(lit(0)),
            commit(lit(1)),
        );

        assert_eq!(
            rejected_responds(first, ill_typed()),
            Malformed::TypeMismatch {
                path: respond_of(1, &[0, 1]),
                expected: Expected::Sort(Sort::Int),
                found: Sort::Bool
            }
        );
    }

    #[test]
    fn reports_an_undeclared_parameter_before_per_key_over_an_unkeyed_component() {
        let form = with_form(|form| {
            form.components[0].shape = ComponentShape::PerKey;
            form.functions[0].respond = Some(unknown_parameter());
        });

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::UnknownParameter {
                path: respond_of(0, &[]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn reports_an_undeclared_parameter_before_a_missing_combine() {
        let form = with_form(|form| {
            form.components[0].combine = None;
            form.functions[0].respond = Some(commit(unknown_parameter()));
        });

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::UnknownParameter {
                path: respond_of(0, &[0]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn reports_a_type_error_before_a_missing_combine() {
        let form = with_form(|form| {
            form.components[0].combine = None;
            form.functions[0].respond = Some(ill_typed());
        });

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::TypeMismatch {
                path: respond_of(0, &[0, 1]),
                expected: Expected::Sort(Sort::Int),
                found: Sort::Bool
            }
        );
    }

    #[test]
    fn reports_an_undeclared_parameter_before_a_wrong_atom_count() {
        let form = with_form(|form| {
            form.functions[0].atom.clear();
            form.functions[0].respond = Some(unknown_parameter());
        });

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::UnknownParameter {
                path: respond_of(0, &[]),
                name: "amount".to_owned()
            }
        );
    }

    #[test]
    fn reports_per_key_over_an_unkeyed_component_before_a_missing_combine() {
        let form = with_form(|form| {
            form.components[0].shape = ComponentShape::PerKey;
            form.components[0].combine = None;
        });

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::PerKeyOnUnkeyedComponent {
                path: path(&[Step::Component(0)])
            }
        );
    }
}
