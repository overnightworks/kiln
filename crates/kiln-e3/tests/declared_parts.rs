mod support;

use kiln_e3::{
    validate, ComponentShape, Family, Field, Malformed, Operand, Path, Sort, Step, Term,
};

use support::*;

mod kiln_e3_03 {
    use super::*;

    #[test]
    fn rejects_a_component_without_combine() {
        let form = with_form(|form| form.components[0].combine = None);

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::MissingCombine {
                path: path(&[Step::Component(0), Step::Combine])
            }
        );
    }

    #[test]
    fn rejects_a_component_without_apply() {
        let form = with_form(|form| form.components[0].apply = None);

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::MissingApply {
                path: path(&[Step::Component(0), Step::Apply])
            }
        );
    }

    #[test]
    fn rejects_a_function_without_respond() {
        let form = with_form(|form| form.functions[1].respond = None);

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::MissingRespond {
                path: path(&[Step::Function(1), Step::Respond])
            }
        );
    }

    #[test]
    fn rejects_a_function_without_valid() {
        let form = with_form(|form| form.functions[0].valid = None);

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::MissingValid {
                path: path(&[Step::Function(0), Step::Valid])
            }
        );
    }

    #[test]
    fn rejects_an_atom_without_a_term_for_every_component() {
        let form = with_form(|form| form.functions[0].atom.clear());

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::AtomCountMismatch {
                path: path(&[Step::Function(0)]),
                expected: 1,
                found: 0
            }
        );
    }

    #[test]
    fn rejects_a_layout_with_the_wrong_number_of_components() {
        let form = with_form(|form| form.components.clear());

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::ComponentCountMismatch {
                path: Path::default(),
                expected: 1,
                found: 0
            }
        );
    }

    #[test]
    fn rejects_a_form_without_a_term_set_for_every_function() {
        let form = with_form(|form| {
            form.functions.pop();
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::FunctionCountMismatch {
                path: Path::default(),
                expected: 2,
                found: 1
            }
        );
    }

    #[test]
    fn rejects_a_carrier_field_of_a_sort_the_language_has_no_field_for() {
        let form = with_form(|form| {
            form.components[0].carrier_fields[0].sort = Sort::Outcome;
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::UnsupportedFieldSort {
                path: path(&[Step::Component(0)]),
                sort: Sort::Outcome
            }
        );
    }

    #[test]
    fn rejects_a_carrier_field_of_an_undeclared_category() {
        let form = with_form(|form| {
            form.components[0].carrier_fields[0].sort = Sort::Cat("phase".to_owned());
        });

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::UnknownCategory {
                path: path(&[Step::Component(0)]),
                name: "phase".to_owned()
            }
        );
    }

    #[test]
    fn accepts_a_carrier_with_a_field_of_every_supported_sort() {
        let table = Term::Table {
            category: "phase".to_owned(),
            entries: vec![phase(1), phase(2), phase(0)],
        };
        let fields = [
            ("i", Sort::Int),
            ("b", Sort::Bool),
            ("o", Sort::Opt),
            ("c", Sort::Cat("phase".to_owned())),
            ("m", Sort::Map("phase".to_owned())),
        ];
        let form = with_form(|form| {
            let component = &mut form.components[0];
            component.carrier_fields = fields
                .iter()
                .map(|(name, sort)| Field {
                    name: (*name).to_owned(),
                    sort: sort.clone(),
                })
                .collect();
            let operands = |operand| {
                Term::Tuple(
                    fields
                        .iter()
                        .map(|(name, _)| field(operand, name))
                        .collect(),
                )
            };
            component.combine = Some(operands(Operand::Left));
            component.apply = Some(Term::Applied);
            for function in &mut form.functions {
                function.atom = vec![Term::Tuple(vec![
                    lit(0),
                    Term::True,
                    Term::Absent,
                    phase(0),
                    table.clone(),
                ])];
            }
        });

        let result = validate(&family_with_phase_category(), &form);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_per_key_over_a_component_that_is_not_keyed() {
        let form = with_form(|form| form.components[0].shape = ComponentShape::PerKey);

        assert_eq!(
            rejection(&Family::quota(), &form),
            Malformed::PerKeyOnUnkeyedComponent {
                path: path(&[Step::Component(0)])
            }
        );
    }
}
