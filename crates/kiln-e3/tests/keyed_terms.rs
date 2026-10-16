mod support;

use kiln_e2::quota::{program, QuotaState};
use kiln_e3::{
    validate, Category, CheckerForm, Expected, Family, Malformed, Path, Signature, Sort,
    StateComponent, Step, Term,
};
use support::*;

mod kiln_e3_04 {
    use super::*;

    fn respond_path(child_steps: &[usize]) -> Path {
        let mut steps = vec![Step::Function(0), Step::Respond];
        steps.extend(child_steps.iter().copied().map(Step::Child));
        Path(steps)
    }

    fn family_with_keyed_component() -> Family {
        let signature = Signature {
            components: vec![StateComponent {
                name: "remaining".to_owned(),
                kind: Sort::Keyed("phase".to_owned()),
            }],
            categories: vec![Category {
                name: "phase".to_owned(),
                size: 2,
            }],
        };
        Family::new(
            program(),
            signature,
            &QuotaState { remaining: 0 }.to_store(),
        )
    }

    fn keyed_apply(apply: Term) -> CheckerForm {
        with_form(|form| {
            form.components[0].carrier_fields = Vec::new();
            form.components[0].combine = Some(Term::Tuple(Vec::new()));
            form.components[0].apply = Some(apply);
            for function in &mut form.functions {
                function.atom = vec![Term::Tuple(Vec::new())];
                function.respond = Some(commit(lit(0)));
            }
        })
    }

    fn phase(code: usize) -> Term {
        Term::CategoryValue {
            category: "phase".to_owned(),
            code,
        }
    }

    fn keyed_literal(rows: usize) -> Term {
        Term::Keyed {
            category: "phase".to_owned(),
            rows: vec![Term::Absent; rows],
        }
    }

    #[test]
    fn validates_a_keyed_apply_that_reads_the_row_at_a_key() {
        let rows = vec![Term::AppliedRow(boxed(phase(1))), Term::Held(boxed(lit(1)))];
        let apply = Term::Keyed {
            category: "phase".to_owned(),
            rows,
        };

        let result = validate(&family_with_keyed_component(), &keyed_apply(apply));

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_a_keyed_literal_with_a_missing_row() {
        assert_eq!(
            rejection(
                &family_with_keyed_component(),
                &keyed_apply(keyed_literal(1))
            ),
            Malformed::KeyedRowCountMismatch {
                path: path(&[Step::Component(0), Step::Apply]),
                expected: 2,
                found: 1
            }
        );
    }

    #[test]
    fn rejects_a_keyed_row_that_is_not_an_opt() {
        let apply = Term::Keyed {
            category: "phase".to_owned(),
            rows: vec![Term::Absent, lit(1)],
        };

        assert_eq!(
            rejection(&family_with_keyed_component(), &keyed_apply(apply)),
            Malformed::TypeMismatch {
                path: path(&[Step::Component(0), Step::Apply, Step::Child(1)]),
                expected: Expected::Sort(Sort::Opt),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_a_row_read_of_a_state_that_is_not_keyed() {
        let respond = commit(Term::Get(
            boxed(Term::StateRow {
                component: "remaining".to_owned(),
                key: boxed(phase(0)),
            }),
            boxed(lit(0)),
        ));

        assert_eq!(
            rejection(&family_with_phase_category(), &with_respond(respond)),
            Malformed::TypeMismatch {
                path: respond_path(&[0, 0]),
                expected: Expected::Keyed,
                found: Sort::Int
            }
        );
    }

    #[test]
    fn validates_a_row_read_of_a_keyed_state() {
        let respond = ite(
            Term::IsPresent(boxed(Term::StateRow {
                component: "remaining".to_owned(),
                key: boxed(phase(0)),
            })),
            commit(lit(0)),
            commit(lit(1)),
        );
        let mut form = keyed_apply(keyed_literal(2));
        form.functions[0].respond = Some(respond);

        assert!(validate(&family_with_keyed_component(), &form).is_ok());
    }
}
