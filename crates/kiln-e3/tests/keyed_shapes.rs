mod support;

use kiln_e3::{
    validate, Binding, CheckerForm, ComponentDef, ComponentShape, Expected, Malformed, Operand,
    Sort, Step, Term,
};
use kiln_ir::Relation;
use support::*;

/// `per-key(fill)` over the keyed `remaining`: combine keeps the earlier row,
/// apply inserts when the row is absent.
fn per_key_fill() -> ComponentDef {
    ComponentDef {
        carrier_fields: vec![int_field("v")],
        shape: ComponentShape::PerKey,
        combine: Some(Term::Whole(Operand::Left)),
        apply: Some(ite(
            Term::IsPresent(boxed(Term::Applied)),
            Term::Applied,
            Term::Held(boxed(field(Operand::Summary, "v"))),
        )),
    }
}

fn entry_at_key_zero() -> Term {
    Term::When(
        boxed(compare(Term::Key, Relation::Equal, lit(0))),
        vec![lit(1)],
    )
}

fn per_key_form(component: ComponentDef, atom: Term) -> CheckerForm {
    with_form(|form| {
        form.components = vec![component];
        for function in &mut form.functions {
            function.atom = vec![atom.clone()];
            function.respond = Some(commit(lit(0)));
        }
    })
}

fn whole_keyed_form(apply: Term) -> CheckerForm {
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

fn keyed_rows(rows: Vec<Term>) -> Term {
    Term::Keyed {
        category: "phase".to_owned(),
        rows,
    }
}

mod kiln_e3_02 {
    use super::*;

    #[test]
    fn validates_a_per_key_fill_whose_apply_receives_the_row() {
        let form = per_key_form(per_key_fill(), entry_at_key_zero());

        let result = validate(&family_with_keyed_component(), &form);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn validates_a_whole_keyed_apply_that_returns_a_keyed_literal() {
        let rows = vec![Term::AppliedRow(boxed(phase(1))), Term::Absent];

        let result = validate(
            &family_with_keyed_component(),
            &whole_keyed_form(keyed_rows(rows)),
        );

        assert!(result.is_ok(), "{result:?}");
    }
}

mod kiln_e3_04 {
    use super::*;

    fn rejected_per_key(component: ComponentDef, atom: Term) -> Malformed {
        rejection(
            &family_with_keyed_component(),
            &per_key_form(component, atom),
        )
    }

    fn atom_path(child_steps: &[Step]) -> kiln_e3::Path {
        let mut steps = vec![Step::Function(0), Step::Atom(0)];
        steps.extend_from_slice(child_steps);
        path(&steps)
    }

    #[test]
    fn rejects_a_per_key_apply_that_returns_something_other_than_a_row() {
        let component = ComponentDef {
            apply: Some(lit(1)),
            ..per_key_fill()
        };

        assert_eq!(
            rejected_per_key(component, entry_at_key_zero()),
            Malformed::TypeMismatch {
                path: path(&[Step::Component(0), Step::Apply]),
                expected: Expected::Sort(Sort::Opt),
                found: Sort::Int
            }
        );
    }

    #[test]
    fn rejects_the_keyed_state_read_as_a_row_inside_a_per_key_apply() {
        let component = ComponentDef {
            apply: Some(Term::AppliedRow(boxed(phase(0)))),
            ..per_key_fill()
        };

        assert_eq!(
            rejected_per_key(component, entry_at_key_zero()),
            Malformed::TypeMismatch {
                path: path(&[Step::Component(0), Step::Apply]),
                expected: Expected::Keyed,
                found: Sort::Opt
            }
        );
    }

    #[test]
    fn rejects_a_per_key_atom_that_is_not_an_entry() {
        let atom = Term::Tuple(vec![lit(1)]);

        assert_eq!(
            rejected_per_key(per_key_fill(), atom),
            Malformed::TypeMismatch {
                path: atom_path(&[]),
                expected: Expected::Entry,
                found: Sort::Carrier
            }
        );
    }

    #[test]
    fn rejects_an_entry_nested_in_an_entry() {
        let atom = Term::When(boxed(Term::True), vec![entry_at_key_zero()]);

        assert_eq!(
            rejected_per_key(per_key_fill(), atom),
            Malformed::NotBound {
                path: atom_path(&[Step::Child(1)]),
                term: Binding::Entry
            }
        );
    }

    #[test]
    fn rejects_a_key_in_the_combine_of_a_per_key_component() {
        let atom = Term::When(boxed(Term::True), vec![lit(1)]);
        let component = ComponentDef {
            combine: Some(Term::Tuple(vec![Term::Key])),
            ..per_key_fill()
        };

        assert_eq!(
            rejected_per_key(component, atom),
            Malformed::NotBound {
                path: path(&[Step::Component(0), Step::Combine, Step::Child(0)]),
                term: Binding::Key
            }
        );
    }

    #[test]
    fn rejects_an_entry_in_the_atom_of_a_component_that_is_not_per_key() {
        let form = with_form(|form| form.functions[0].atom = vec![entry_at_key_zero()]);

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::NotBound {
                path: atom_path(&[]),
                term: Binding::Entry
            }
        );
    }

    #[test]
    fn rejects_a_key_in_the_atom_of_a_component_that_is_not_per_key() {
        let form = with_form(|form| {
            form.functions[0].atom = vec![Term::Tuple(vec![Term::Key, lit(0), lit(0)])];
        });

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::NotBound {
                path: atom_path(&[Step::Child(0)]),
                term: Binding::Key
            }
        );
    }

    #[test]
    fn rejects_an_entry_inside_a_combine() {
        let form = with_form(|form| form.components[0].combine = Some(entry_at_key_zero()));

        assert_eq!(
            rejection(&kiln_e3::Family::quota(), &form),
            Malformed::NotBound {
                path: path(&[Step::Component(0), Step::Combine]),
                term: Binding::Entry
            }
        );
    }

    #[test]
    fn rejects_the_bare_applied_value_in_a_whole_keyed_apply() {
        assert_eq!(
            rejection(
                &family_with_keyed_component(),
                &whole_keyed_form(Term::Applied)
            ),
            Malformed::NotBound {
                path: path(&[Step::Component(0), Step::Apply]),
                term: Binding::Applied
            }
        );
    }

    #[test]
    fn rejects_a_whole_keyed_apply_that_chooses_between_keyed_literals() {
        let rows = || keyed_rows(vec![Term::Absent, Term::Absent]);
        let apply = ite(Term::True, rows(), rows());

        assert_eq!(
            rejection(&family_with_keyed_component(), &whole_keyed_form(apply)),
            Malformed::NoProduction {
                path: path(&[Step::Component(0), Step::Apply]),
                sort: Sort::Keyed("phase".to_owned())
            }
        );
    }

    #[test]
    fn rejects_the_whole_keyed_state_read_as_a_value() {
        let family = family_with_keyed_component();
        let form = with_form(|form| {
            form.components[0].carrier_fields = Vec::new();
            form.components[0].combine = Some(Term::Tuple(Vec::new()));
            form.components[0].apply = Some(keyed_rows(vec![Term::Absent, Term::Absent]));
            for function in &mut form.functions {
                function.atom = vec![Term::Tuple(Vec::new())];
                function.respond = Some(commit(remaining()));
            }
        });

        assert_eq!(
            rejection(&family, &form),
            Malformed::NoProduction {
                path: path(&[Step::Function(0), Step::Respond, Step::Child(0)]),
                sort: Sort::Keyed("phase".to_owned())
            }
        );
    }
}
