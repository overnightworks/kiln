mod support;

use kiln_e3::{validate, Family, Malformed, Path, Sort, Step};

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
    fn accepts_carrier_fields_of_every_supported_sort() {
        let form = with_form(|form| {
            let fields = &mut form.components[0].carrier_fields;
            fields[0].sort = Sort::Opt;
            fields[1].sort = Sort::Bool;
            fields[2].sort = Sort::Map("phase".to_owned());
            form.components[0].carrier_fields.push(kiln_e3::Field {
                name: "c".to_owned(),
                sort: Sort::Cat("phase".to_owned()),
            });
        });

        let rejected = validate(&family_with_phase_category(), &form).unwrap_err();

        assert!(!matches!(
            rejected,
            Malformed::UnsupportedFieldSort { .. } | Malformed::UnknownCategory { .. }
        ));
    }
}
