use kiln_e1::UnknownCause;
use kiln_ir::Effect;

use super::programs::*;

/// A district's orders of `LINES`: the prefix `(w, d)`, the interval on `o_id`.
fn lines_of_one_district() -> Effect {
    read_prefix_range(
        LINES,
        vec![param("w"), param("d")],
        Some((literal(0), literal(9))),
        0,
    )
}

fn archive_at(key: Vec<kiln_ir::Expr>) -> Effect {
    read_at(ARCHIVE, key, 2)
}

mod kiln_e1_42 {
    use super::*;

    #[test]
    fn a_prefix_and_interval_from_arguments_and_literals_are_static_also_without_an_interval() {
        let ranges = [
            lines_of_one_district(),
            read_prefix_range(LINES, vec![], None, 0),
            read_prefix_range(LINES, vec![param("w")], None, 0),
            read_prefix_range(
                LINES,
                vec![plus(param("w"), literal(1))],
                Some((param("low"), param("high"))),
                0,
            ),
        ];
        for range in ranges {
            let body = returns(vec![range]);
            assert_eq!(category_of(body.clone()), STATIC, "{body:?}");
        }
    }

    #[test]
    fn a_prefix_or_interval_from_a_read_result_is_unknown_also_when_computed() {
        let from_read = || column_of(3, "customer_id");
        let computed = || plus(from_read(), literal(1));
        let ranges = [
            read_prefix_range(LINES, vec![from_read()], None, 0),
            read_prefix_range(LINES, vec![computed()], None, 0),
            read_prefix_range(LINES, vec![param("w")], Some((from_read(), literal(9))), 0),
            read_prefix_range(LINES, vec![param("w")], Some((literal(0), computed())), 0),
        ];
        for range in ranges {
            let body = returns(vec![read("orders", param("id"), 3), range]);
            assert_unknown(body, UnknownCause::PrimaryRangeBoundedByRead);
        }
    }
}

mod kiln_e1_43 {
    use super::*;

    fn routed_example(parent_key: Vec<kiln_ir::Expr>) -> kiln_ir::Block {
        returns(vec![
            lines_of_one_district(),
            iterate(0, 1, vec![archive_at(parent_key)]),
        ])
    }

    fn parent_at_district_and(component: kiln_ir::Expr) -> Vec<kiln_ir::Expr> {
        vec![param("w"), param("d"), component]
    }

    #[test]
    fn a_composite_foreign_key_matched_through_the_prefix_is_one_resolution_by_the_range_read() {
        let body = routed_example(parent_at_district_and(column_of(1, "customer_id")));

        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]/body/effects[0]"]
        );
    }

    #[test]
    fn a_computed_static_prefix_aliases_like_any_other() {
        let range = read_prefix_range(
            LINES,
            vec![plus(param("w"), literal(1)), param("d")],
            Some((literal(0), literal(9))),
            0,
        );
        let parent_key = |warehouse| {
            returns(vec![
                range.clone(),
                iterate(
                    0,
                    1,
                    vec![archive_at(vec![
                        warehouse,
                        param("d"),
                        column_of(1, "customer_id"),
                    ])],
                ),
            ])
        };

        assert_eq!(
            category_of(parent_key(plus(param("w"), literal(1)))),
            ONE_HOP
        );
        assert_unknown(
            parent_key(param("w")),
            UnknownCause::KeyNotIdentified {
                table: table(ARCHIVE),
            },
        );
    }

    #[test]
    fn a_key_column_the_prefix_leaves_open_is_no_alias() {
        let one_component_prefix =
            read_prefix_range(LINES, vec![param("w")], Some((literal(0), literal(9))), 0);
        let body = returns(vec![
            one_component_prefix,
            iterate(
                0,
                1,
                vec![archive_at(parent_at_district_and(column_of(
                    1,
                    "customer_id",
                )))],
            ),
        ]);

        assert_unknown(
            body,
            UnknownCause::KeyNotIdentified {
                table: table(ARCHIVE),
            },
        );
    }

    #[test]
    fn a_prefix_read_from_a_result_gives_no_alias() {
        let read_prefix = read_prefix_range(
            LINES,
            vec![column_of(3, "customer_id"), param("d")],
            None,
            0,
        );
        let body = returns(vec![
            read("orders", param("id"), 3),
            read_prefix,
            iterate(
                0,
                1,
                vec![archive_at(vec![
                    column_of(3, "customer_id"),
                    param("d"),
                    column_of(1, "customer_id"),
                ])],
            ),
        ]);

        assert_unknown(body, UnknownCause::PrimaryRangeBoundedByRead);
    }

    #[test]
    fn a_key_of_the_ranges_table_taking_the_prefix_operands_and_one_rows_other_columns_is_covered()
    {
        let key = |district| vec![param("w"), district, column_of(1, "o_id")];
        let over_rows = |district| {
            returns(vec![
                lines_of_one_district(),
                iterate(0, 1, vec![update_at(LINES, key(district), &[])]),
            ])
        };

        assert_eq!(category_of(over_rows(param("d"))), STATIC);
        assert_unknown(
            over_rows(param("other_district")),
            UnknownCause::KeyNotIdentified {
                table: table(LINES),
            },
        );
    }

    #[test]
    fn the_alias_ends_with_the_iteration_that_yields_the_row() {
        let after_the_loop = returns(vec![
            lines_of_one_district(),
            iterate(0, 1, vec![]),
            archive_at(parent_at_district_and(column_of(1, "customer_id"))),
        ]);

        // The evaluator aborts at the read of a released row, so the path ends
        // there as a static one instead of resolving through the range.
        assert_eq!(category_of(after_the_loop.clone()), STATIC);
        assert_eq!(
            evidence_of(after_the_loop),
            ["f/body/effects[0]", "f/body/effects[2]"]
        );
    }
}
