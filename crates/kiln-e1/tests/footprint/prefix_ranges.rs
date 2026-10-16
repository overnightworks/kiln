use kiln_e1::{Classification, UnknownCause};
use kiln_ir::{
    Column, ColumnName, ColumnType, Effect, Expr, Function, FunctionName, IntegerWidth,
    KeyDeclaration, Program, Schema, Table, Value,
};

use super::keys::kiln_e1_27::{classification_of, table_keyed_by};
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

    #[test]
    fn an_interval_bound_of_the_wrong_kind_for_the_column_after_the_prefix_ends_the_path_before_the_range(
    ) {
        let text = || Expr::Literal(Value::Text("t".to_owned()));
        let from_slot = || column_of(2, "c");
        for interval in [(from_slot(), text()), (text(), from_slot())] {
            let program = interval_bounds_from_slot_two("t2", interval);
            let Classification::Unknown { evidence, .. } = classification_of(&program) else {
                panic!("a text interval bound taken from an integer column must not classify");
            };
            assert_eq!(
                evidence.unwrap().paths(),
                [
                    "f/body/effects[0]",
                    "f/body/terminal/when_present/effects[0]",
                    "f/body/terminal/when_present/effects[1]",
                    "f/body/terminal/when_present/effects[2]",
                    "f/body/terminal/when_absent/effects[0]",
                    "f/body/terminal/when_absent/effects[1]"
                ]
            );
        }
    }

    #[test]
    fn an_interval_bound_of_the_right_kind_for_the_column_after_the_prefix_keeps_the_range() {
        let text = || Expr::Literal(Value::Text("t".to_owned()));
        let from_slot = || column_of(2, "c");
        for interval in [(from_slot(), text()), (text(), from_slot())] {
            let program = interval_bounds_from_slot_two("t1", interval);
            let Classification::Proven { evidence, .. } = classification_of(&program) else {
                panic!("a text interval bound taken from a text column classifies");
            };
            assert_eq!(
                evidence.paths(),
                [
                    "f/body/effects[0]",
                    "f/body/terminal/when_present/effects[0]",
                    "f/body/terminal/when_present/effects[1]",
                    "f/body/terminal/when_present/effects[2]",
                    "f/body/terminal/when_present/effects[3]",
                    "f/body/terminal/when_present/effects[4]",
                    "f/body/terminal/when_absent/effects[0]",
                    "f/body/terminal/when_absent/effects[1]",
                    "f/body/terminal/when_present/terminal",
                    "f/body/terminal/when_absent/terminal"
                ]
            );
        }
    }
}

/// `pairs` has an integer first and a text second key column, so a prefix of
/// one integer is followed by an interval over text.
fn pairs_table() -> Table {
    let column = |name: &str, kind| Column {
        name: ColumnName(name.to_owned()),
        kind,
    };
    let (first, second) = (column("first", INTEGER), column("second", ColumnType::Text));
    Table {
        name: table("pairs"),
        key: KeyDeclaration::Columns(vec![first.name.clone(), second.name.clone()]),
        columns: vec![first, second],
    }
}

const INTEGER: ColumnType = ColumnType::Integer {
    width: IntegerWidth::I64,
    domain: None,
};

/// Slot 2 is read from `t1`, keyed by text, on one arm and from `t2`, keyed by
/// an integer, on the other, so validation cannot tell what its column `c`
/// is; on the present arm it is read from `bound_on_path`. The present arm then
/// reads `pairs` below the prefix `(1)` with the interval `(from, to)`, and
/// afterwards `b`.
fn interval_bounds_from_slot_two(bound_on_path: &str, interval: (Expr, Expr)) -> Program {
    let plain = |name| table_keyed_by(name, "k", INTEGER);
    let reading = |name: &str, into| match name {
        "t1" => read("t1", Expr::Literal(Value::Text("k".to_owned())), into),
        _ => read("t2", literal(1), into),
    };
    let other = if bound_on_path == "t1" { "t2" } else { "t1" };
    let present = returns(vec![
        read("a", literal(1), 1),
        reading(bound_on_path, 2),
        reading(other, 5),
        read_prefix_range("pairs", vec![literal(1)], Some(interval), 6),
        read("b", literal(1), 4),
    ]);
    let absent = returns(vec![reading(other, 2), read("b", literal(1), 4)]);
    Program {
        schema: Schema {
            tables: vec![
                plain("sel"),
                plain("a"),
                plain("b"),
                pairs_table(),
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
