mod support;

use kiln_e3::{Family, Malformed, Step, Term};
use support::*;

fn unknown() -> Term {
    Term::Param("amount".to_owned())
}

/// Each term shape with the unknown parameter at one child position.
fn shapes_with_unknown_at_each_position() -> Vec<(Term, usize)> {
    let b = boxed;
    let key = || lit(0);
    let mut cases: Vec<(Term, usize)> = vec![
        (Term::Code(b(unknown())), 0),
        (Term::Not(b(unknown())), 0),
        (Term::IsPresent(b(unknown())), 0),
        (Term::Held(b(unknown())), 0),
        (Term::AppliedRow(b(unknown())), 0),
        (Term::Commit(b(unknown())), 0),
        (Term::Abort(b(unknown())), 0),
        (Term::Mod(b(unknown()), 3), 0),
        (
            Term::Of {
                category: "phase".to_owned(),
                code: b(unknown()),
            },
            0,
        ),
        (
            Term::StateRow {
                component: "remaining".to_owned(),
                key: b(unknown()),
            },
            0,
        ),
    ];
    type Binary = fn(Box<Term>, Box<Term>) -> Term;
    let binaries: [Binary; 9] = [
        Term::Get,
        Term::Add,
        Term::Sub,
        Term::Min,
        Term::Max,
        Term::And,
        Term::Or,
        Term::CategoryEqual,
        Term::Lookup,
    ];
    for binary in binaries {
        cases.push((binary(b(unknown()), b(key())), 0));
        cases.push((binary(b(key()), b(unknown())), 1));
    }
    cases.push((
        Term::Compare(b(unknown()), kiln_ir::Relation::Less, b(key())),
        0,
    ));
    cases.push((
        Term::Compare(b(key()), kiln_ir::Relation::Less, b(unknown())),
        1,
    ));
    for position in 0..3 {
        let operand = |at: usize| b(if at == position { unknown() } else { key() });
        cases.push((Term::Clamp(operand(0), operand(1), operand(2)), position));
        cases.push((Term::Ite(operand(0), operand(1), operand(2)), position));
    }
    for position in 0..2 {
        let parts = || {
            (0..2)
                .map(|at| if at == position { unknown() } else { key() })
                .collect::<Vec<_>>()
        };
        let category = "phase".to_owned();
        cases.push((
            Term::Keyed {
                category: category.clone(),
                rows: parts(),
            },
            position,
        ));
        cases.push((
            Term::Table {
                category,
                entries: parts(),
            },
            position,
        ));
        cases.push((Term::Tuple(parts()), position));
        cases.push((Term::When(b(key()), parts()), position + 1));
        cases.push((
            Term::Case {
                scrutinee: b(key()),
                arms: parts(),
            },
            position + 1,
        ));
    }
    cases.push((Term::When(b(unknown()), vec![key()]), 0));
    cases.push((
        Term::Case {
            scrutinee: b(unknown()),
            arms: vec![key()],
        },
        0,
    ));
    cases
}

mod kiln_e3_04 {
    use super::*;

    #[test]
    fn finds_an_undeclared_parameter_at_every_position_of_every_term_shape() {
        for (term, position) in shapes_with_unknown_at_each_position() {
            let rejected = rejection(&family_with_phase_category(), &with_respond(term.clone()));

            assert_eq!(
                rejected,
                Malformed::UnknownParameter {
                    path: path(&[Step::Function(0), Step::Respond, Step::Child(position)]),
                    name: "amount".to_owned()
                },
                "{term:?}"
            );
        }
    }

    #[test]
    fn finds_an_undeclared_category_in_every_term_that_names_one() {
        let category = "phase".to_owned();
        let terms = [
            Term::Of {
                category: category.clone(),
                code: boxed(lit(0)),
            },
            Term::Table {
                category: category.clone(),
                entries: Vec::new(),
            },
            Term::Keyed {
                category: category.clone(),
                rows: Vec::new(),
            },
        ];
        for term in terms {
            let rejected = rejection(&Family::quota(), &with_respond(term.clone()));

            assert_eq!(
                rejected,
                Malformed::UnknownCategory {
                    path: path(&[Step::Function(0), Step::Respond]),
                    name: category.clone()
                },
                "{term:?}"
            );
        }
    }
}
