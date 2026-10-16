//! Declared secondary indexes and indexed range reads, driven through
//! `kiln_ir::run` (`specs/kiln-ir.spec.md`, KILN-IR-65 to KILN-IR-72).

mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns, Block,
    Column, ColumnType, Condition, Effect, Execution, Expr, Index, IndexName, IntegerWidth, Key,
    Outcome, ProgramError, Schema, SchemaError, Slot, Store, Terminal, Value,
};
use support::{
    aborted_reason, call_under_test, column, committed_response, insert_row, integer, keyed_by_id,
    literal_interval, literal_key, one_part, program_with_schema, run_ok, store_of_rows, table,
    text, RowSpec,
};

fn index_name(name: &str) -> IndexName {
    IndexName(name.to_owned())
}

fn people_index(name: &str, columns: &[&str], unique: bool) -> Index {
    Index {
        name: index_name(name),
        table: table("people"),
        columns: columns.iter().map(|name| column(name)).collect(),
        unique,
    }
}

fn people_schema(indexes: Vec<Index>) -> Schema {
    let integer_column = |name: &str| Column {
        name: column(name),
        kind: ColumnType::Integer {
            width: IntegerWidth::I32,
            domain: None,
        },
    };
    Schema {
        tables: vec![
            keyed_by_id(
                "people",
                vec![
                    Column {
                        name: column("last"),
                        kind: ColumnType::Text,
                    },
                    integer_column("age"),
                ],
            ),
            keyed_by_id("visited", vec![]),
        ],
        indexes,
        ..Default::default()
    }
}

fn indexed_schema() -> Schema {
    people_schema(vec![
        people_index("by_last", &["last"], false),
        people_index("by_last_age", &["last", "age"], true),
        people_index("by_age", &["age"], false),
    ])
}

fn person(id: i64, last: &str, age: i64) -> RowSpec {
    (
        "people",
        id,
        vec![("last", text(last)), ("age", integer(age))],
    )
}

fn index_range_read(index: &str, start: Vec<Value>, end: Vec<Value>, bound: i64) -> Effect {
    Effect::ReadIndexRange {
        index: index_name(index),
        start: start.into_iter().map(Expr::Literal).collect(),
        end: end.into_iter().map(Expr::Literal).collect(),
        bound: Expr::Literal(integer(bound)),
        into: Slot(0),
    }
}

fn id_of_row(slot: Slot) -> Expr {
    Expr::Projection {
        slot,
        column: column("id"),
    }
}

fn visit_each_row_into_visited() -> Effect {
    Effect::Iterate {
        source: Slot(0),
        row: Slot(1),
        body: vec![Effect::Insert {
            table: table("visited"),
            key: vec![id_of_row(Slot(1))],
            columns: Default::default(),
        }],
    }
}

fn run_effects(effects: Vec<Effect>, store: &Store) -> Execution {
    let program = program_with_schema(
        indexed_schema(),
        vec![],
        effects,
        Terminal::Return(Expr::Literal(integer(0))),
    );
    run_ok(&program, &call_under_test(vec![]), store)
}

fn people_store(people: &[(i64, &str, i64)]) -> Store {
    let rows: Vec<RowSpec> = people
        .iter()
        .map(|&(id, last, age)| person(id, last, age))
        .collect();
    store_of_rows(&rows)
}

fn visited_ids(execution: &Execution) -> Vec<i64> {
    execution
        .access_log
        .iter()
        .filter(|entry| entry.table == table("visited") && entry.kind == AccessKind::Insert)
        .map(|entry| match &entry.target {
            AccessTarget::Point(Key(components)) => match components.as_slice() {
                [Value::Integer(id)] => *id,
                other => panic!("expected a one-part integer key, got {other:?}"),
            },
            other => panic!("expected an integer point target, got {other:?}"),
        })
        .collect()
}

fn ids_visited_in(index: &str, start: Vec<Value>, end: Vec<Value>, store: &Store) -> Vec<i64> {
    let execution = run_effects(
        vec![
            index_range_read(index, start, end, 10),
            visit_each_row_into_visited(),
        ],
        store,
    );
    visited_ids(&execution)
}

fn branch_on_range_present() -> Terminal {
    let returning = |value| {
        Box::new(Block {
            effects: vec![],
            terminal: Terminal::Return(Expr::Literal(integer(value))),
        })
    };
    Terminal::Branch {
        condition: Condition::RowPresent(Slot(0)),
        when_present: returning(1),
        when_absent: returning(0),
    }
}

mod kiln_ir_65 {
    use super::*;

    fn validated(indexes: Vec<Index>) -> Result<(), SchemaError> {
        people_schema(indexes).validate()
    }

    #[test]
    fn accepts_unique_and_non_unique_indexes_over_integer_text_and_several_columns() {
        let indexes = vec![
            people_index("by_last", &["last"], false),
            people_index("by_age", &["age"], true),
            people_index("by_last_age", &["last", "age"], false),
        ];

        assert_eq!(validated(indexes), Ok(()));
    }

    #[test]
    fn rejects_an_index_on_an_undeclared_table() {
        let index = Index {
            table: table("ghosts"),
            ..people_index("by_last", &["last"], false)
        };

        assert_eq!(
            validated(vec![index.clone()]),
            Err(SchemaError::UnknownIndexTarget(index))
        );
    }

    #[test]
    fn rejects_an_index_on_an_undeclared_column() {
        let index = people_index("by_last_nickname", &["last", "nickname"], false);

        assert_eq!(
            validated(vec![index.clone()]),
            Err(SchemaError::UnknownIndexTarget(index))
        );
    }

    #[test]
    fn rejects_an_index_with_no_column() {
        let index = people_index("by_nothing", &[], false);

        assert_eq!(
            validated(vec![index.clone()]),
            Err(SchemaError::IndexWithoutColumns(index))
        );
    }

    #[test]
    fn rejects_two_indexes_sharing_a_name() {
        let indexes = vec![
            people_index("by_name", &["last"], false),
            people_index("by_name", &["age"], false),
        ];

        assert_eq!(
            validated(indexes),
            Err(SchemaError::DuplicateIndexName(index_name("by_name")))
        );
    }

    #[test]
    fn run_returns_the_schema_error_instead_of_panicking() {
        let schema = people_schema(vec![people_index("by_nothing", &[], false)]);
        let program = program_with_schema(
            schema,
            vec![],
            vec![],
            Terminal::Return(Expr::Literal(integer(0))),
        );

        let result = kiln_ir::run(&program, &call_under_test(vec![]), &Store::empty());

        assert!(matches!(
            result,
            Err(ProgramError::Schema(SchemaError::IndexWithoutColumns(_)))
        ));
    }
}

mod kiln_ir_66 {
    use super::*;

    #[test]
    fn orders_rows_by_indexed_value_then_primary_key_and_returns_every_tie() {
        let store = people_store(&[(5, "b", 1), (2, "b", 1), (9, "a", 1), (1, "c", 1)]);

        let visited = ids_visited_in("by_last", vec![text("a")], vec![text("c")], &store);

        assert_eq!(visited, vec![9, 2, 5, 1]);
    }

    #[test]
    fn includes_both_bounds_and_excludes_rows_outside_them() {
        let store = people_store(&[(1, "a", 1), (2, "b", 1), (3, "c", 1), (4, "d", 1)]);

        let visited = ids_visited_in("by_last", vec![text("b")], vec![text("c")], &store);

        assert_eq!(visited, vec![2, 3]);
    }

    #[test]
    fn orders_by_the_columns_in_declared_order_with_bounds_per_column() {
        let store = people_store(&[(1, "b", 30), (2, "a", 40), (3, "b", 20), (4, "c", 10)]);

        let visited = ids_visited_in(
            "by_last_age",
            vec![text("a"), integer(0)],
            vec![text("b"), integer(30)],
            &store,
        );

        assert_eq!(visited, vec![2, 3, 1]);
    }

    #[test]
    fn leaves_out_a_row_lacking_an_indexed_column() {
        let store = store_of_rows(&[
            person(1, "a", 1),
            ("people", 2, vec![("id", integer(2)), ("last", text("a"))]),
        ]);

        let visited = ids_visited_in(
            "by_age",
            vec![integer(i64::MIN)],
            vec![integer(i64::MAX)],
            &store,
        );

        assert_eq!(visited, vec![1]);
    }
}

mod kiln_ir_67 {
    use super::*;

    #[test]
    fn a_reversed_range_visits_nothing() {
        let store = people_store(&[(1, "a", 1), (2, "b", 1)]);

        let visited = ids_visited_in("by_last", vec![text("b")], vec![text("a")], &store);

        assert_eq!(visited, Vec::<i64>::new());
    }

    #[test]
    fn a_range_matching_nothing_is_branchable_as_an_absence_and_a_match_as_present() {
        let store = people_store(&[(1, "m", 1)]);
        let response_for = |start: &str, end: &str| {
            let program = program_with_schema(
                indexed_schema(),
                vec![],
                vec![index_range_read(
                    "by_last",
                    vec![text(start)],
                    vec![text(end)],
                    10,
                )],
                branch_on_range_present(),
            );
            let execution = run_ok(&program, &call_under_test(vec![]), &store);
            committed_response(&execution).clone()
        };

        assert_eq!(
            (response_for("a", "b"), response_for("a", "z")),
            (integer(0), integer(1))
        );
    }
}

mod kiln_ir_68 {
    use super::*;

    #[test]
    fn sees_a_row_the_transaction_inserted_earlier_in_index_order() {
        let store = people_store(&[(1, "a", 1), (3, "c", 1)]);
        let execution = run_effects(
            vec![
                insert_row("people", 2, [("last", text("b")), ("age", integer(1))]),
                index_range_read("by_last", vec![text("a")], vec![text("c")], 10),
                visit_each_row_into_visited(),
            ],
            &store,
        );

        assert_eq!(visited_ids(&execution), vec![1, 2, 3]);
    }
}

mod kiln_ir_69 {
    use super::*;

    #[test]
    fn aborts_when_more_rows_match_than_the_bound_and_binds_nothing() {
        let store = people_store(&[(1, "a", 1), (2, "a", 1), (3, "a", 1)]);
        let execution = run_effects(
            vec![
                index_range_read("by_last", vec![text("a")], vec![text("a")], 2),
                visit_each_row_into_visited(),
            ],
            &store,
        );

        assert_eq!(
            (aborted_reason(&execution), visited_ids(&execution)),
            (
                &AbortReason::BoundExceeded {
                    table: table("people"),
                    bound: 2,
                    matched: 3,
                },
                vec![]
            )
        );
        assert_eq!(execution.store, store);
    }

    #[test]
    fn commits_when_the_matches_equal_the_bound() {
        let store = people_store(&[(1, "a", 1), (2, "a", 1)]);
        let execution = run_effects(
            vec![index_range_read(
                "by_last",
                vec![text("a")],
                vec![text("a")],
                2,
            )],
            &store,
        );

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
    }
}

mod kiln_ir_70 {
    use super::*;

    fn index_entry(
        index: &str,
        columns: &[&str],
        start: Vec<Value>,
        end: Vec<Value>,
    ) -> AccessLogEntry {
        AccessLogEntry {
            table: table("people"),
            kind: AccessKind::IndexRangeRead,
            target: AccessTarget::IndexRange {
                index: index_name(index),
                start,
                end,
            },
            columns: AccessedColumns::Named(columns.iter().map(|name| column(name)).collect()),
            origin: AccessOrigin::TransactionLogic,
        }
    }

    #[test]
    fn logs_one_entry_naming_the_index_and_its_bounds_for_matching_empty_and_reversed_ranges() {
        let store = people_store(&[(1, "m", 1)]);
        let logged_for = |start: &str, end: &str| {
            run_effects(
                vec![index_range_read(
                    "by_last",
                    vec![text(start)],
                    vec![text(end)],
                    10,
                )],
                &store,
            )
            .access_log
        };

        let expected = |start: &str, end: &str| {
            vec![index_entry(
                "by_last",
                &["last"],
                vec![text(start)],
                vec![text(end)],
            )]
        };
        assert_eq!(logged_for("a", "z"), expected("a", "z"));
        assert_eq!(logged_for("a", "b"), expected("a", "b"));
        assert_eq!(logged_for("z", "a"), expected("z", "a"));
    }

    #[test]
    fn logs_every_bound_of_a_multi_column_index() {
        let execution = run_effects(
            vec![index_range_read(
                "by_last_age",
                vec![text("a"), integer(1)],
                vec![text("b"), integer(2)],
                10,
            )],
            &Store::empty(),
        );

        assert_eq!(
            execution.access_log,
            vec![index_entry(
                "by_last_age",
                &["last", "age"],
                vec![text("a"), integer(1)],
                vec![text("b"), integer(2)],
            )]
        );
    }

    #[test]
    fn keeps_the_entry_when_the_call_aborts_on_the_bound() {
        let store = people_store(&[(1, "a", 1), (2, "a", 1)]);
        let execution = run_effects(
            vec![index_range_read(
                "by_last",
                vec![text("a")],
                vec![text("a")],
                1,
            )],
            &store,
        );

        assert_eq!(
            execution.access_log,
            vec![index_entry(
                "by_last",
                &["last"],
                vec![text("a")],
                vec![text("a")]
            )]
        );
    }
}

mod kiln_ir_71 {
    use super::*;

    fn read_each_row_by_key() -> Effect {
        Effect::Iterate {
            source: Slot(0),
            row: Slot(1),
            body: vec![Effect::Read {
                table: table("people"),
                key: vec![id_of_row(Slot(1))],
                into: Slot(2),
            }],
        }
    }

    fn kinds_of(program_effects: Vec<Effect>, store: &Store) -> Vec<AccessKind> {
        run_effects(program_effects, store)
            .access_log
            .into_iter()
            .map(|entry| entry.kind)
            .collect()
    }

    #[test]
    fn an_index_only_read_leaves_no_point_read() {
        let store = people_store(&[(1, "a", 1), (2, "a", 1)]);

        let kinds = kinds_of(
            vec![index_range_read(
                "by_last",
                vec![text("a")],
                vec![text("a")],
                10,
            )],
            &store,
        );

        assert_eq!(kinds, vec![AccessKind::IndexRangeRead]);
    }

    #[test]
    fn projecting_a_column_outside_the_index_from_an_index_row_aborts_with_missing_column() {
        let store = people_store(&[(1, "a", 30)]);

        let execution = run_effects(
            vec![
                index_range_read("by_last", vec![text("a")], vec![text("a")], 10),
                Effect::Iterate {
                    source: Slot(0),
                    row: Slot(1),
                    body: vec![Effect::Insert {
                        table: table("visited"),
                        key: vec![Expr::Projection {
                            slot: Slot(1),
                            column: column("age"),
                        }],
                        columns: Default::default(),
                    }],
                },
            ],
            &store,
        );

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::MissingColumn {
                slot: Slot(1),
                column: column("age"),
            }
        );
    }

    #[test]
    fn projecting_an_indexed_column_from_an_index_row_yields_its_value() {
        let store = people_store(&[(1, "a", 30)]);

        let execution = run_effects(
            vec![
                index_range_read("by_age", vec![integer(0)], vec![integer(99)], 10),
                Effect::Iterate {
                    source: Slot(0),
                    row: Slot(1),
                    body: vec![Effect::Insert {
                        table: table("visited"),
                        key: vec![Expr::Projection {
                            slot: Slot(1),
                            column: column("age"),
                        }],
                        columns: Default::default(),
                    }],
                },
            ],
            &store,
        );

        assert_eq!(visited_ids(&execution), vec![30]);
    }

    #[test]
    fn following_the_index_to_base_rows_leaves_one_point_read_per_row_after_the_entry() {
        let store = people_store(&[(1, "a", 1), (2, "a", 1)]);

        let execution = run_effects(
            vec![
                index_range_read("by_last", vec![text("a")], vec![text("a")], 10),
                read_each_row_by_key(),
            ],
            &store,
        );

        let kinds: Vec<_> = execution
            .access_log
            .iter()
            .map(|entry| entry.kind.clone())
            .collect();
        let point_keys: Vec<_> = execution
            .access_log
            .iter()
            .filter_map(|entry| match &entry.target {
                AccessTarget::Point(key) => Some(key.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                AccessKind::IndexRangeRead,
                AccessKind::PointRead,
                AccessKind::PointRead
            ]
        );
        assert_eq!(point_keys, vec![one_part(1), one_part(2)]);
    }
}

mod kiln_ir_72 {
    use super::*;

    fn primary_key_of_point_read(key: i64) -> Execution {
        let store = people_store(&[(7, "a", 1)]);
        let program = program_with_schema(
            indexed_schema(),
            vec![],
            vec![Effect::Read {
                table: table("people"),
                key: literal_key(key),
                into: Slot(0),
            }],
            Terminal::Return(id_of_row(Slot(0))),
        );
        run_ok(&program, &call_under_test(vec![]), &store)
    }

    #[test]
    fn a_point_read_slot_yields_the_key_the_read_used() {
        let execution = primary_key_of_point_read(7);

        assert_eq!(committed_response(&execution), &integer(7));
    }

    #[test]
    fn a_point_read_that_found_no_row_aborts_with_projection_source_absent() {
        let execution = primary_key_of_point_read(8);

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::ProjectionSourceAbsent { slot: Slot(0) }
        );
    }

    #[test]
    fn a_primary_key_range_row_yields_its_store_key() {
        let store = people_store(&[(3, "a", 1), (1, "b", 2), (2, "c", 3)]);

        let execution = run_effects(
            vec![
                Effect::ReadRange {
                    table: table("people"),
                    prefix: vec![],
                    interval: literal_interval(1, 3),
                    bound: Expr::Literal(integer(10)),
                    into: Slot(0),
                },
                visit_each_row_into_visited(),
            ],
            &store,
        );

        assert_eq!(visited_ids(&execution), vec![1, 2, 3]);
    }

    #[test]
    fn an_index_row_yields_its_store_key() {
        let store = people_store(&[(5, "b", 1), (4, "a", 2)]);

        let ids = ids_visited_in("by_last", vec![text("a")], vec![text("z")], &store);

        assert_eq!(ids, vec![4, 5]);
    }

    #[test]
    fn a_slot_no_read_has_bound_aborts_with_projection_source_absent() {
        let program = program_with_schema(
            indexed_schema(),
            vec![],
            vec![],
            Terminal::Return(id_of_row(Slot(4))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &Store::empty());

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::ProjectionSourceAbsent { slot: Slot(4) }
        );
    }
}
