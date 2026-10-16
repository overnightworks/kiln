use kiln_e1::{classify_function, Classification, ClassifyError, IndexReadKind, UnknownCause};
use kiln_ir::{Effect, Expr, FunctionFault, FunctionName, ProgramError};

use super::programs::*;

fn line_at(w: &str, d: &str, o: Expr) -> Vec<Expr> {
    vec![param(w), param(d), o]
}

fn read_line(into: u32) -> Effect {
    read_at(LINES, line_at("w", "d", param("o")), into)
}

fn key_not_identified() -> UnknownCause {
    UnknownCause::KeyNotIdentified {
        table: table(LINES),
    }
}

mod kiln_e1_18 {
    use super::*;

    #[test]
    fn a_key_of_three_components_from_arguments_and_literals_is_static() {
        let key = || line_at("w", "d", literal(1));
        let bodies = [
            returns(vec![read_at(LINES, key(), 0)]),
            returns(vec![update_at(LINES, key(), &[])]),
            returns(vec![insert_at(
                LINES,
                line_at("w", "d", plus(param("o"), literal(1))),
            )]),
        ];
        for body in bodies {
            assert_eq!(category_of(body.clone()), STATIC, "{body:?}");
        }
    }

    #[test]
    fn the_key_columns_of_a_point_read_row_are_the_operands_the_read_used() {
        let bodies = [
            returns(vec![read_line(0), update_at(LINES, line_key_of(0), &[])]),
            returns(vec![
                read_line(0),
                insert_at(
                    LINES,
                    vec![
                        column_of(0, "w_id"),
                        column_of(0, "d_id"),
                        plus(column_of(0, "o_id"), literal(1)),
                    ],
                ),
            ]),
        ];
        for body in bodies {
            assert_eq!(category_of(body.clone()), STATIC, "{body:?}");
        }
    }
}

mod kiln_e1_21 {
    use super::*;

    #[test]
    fn a_foreign_key_column_of_a_row_with_a_three_part_key_resolves_one_lookup_in_one_hop() {
        let body = returns(vec![
            read_line(0),
            read("customers", column_of(0, "customer_id"), 1),
        ]);
        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]"]
        );
    }
}

mod kiln_e1_22 {
    use super::*;

    fn after_an_order(key: Vec<Expr>) -> kiln_ir::Block {
        returns(vec![
            read("orders", param("order_id"), 0),
            read_at(LINES, key, 1),
        ])
    }

    #[test]
    fn a_three_part_key_with_a_read_derived_component_and_no_match_is_unknown_naming_the_table() {
        let from_order = |column: &str| column_of(0, column);
        let keys = [
            vec![from_order("customer_id"), param("d"), param("o")],
            vec![param("w"), param("d"), from_order("product_id")],
            vec![
                from_order("customer_id"),
                from_order("product_id"),
                literal(1),
            ],
        ];
        for key in keys {
            assert_unknown(after_an_order(key), key_not_identified());
        }
    }

    #[test]
    fn a_three_part_key_with_an_arithmetic_component_over_a_read_is_unknown_as_computed() {
        let computed = plus(column_of(0, "customer_id"), literal(1));
        assert_unknown(
            after_an_order(vec![param("w"), param("d"), computed]),
            UnknownCause::KeyComputedFromRead {
                table: table(LINES),
            },
        );
    }

    #[test]
    fn a_program_with_a_key_of_the_wrong_length_is_reported_not_classified() {
        let body = returns(vec![read_at(LINES, vec![param("w")], 0)]);
        let function = FunctionName(FUNCTION.to_owned());
        assert_eq!(
            classify_function(&program(body), &function),
            Err(ClassifyError::InvalidProgram(ProgramError::Function {
                function: function.clone(),
                fault: FunctionFault::KeyArity {
                    table: table(LINES),
                    expected: 3,
                    given: 1,
                },
            }))
        );
    }
}

mod kiln_e1_37 {
    use super::*;

    fn lines_of_one_order() -> Effect {
        read_range_at(
            LINES,
            line_at("w", "d", literal(0)),
            line_at("w", "d", literal(9)),
            0,
        )
    }

    fn lines_of_one_item() -> Effect {
        read_index("lines_by_item", vec![param("item")], vec![param("item")], 0)
    }

    fn over_rows_of_slot_zero(key: Vec<Expr>) -> Vec<Effect> {
        vec![iterate(0, 1, vec![update_at(LINES, key, &[])])]
    }

    fn body_of(range: Effect, key: Vec<Expr>) -> kiln_ir::Block {
        let mut effects = vec![range];
        effects.extend(over_rows_of_slot_zero(key));
        returns(effects)
    }

    #[test]
    fn a_key_projected_whole_from_a_row_of_a_range_over_its_table_is_covered_with_no_resolution() {
        let body = body_of(lines_of_one_order(), line_key_of(1));
        assert_eq!(category_of(body.clone()), STATIC);
        assert_eq!(
            evidence_of(body),
            [
                "f/body/effects[0]",
                "f/body/effects[1]/body/effects[0]",
                "f/body/terminal"
            ]
        );
    }

    #[test]
    fn the_same_key_over_a_declared_index_range_of_the_table_is_one_resolution_by_the_index_read() {
        let body = body_of(lines_of_one_item(), line_key_of(1));
        let result = classify(body);
        assert!(matches!(
            result.classification,
            Classification::Proven {
                category: kiln_e1::ProvenCategory::OneHop,
                ..
            }
        ));
        assert_eq!(
            result.index_reads[0].kind,
            IndexReadKind::FollowedByRowAccesses
        );
    }

    #[test]
    fn a_key_not_projected_whole_in_order_from_one_row_is_unknown_naming_the_table() {
        let row = |column: &str| column_of(1, column);
        let other_range = read_range_at(
            LINES,
            line_at("w", "d", literal(0)),
            line_at("w", "d", literal(9)),
            2,
        );
        let swapped = vec![row("d_id"), row("w_id"), row("o_id")];
        let one_static = vec![param("w"), row("d_id"), row("o_id")];
        let two_rows = vec![row("w_id"), row("d_id"), column_of(3, "o_id")];
        let nested = |key| {
            returns(vec![
                lines_of_one_order(),
                other_range.clone(),
                iterate(0, 1, vec![iterate(2, 3, vec![update_at(LINES, key, &[])])]),
            ])
        };
        for key in [swapped, one_static] {
            assert_unknown(body_of(lines_of_one_order(), key), key_not_identified());
        }
        assert_unknown(nested(two_rows), key_not_identified());
    }

    #[test]
    fn a_key_mixing_two_iteration_rows_of_the_same_range_is_unknown_for_primary_and_index_ranges() {
        let mixed_key = vec![
            column_of(1, "w_id"),
            column_of(2, "d_id"),
            column_of(2, "o_id"),
        ];
        for range in [lines_of_one_order(), lines_of_one_item()] {
            let body = returns(vec![
                range,
                iterate(
                    0,
                    1,
                    vec![iterate(
                        0,
                        2,
                        vec![update_at(LINES, mixed_key.clone(), &[])],
                    )],
                ),
            ]);
            assert_unknown(body, key_not_identified());
        }
    }

    #[test]
    fn a_key_projected_from_a_range_over_another_table_with_the_same_key_columns_is_unknown() {
        let archive_ranges = [
            read_range_at(
                ARCHIVE,
                line_at("w", "d", literal(0)),
                line_at("w", "d", literal(9)),
                0,
            ),
            read_index(
                "archive_by_item",
                vec![param("item")],
                vec![param("item")],
                0,
            ),
        ];
        for archive_range in archive_ranges {
            let body = returns(vec![
                archive_range,
                iterate(0, 1, vec![update_at(LINES, line_key_of(1), &[])]),
            ]);
            assert_unknown(body, key_not_identified());
        }
    }

    #[test]
    fn a_key_with_components_of_a_range_row_is_unknown_when_the_range_is_over_another_table() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(
                0,
                1,
                vec![update_at(
                    LINES,
                    vec![column_of(1, "id"), column_of(1, "id"), column_of(1, "id")],
                    &[],
                )],
            ),
        ]);
        assert_unknown(body, key_not_identified());
    }
}
