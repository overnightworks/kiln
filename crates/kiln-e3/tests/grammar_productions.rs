mod support;

use kiln_e3::{validate, Binding, Family, Malformed, Operand, Sort, Step, Term};
use kiln_ir::{AbortReason, OverflowReason, Relation};
use support::*;

fn arithmetic_overflow() -> AbortReason {
    AbortReason::Overflow(OverflowReason::Arithmetic)
}

fn respond_path(child_steps: &[usize]) -> kiln_e3::Path {
    let mut steps = vec![Step::Function(0), Step::Respond];
    steps.extend(child_steps.iter().copied().map(Step::Child));
    path(&steps)
}

fn phase_table() -> Term {
    Term::Table {
        category: "phase".to_owned(),
        entries: vec![phase(1), phase(2), phase(0)],
    }
}

fn rejection_of_respond(respond: Term) -> Malformed {
    rejection(&family_with_phase_category(), &with_respond(respond))
}

mod kiln_e3_02 {
    use super::*;

    #[test]
    fn validates_ite_over_each_sort_the_grammar_gives_it() {
        let phase_choice = ite(Term::True, phase(0), phase(1));
        let int_choice = ite(Term::True, lit(0), lit(1));
        let row_choice = ite(Term::True, Term::Absent, Term::Held(boxed(lit(1))));
        let responds = [
            commit(int_choice),
            ite(
                Term::CategoryEqual(boxed(phase_choice), boxed(phase(2))),
                commit(lit(0)),
                commit(lit(1)),
            ),
            commit(Term::Get(boxed(row_choice), boxed(lit(0)))),
        ];
        for respond in responds {
            let result = validate(&family_with_phase_category(), &with_respond(respond));

            assert!(result.is_ok(), "{result:?}");
        }
    }

    #[test]
    fn validates_state_and_parameters_in_a_respond_and_parameters_in_valid_and_atom() {
        let family = Family::quota();
        let form = with_form(|form| {
            form.functions[0].valid = Some(compare(quantity(&family), Relation::Greater, lit(0)));
            form.functions[0].atom = vec![Term::Tuple(vec![quantity(&family), lit(0), lit(1000)])];
            form.functions[0].respond = Some(commit(add(remaining(), quantity(&family))));
        });

        assert!(validate(&family, &form).is_ok());
    }
}

mod kiln_e3_04 {
    use super::*;

    #[test]
    fn rejects_ite_over_names() {
        let respond = Term::Abort(boxed(ite(
            Term::True,
            Term::Name(arithmetic_overflow()),
            Term::Name(arithmetic_overflow()),
        )));

        assert_eq!(
            rejection(&Family::quota(), &with_respond(respond)),
            Malformed::NoProduction {
                path: respond_path(&[0]),
                sort: Sort::Name
            }
        );
    }

    #[test]
    fn rejects_ite_over_maps() {
        let chosen = ite(Term::True, phase_table(), phase_table());
        let respond = commit(Term::Code(boxed(Term::Lookup(
            boxed(chosen),
            boxed(phase(0)),
        ))));

        assert_eq!(
            rejection_of_respond(respond),
            Malformed::NoProduction {
                path: respond_path(&[0, 0, 0]),
                sort: Sort::Map("phase".to_owned())
            }
        );
    }

    #[test]
    fn rejects_ite_over_bools() {
        let respond = ite(
            ite(Term::True, Term::True, Term::False),
            commit(lit(0)),
            commit(lit(1)),
        );

        assert_eq!(
            rejection_of_respond(respond),
            Malformed::NoProduction {
                path: respond_path(&[0]),
                sort: Sort::Bool
            }
        );
    }

    #[test]
    fn rejects_ite_over_carriers_where_a_value_is_wanted() {
        let carriers = ite(Term::True, Term::Tuple(Vec::new()), Term::Tuple(Vec::new()));

        assert_eq!(
            rejection_of_respond(commit(carriers)),
            Malformed::NoProduction {
                path: respond_path(&[0]),
                sort: Sort::Carrier
            }
        );
    }

    #[test]
    fn rejects_state_in_valid() {
        let form = with_form(|form| {
            form.functions[0].valid = Some(compare(remaining(), Relation::Greater, lit(0)));
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::NotBound {
                path: path(&[Step::Function(0), Step::Valid, Step::Child(0)]),
                term: Binding::State
            }
        );
    }

    #[test]
    fn rejects_state_in_an_atom() {
        let form = with_form(|form| {
            form.functions[1].atom = vec![Term::Tuple(vec![remaining(), lit(0), lit(1000)])];
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::NotBound {
                path: path(&[Step::Function(1), Step::Atom(0), Step::Child(0)]),
                term: Binding::State
            }
        );
    }

    #[test]
    fn rejects_an_operand_field_in_a_respond() {
        let respond = commit(field(Operand::Summary, "a"));

        assert_eq!(
            rejection(&Family::quota(), &with_respond(respond)),
            Malformed::NotBound {
                path: respond_path(&[0]),
                term: Binding::SummaryOperand
            }
        );
    }
}
