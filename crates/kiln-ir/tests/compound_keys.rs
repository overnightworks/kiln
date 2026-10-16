//! Compound keys (`docs/concept.md` §2.2 D8) driven through `kiln_ir::run`
//! (`specs/kiln-ir.spec.md`, KILN-IR-76 to KILN-IR-78 and the criteria D8
//! changes): declaring, reading, writing and projecting keys.
mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessTarget, AccessedColumns, Block, Effect, Expr, Index, IndexName,
    Key, KeyColumn, KeyDeclaration, Outcome, Schema, SchemaError, Slot, Store, Table, Terminal,
    UniqueConstraint,
};
use support::world::*;
use support::{
    aborted_reason, call_under_test, column, columns_of, columns_of_values, committed_response,
    integer, run_ok, table, text,
};

mod kiln_ir_76 {
    use super::*;

    fn schema_keyed_by(key: KeyDeclaration) -> Schema {
        Schema {
            tables: vec![Table {
                key,
                ..lines_table("lines")
            }],
            ..Default::default()
        }
    }

    fn unlocated(column_name: &str) -> KeyColumn {
        KeyColumn {
            table: table("lines"),
            column: column(column_name),
        }
    }

    #[test]
    fn a_key_is_an_ordered_non_empty_list_of_the_tables_own_columns_or_keyless() {
        let cases = [
            (key_of(&["w", "d", "o"]), Ok(())),
            (key_of(&["o", "w"]), Ok(())),
            (KeyDeclaration::Keyless, Ok(())),
            (
                KeyDeclaration::Columns(vec![]),
                Err(SchemaError::EmptyKey(table("lines"))),
            ),
            (
                key_of(&["w", "nope"]),
                Err(SchemaError::UnknownKeyColumn(unlocated("nope"))),
            ),
            (
                key_of(&["w", "d", "w"]),
                Err(SchemaError::RepeatedKeyColumn(unlocated("w"))),
            ),
        ];

        for (key, expected) in cases {
            assert_eq!(schema_keyed_by(key).validate(), expected);
        }
    }

    #[test]
    fn an_index_on_a_keyless_table_is_rejected() {
        let index = Index {
            name: IndexName("by_what".to_owned()),
            table: table("events"),
            columns: vec![column("what")],
            unique: false,
        };
        let schema = Schema {
            indexes: vec![index.clone()],
            ..world_schema()
        };

        assert_eq!(
            schema.validate(),
            Err(SchemaError::IndexOnKeylessTable(index))
        );
    }

    #[test]
    fn constraints_may_name_a_keyless_table() {
        let schema = Schema {
            unique_constraints: vec![UniqueConstraint {
                table: table("events"),
                columns: vec![column("what")],
            }],
            ..world_schema()
        };

        assert_eq!(schema.validate(), Ok(()));
    }
}

mod kiln_ir_77 {
    use super::*;

    #[test]
    fn insert_read_update_and_delete_by_a_three_component_key() {
        let respond_with_qty = Terminal::Return(project(Slot(0), "qty"));
        let program = |effects| world_program(effects, respond_with_qty.clone());

        let inserted = run_ok(
            &program(vec![insert_line(1, 2, 3, 7), read_line(1, 2, 3, Slot(0))]),
            &call_under_test(vec![]),
            &Store::empty(),
        );
        let updated = run_ok(
            &program(vec![
                update_line_qty(1, 2, 3, 8),
                read_line(1, 2, 3, Slot(0)),
            ]),
            &call_under_test(vec![]),
            &inserted.store,
        );
        let deleted = run_world(vec![delete_line(1, 2, 3)], &updated.store);

        assert_eq!(committed_response(&inserted), &integer(7));
        assert_eq!(committed_response(&updated), &integer(8));
        assert_eq!(deleted.store, Store::empty());
    }

    #[test]
    fn a_row_is_found_only_by_its_whole_key() {
        let store = lines_store(&[(1, 2, 3, 7)]);
        let near_misses = [(0, 2, 3), (1, 0, 3), (1, 2, 0)];

        for (w, d, o) in near_misses {
            let read = run_ok(
                &world_program(
                    vec![read_line(w, d, o, Slot(0))],
                    Terminal::Branch {
                        condition: kiln_ir::Condition::RowPresent(Slot(0)),
                        when_present: Box::new(Block {
                            effects: vec![],
                            terminal: Terminal::Return(literal(1)),
                        }),
                        when_absent: Box::new(Block {
                            effects: vec![],
                            terminal: returning_zero(),
                        }),
                    },
                ),
                &call_under_test(vec![]),
                &store,
            );
            let update = run_world(vec![update_line_qty(w, d, o, 1)], &store);
            let delete = run_world(vec![delete_line(w, d, o)], &store);
            let insert = run_world(vec![insert_line(w, d, o, 1)], &store);

            assert_eq!(committed_response(&read), &integer(0));
            for missing in [&update, &delete] {
                assert_eq!(
                    aborted_reason(missing),
                    &AbortReason::MissingRow {
                        table: table("lines"),
                        key: line_key(w, d, o)
                    }
                );
            }
            assert_eq!(insert.store, lines_store(&[(1, 2, 3, 7), (w, d, o, 1)]));
        }
    }

    #[test]
    fn a_text_keyed_table_is_read_and_written_by_its_text_key() {
        let store = Store::empty();
        let insert = Effect::Insert {
            table: table("labels"),
            key: vec![Expr::Literal(text("alpha"))],
            columns: columns_of([("note", Expr::Literal(text("first")))]),
        };

        let execution = run_world(vec![insert], &store);

        let row = execution
            .store
            .row(&table("labels"), &Key(vec![text("alpha")]))
            .expect("the row is stored under its text key");
        assert_eq!(row.columns, columns_of_values([("note", text("first"))]));
    }
}

mod kiln_ir_18 {
    use super::*;

    #[test]
    fn inserting_a_key_equal_in_every_component_aborts_and_one_differing_in_a_component_commits() {
        let store = lines_store(&[(1, 2, 3, 7)]);

        let duplicate = run_world(vec![insert_line(1, 2, 3, 9)], &store);
        let neighbour = run_world(vec![insert_line(1, 2, 4, 9)], &store);

        assert_eq!(
            aborted_reason(&duplicate),
            &AbortReason::DuplicateKey {
                table: table("lines"),
                key: line_key(1, 2, 3)
            }
        );
        assert_eq!(duplicate.store, store);
        assert_eq!(neighbour.store, lines_store(&[(1, 2, 3, 7), (1, 2, 4, 9)]));
    }
}

mod kiln_ir_09 {
    use super::*;

    #[test]
    fn a_range_between_compound_keys_is_lexicographic_not_componentwise() {
        let all = [
            (1, 2, 4, 0),
            (1, 9, 0, 0),
            (2, 0, 0, 0),
            (1, 2, 5, 0),
            (1, 3, 1, 0),
            (2, 0, 1, 0),
            (0, 9, 9, 0),
        ];
        let store = lines_store(&all);
        let execution = run_world(
            vec![
                read_lines_range((1, 2, 5), (2, 0, 0), Slot(0)),
                copy_keys_into_seen(Slot(0)),
            ],
            &store,
        );

        assert_eq!(
            keys_in_seen(&execution),
            vec![
                line_key(1, 2, 5),
                line_key(1, 3, 1),
                line_key(1, 9, 0),
                line_key(2, 0, 0)
            ]
        );
    }
}

mod kiln_ir_35 {
    use super::*;

    #[test]
    fn entries_carry_every_key_component_and_an_insert_names_only_its_non_key_columns() {
        let store = lines_store(&[(1, 2, 3, 7)]);
        let execution = run_world(
            vec![
                read_line(1, 2, 3, Slot(0)),
                read_lines_range((1, 2, 3), (1, 2, 9), Slot(1)),
                insert_line(5, 6, 7, 1),
            ],
            &store,
        );

        let logged: Vec<_> = execution
            .access_log
            .iter()
            .map(|entry| {
                (
                    entry.kind.clone(),
                    entry.target.clone(),
                    entry.columns.clone(),
                )
            })
            .collect();
        assert_eq!(
            logged,
            vec![
                (
                    AccessKind::PointRead,
                    AccessTarget::Point(line_key(1, 2, 3)),
                    AccessedColumns::EntireRow
                ),
                (
                    AccessKind::RangeRead,
                    AccessTarget::Range {
                        start: line_key(1, 2, 3),
                        end: line_key(1, 2, 9)
                    },
                    AccessedColumns::EntireRow
                ),
                (
                    AccessKind::Insert,
                    AccessTarget::Point(line_key(5, 6, 7)),
                    AccessedColumns::Named(vec![column("qty")])
                ),
            ]
        );
    }
}

mod kiln_ir_78 {
    use super::*;

    #[test]
    fn a_stored_row_holds_no_key_column() {
        let execution = run_world(vec![insert_line(1, 2, 3, 7)], &Store::empty());

        let row = execution
            .store
            .row(&table("lines"), &line_key(1, 2, 3))
            .expect("the inserted row");
        assert_eq!(row.columns, columns_of_values([("qty", integer(7))]));
    }

    #[test]
    fn each_key_column_projects_its_component_of_a_point_read_row() {
        let store = lines_store(&[(4, 5, 6, 7)]);

        for (name, expected) in [("w", 4), ("d", 5), ("o", 6)] {
            let execution = run_ok(
                &world_program(
                    vec![read_line(4, 5, 6, Slot(0))],
                    Terminal::Return(project(Slot(0), name)),
                ),
                &call_under_test(vec![]),
                &store,
            );

            assert_eq!(committed_response(&execution), &integer(expected), "{name}");
        }
    }

    #[test]
    fn key_columns_project_from_primary_range_and_index_rows() {
        let all = [(3, 1, 1, 30), (1, 2, 2, 20), (2, 2, 1, 10), (1, 1, 1, 10)];
        let store = lines_store(&all);
        let reads = [
            (
                "primary range",
                read_lines_range((0, 0, 0), (9, 9, 9), Slot(0)),
                vec![(1, 1, 1), (1, 2, 2), (2, 2, 1), (3, 1, 1)],
            ),
            (
                "index on a non-key column",
                index_range("lines_by_qty", vec![0], vec![99]),
                vec![(1, 1, 1), (2, 2, 1), (1, 2, 2), (3, 1, 1)],
            ),
            (
                "index naming a key column",
                index_range("lines_by_d_qty", vec![0, 0], vec![9, 99]),
                vec![(1, 1, 1), (3, 1, 1), (2, 2, 1), (1, 2, 2)],
            ),
        ];

        for (case, read, expected) in reads {
            let execution = run_world(vec![read, copy_keys_into_seen(Slot(0))], &store);

            let expected: Vec<Key> = expected
                .into_iter()
                .map(|(w, d, o)| line_key(w, d, o))
                .collect();
            assert_eq!(keys_in_seen(&execution), expected, "{case}");
        }
    }

    #[test]
    fn an_index_row_exposes_its_non_key_indexed_columns_and_its_key_columns_only() {
        let store = lines_store(&[(1, 2, 3, 7)]);
        let projecting = |name: &str| {
            let read = index_range("lines_by_qty", vec![0], vec![99]);
            let iterate = Effect::Iterate {
                source: Slot(0),
                row: Slot(1),
                body: vec![Effect::Insert {
                    table: table("buckets"),
                    key: vec![project(Slot(1), name)],
                    columns: Default::default(),
                }],
            };
            run_world(vec![read, iterate], &store)
        };

        assert!(matches!(projecting("w").outcome, Outcome::Committed { .. }));
        assert_eq!(
            aborted_reason(&projecting("nope")),
            &AbortReason::MissingColumn {
                slot: Slot(1),
                column: column("nope")
            }
        );
    }

    fn index_range(index: &str, start: Vec<i64>, end: Vec<i64>) -> Effect {
        let literals = |bounds: Vec<i64>| bounds.into_iter().map(literal).collect();
        Effect::ReadIndexRange {
            index: IndexName(index.to_owned()),
            start: literals(start),
            end: literals(end),
            bound: literal(100),
            into: Slot(0),
        }
    }
}
