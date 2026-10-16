use kiln_e1::{classify_function, Classification, ClassifyError, IndexReadKind, UnknownCause};
use kiln_ir::{
    Column, ColumnName, ColumnType, Effect, Expr, Function, FunctionFault, FunctionName,
    IntegerWidth, KeyComponentFault, KeyDeclaration, Program, ProgramError, Schema, Table, Value,
};

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

    fn lines_of_one_warehouse() -> Effect {
        read_prefix_range(LINES, vec![param("w")], Some((literal(0), literal(9))), 0)
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
        let body = body_of(lines_of_one_warehouse(), line_key_of(1));
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
        let other_range =
            read_prefix_range(LINES, vec![param("w")], Some((literal(0), literal(9))), 2);
        let swapped = vec![row("d_id"), row("w_id"), row("o_id")];
        let one_static = vec![row("w_id"), param("d"), row("o_id")];
        let two_rows = vec![row("w_id"), row("d_id"), column_of(3, "o_id")];
        let nested = |key| {
            returns(vec![
                lines_of_one_warehouse(),
                other_range.clone(),
                iterate(0, 1, vec![iterate(2, 3, vec![update_at(LINES, key, &[])])]),
            ])
        };
        for key in [swapped, one_static] {
            assert_unknown(body_of(lines_of_one_warehouse(), key), key_not_identified());
        }
        assert_unknown(nested(two_rows), key_not_identified());
    }

    #[test]
    fn a_key_mixing_two_iteration_rows_of_the_same_range_is_unknown_for_primary_and_index_ranges() {
        let mixed_key = vec![
            column_of(1, "w_id"),
            column_of(1, "d_id"),
            column_of(2, "o_id"),
        ];
        for range in [lines_of_one_warehouse(), lines_of_one_item()] {
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
    fn a_key_mixing_the_rows_of_two_iterations_that_reuse_one_row_slot_is_unknown() {
        let outer_read_into_slot_3 = read_at(LINES, line_key_of(1), 3);
        let mixed_key = vec![
            column_of(1, "w_id"),
            column_of(3, "d_id"),
            column_of(1, "o_id"),
        ];
        for range in [lines_of_one_warehouse(), lines_of_one_item()] {
            let body = returns(vec![
                range,
                iterate(
                    0,
                    1,
                    vec![
                        outer_read_into_slot_3.clone(),
                        iterate(0, 1, vec![update_at(LINES, mixed_key.clone(), &[])]),
                    ],
                ),
            ]);
            assert_unknown(body, key_not_identified());
        }
    }

    #[test]
    fn a_row_slot_bound_again_by_a_later_read_names_that_read_s_row_not_the_range_s() {
        let body = returns(vec![
            lines_of_one_warehouse(),
            iterate(0, 1, vec![]),
            read(LINE_REFS, param("id"), 1),
            read_at(LINES, line_key_of(1), 2),
        ]);
        assert_unknown(body, key_not_identified());
    }

    #[test]
    fn arms_that_each_access_every_row_of_a_range_by_its_full_key_reach_the_same_targets() {
        let each_row_in_slot = |row: u32| {
            returns(vec![iterate(
                0,
                row,
                vec![update_at(LINES, line_key_of(row), &[])],
            )])
        };
        let expected = [
            (lines_of_one_warehouse(), STATIC),
            (lines_of_one_item(), ONE_HOP),
        ];
        for (range, category) in expected {
            let body = branch_on_presence(
                vec![range, read("accounts", param("id"), 5)],
                5,
                each_row_in_slot(1),
                each_row_in_slot(2),
            );
            assert_eq!(category_of(body.clone()), category, "{body:?}");
        }
    }

    #[test]
    fn a_key_projected_from_a_range_over_another_table_with_the_same_key_columns_is_unknown() {
        let archive_ranges = [
            read_prefix_range(
                ARCHIVE,
                vec![param("w"), param("d")],
                Some((literal(0), literal(9))),
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

mod kiln_e1_19 {
    use super::*;

    const SELECTOR: &str = "f/body/effects[1]";

    fn lines_of_one_warehouse() -> Effect {
        read_prefix_range(LINES, vec![param("w")], Some((literal(0), literal(9))), 0)
    }

    /// A branch on a data read whose arms run over the rows of one range read
    /// before it.
    fn arms_over_one_range(present: Vec<Effect>, absent: Vec<Effect>) -> kiln_ir::Block {
        branch_on_presence(
            vec![lines_of_one_warehouse(), read("accounts", param("id"), 5)],
            5,
            returns(present),
            returns(absent),
        )
    }

    /// Every pair of rows of the range, the row in `outer` iterated around the
    /// row in `inner`, archived under `key`.
    fn every_pair(outer: u32, inner: u32, key: Vec<Expr>) -> Vec<Effect> {
        vec![iterate(
            0,
            outer,
            vec![iterate(0, inner, vec![insert_at(ARCHIVE, key)])],
        )]
    }

    fn archive_key(w: u32, d: u32, o: u32) -> Vec<Expr> {
        vec![
            column_of(w, "w_id"),
            column_of(d, "d_id"),
            column_of(o, "o_id"),
        ]
    }

    #[test]
    fn arms_pairing_the_components_of_two_rows_differently_reach_incomparable_targets() {
        let body = arms_over_one_range(
            every_pair(1, 2, archive_key(1, 1, 2)),
            every_pair(1, 2, archive_key(1, 2, 2)),
        );
        assert_unknown(
            body,
            UnknownCause::DataSelectedTargets {
                selector: SELECTOR.to_owned(),
            },
        );
    }

    #[test]
    fn arms_nesting_two_iterations_the_other_way_round_around_one_key_reach_the_same_targets() {
        let body = arms_over_one_range(
            every_pair(1, 2, archive_key(1, 1, 2)),
            every_pair(2, 1, archive_key(1, 1, 2)),
        );
        assert_eq!(category_of(body), ONE_HOP);
    }

    #[test]
    fn arms_reaching_the_same_targets_through_iterations_in_another_order_count_as_unknown() {
        let lines_then_archive = vec![
            iterate(0, 1, vec![update_at(LINES, line_key_of(1), &[])]),
            iterate(0, 2, vec![insert_at(ARCHIVE, line_key_of(2))]),
        ];
        let archive_then_lines = vec![
            iterate(0, 1, vec![insert_at(ARCHIVE, line_key_of(1))]),
            iterate(0, 2, vec![update_at(LINES, line_key_of(2), &[])]),
        ];
        assert_unknown(
            arms_over_one_range(lines_then_archive, archive_then_lines),
            UnknownCause::DataSelectedTargets {
                selector: SELECTOR.to_owned(),
            },
        );
    }
}

pub(super) mod kiln_e1_27 {
    use super::*;

    #[test]
    fn a_program_with_a_key_component_of_the_wrong_kind_is_reported_not_classified() {
        let k = || literal(1);
        let text_key = || Expr::Literal(Value::Text("text".to_owned()));
        let present = returns(vec![
            read("a", k(), 1),
            read("audit", text_key(), 2),
            read("b", k(), 3),
        ]);
        let absent = returns(vec![read("b", k(), 3)]);
        let body = branch_on_presence(vec![read("accounts", k(), 0)], 0, present, absent);
        let function = FunctionName(FUNCTION.to_owned());
        assert_eq!(
            classify_function(&program(body), &function),
            Err(ClassifyError::InvalidProgram(ProgramError::Function {
                function: function.clone(),
                fault: FunctionFault::KeyComponent {
                    table: table("audit"),
                    column: ColumnName(KEY_COLUMN.to_owned()),
                    fault: KeyComponentFault::WrongKind,
                },
            }))
        );
    }

    const INTEGER: ColumnType = ColumnType::Integer {
        width: IntegerWidth::I64,
        domain: None,
    };

    pub(crate) fn table_keyed_by(name: &str, key: &str, kind: ColumnType) -> Table {
        let key = ColumnName(key.to_owned());
        Table {
            name: table(name),
            columns: vec![Column {
                name: key.clone(),
                kind,
            }],
            key: KeyDeclaration::Columns(vec![key]),
        }
    }

    fn text_key() -> Expr {
        Expr::Literal(Value::Text("k".to_owned()))
    }

    pub(crate) fn point_read_of_u(key: Expr) -> Effect {
        read("u", key, 3)
    }

    fn range_over_u_starting_at(key: Expr) -> Effect {
        read_range("u", key, literal(1), 3)
    }

    /// Slot 2 is read from `t1`, keyed by text, on one arm and from `t2`,
    /// keyed by an integer, on the other (the present arm reads it too, into another slot, so the
    /// arms' accesses stay ordered by inclusion), so no single table says what a
    /// projection of it is; the present arm reads `bound_on_path` into it. The
    /// key of `u` is the integer `id`; `c`, the key of `t1` and `t2`, is what
    /// the slot's row holds. `access` takes `key` after both arms' reads.
    pub(crate) fn slot_bound_by_either_arm(
        bound_on_path: &str,
        key: Expr,
        access: fn(Expr) -> Effect,
    ) -> Program {
        let plain = |name| table_keyed_by(name, KEY_COLUMN, INTEGER);
        let reading = |name: &str, into| match name {
            "t1" => read("t1", text_key(), into),
            _ => read("t2", literal(1), into),
        };
        let other = if bound_on_path == "t1" { "t2" } else { "t1" };
        let present = returns(vec![
            read("a", literal(1), 1),
            reading(bound_on_path, 2),
            reading(other, 5),
            access(key),
            read("b", literal(1), 4),
        ]);
        let absent = returns(vec![reading(other, 2), read("b", literal(1), 4)]);
        Program {
            schema: Schema {
                tables: vec![
                    plain("sel"),
                    plain("a"),
                    plain("b"),
                    plain("u"),
                    table_keyed_by("t1", "c", ColumnType::Text),
                    table_keyed_by("t2", "c", INTEGER),
                ],
                ..Schema::default()
            },
            functions: vec![Function {
                name: FunctionName(FUNCTION.to_owned()),
                parameters: Vec::new(),
                body: branch_on_presence(vec![read("sel", literal(1), 0)], 0, present, absent),
            }],
        }
    }

    pub(crate) fn classification_of(program: &Program) -> Classification {
        let function = FunctionName(FUNCTION.to_owned());
        classify_function(program, &function)
            .unwrap()
            .classification
    }

    #[test]
    fn a_key_projecting_a_column_of_the_wrong_kind_from_a_slot_two_arms_bind_ends_the_path_before_its_access(
    ) {
        for keyed_by_slot_2 in [point_read_of_u, range_over_u_starting_at] {
            let program = slot_bound_by_either_arm("t1", column_of(2, "c"), keyed_by_slot_2);
            let classification = classification_of(&program);
            assert!(
                matches!(classification, Classification::Unknown { .. }),
                "{classification:?}"
            );
        }
    }

    #[test]
    fn a_key_projecting_a_column_of_the_right_kind_from_a_slot_two_arms_bind_keeps_its_access() {
        for keyed_by_slot_2 in [point_read_of_u, range_over_u_starting_at] {
            let program = slot_bound_by_either_arm("t2", column_of(2, "c"), keyed_by_slot_2);
            let classification = classification_of(&program);
            assert!(
                matches!(classification, Classification::Proven { .. }),
                "{classification:?}"
            );
        }
    }
}

mod kiln_e1_40 {
    use super::*;

    fn read_shipment() -> Effect {
        read(SHIPMENTS, param("id"), 0)
    }

    fn from_shipment(columns: [&str; 3]) -> Vec<Expr> {
        columns.map(|column| column_of(0, column)).to_vec()
    }

    #[test]
    fn a_whole_key_projecting_the_columns_of_a_declared_foreign_key_in_order_is_one_resolution() {
        let body = returns(vec![
            read_shipment(),
            read_at(LINES, from_shipment(LINE_KEY), 1),
        ]);

        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]"]
        );
    }

    #[test]
    fn components_equal_to_the_read_row_s_key_columns_by_an_alias_count_as_those_columns() {
        let body = returns(vec![
            read_line(0),
            read_at(
                ARCHIVE,
                vec![param("w"), param("d"), column_of(0, "customer_id")],
                1,
            ),
        ]);

        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]"]
        );
    }

    #[test]
    fn a_key_column_of_a_row_read_at_a_read_derived_key_is_that_key_s_operand() {
        let body = returns(vec![
            read_order(),
            read_customer_of_order(1),
            read("customers", key_of(1), 2),
        ]);

        assert_eq!(category_of(body), ONE_HOP);
    }

    #[test]
    fn a_component_not_provably_equal_to_its_foreign_key_column_leaves_the_key_unidentified() {
        let line_row = |column: &str| column_of(0, column);
        let inventory_of =
            |warehouse: Expr| read_at(INVENTORY, vec![warehouse, line_row("item_id")], 1);
        let matching = returns(vec![read_line(0), inventory_of(line_row("customer_id"))]);
        let unrelated = returns(vec![read_line(0), inventory_of(param("w"))]);

        assert_eq!(category_of(matching), ONE_HOP);
        assert_unknown(
            unrelated,
            UnknownCause::KeyNotIdentified {
                table: table(INVENTORY),
            },
        );
    }

    #[test]
    fn a_partial_or_reordered_match_of_a_foreign_key_is_unknown_naming_the_table() {
        let lines_keyed = |key: Vec<Expr>| returns(vec![read_shipment(), read_at(LINES, key, 1)]);
        let from_row = |columns: [&str; 3]| from_shipment(columns);
        let keys = [
            vec![column_of(0, "w_id"), column_of(0, "d_id"), param("o")],
            vec![column_of(0, "w_id"), column_of(0, "d_id"), literal(1)],
            from_row(["d_id", "w_id", "o_id"]),
            from_row(["w_id", "d_id", "w_id"]),
        ];

        for key in keys {
            assert_unknown(lines_keyed(key), key_not_identified());
        }
    }

    #[test]
    fn components_from_two_different_rows_are_no_foreign_key_match() {
        let body = returns(vec![
            read(SHIPMENTS, param("first"), 0),
            read(SHIPMENTS, param("second"), 1),
            read_at(
                LINES,
                vec![
                    column_of(0, "w_id"),
                    column_of(0, "d_id"),
                    column_of(1, "o_id"),
                ],
                2,
            ),
        ]);

        assert_unknown(body, key_not_identified());
    }
}
