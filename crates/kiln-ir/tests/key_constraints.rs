//! Constraints over compound keys (`specs/kiln-ir.spec.md`, KILN-IR-83,
//! KILN-IR-45 and KILN-IR-57).
mod support;

use kiln_ir::{
    AbortReason, AccessTarget, CheckConstraint, Comparison, ConstraintRef, Effect, Execution,
    ForeignKey, IntegerWidth, Key, Outcome, Row, Schema, SchemaError, Store, Table,
    UniqueConstraint,
};
use support::world::*;
use support::{
    aborted_reason, call_under_test, column, columns_of, columns_of_values, constraint_entries,
    integer, program_with_schema, run_ok, table,
};

mod kiln_ir_83 {
    use super::*;

    /// `edges` is keyed by `(src, dst)`; `src` references `nodes`, `dst` is
    /// unique and checked non-negative.
    fn edges_schema() -> Schema {
        Schema {
            tables: vec![
                Table {
                    name: table("nodes"),
                    columns: vec![int_column("id", IntegerWidth::I32)],
                    key: key_of(&["id"]),
                },
                Table {
                    name: table("edges"),
                    columns: vec![
                        int_column("src", IntegerWidth::I32),
                        int_column("dst", IntegerWidth::I32),
                        int_column("weight", IntegerWidth::I32),
                    ],
                    key: key_of(&["src", "dst"]),
                },
            ],
            unique_constraints: vec![dst_unique()],
            foreign_keys: vec![src_references_nodes()],
            check_constraints: vec![dst_non_negative()],
            ..Default::default()
        }
    }

    fn dst_unique() -> UniqueConstraint {
        UniqueConstraint {
            table: table("edges"),
            columns: vec![column("dst")],
        }
    }

    fn src_references_nodes() -> ForeignKey {
        ForeignKey {
            table: table("edges"),
            columns: vec![column("src")],
            references: table("nodes"),
        }
    }

    fn dst_non_negative() -> CheckConstraint {
        CheckConstraint {
            table: table("edges"),
            column: column("dst"),
            comparison: Comparison::AtLeast,
            operand: integer(0),
        }
    }

    fn edge_key(src: i64, dst: i64) -> Key {
        Key(vec![integer(src), integer(dst)])
    }

    fn node_store(ids: &[i64]) -> Store {
        Store::containing(
            ids.iter()
                .map(|&id| (table("nodes"), Key(vec![integer(id)]), Row::default())),
        )
    }

    fn run_edges(effects: Vec<Effect>, store: &Store) -> Execution {
        let program = program_with_schema(edges_schema(), vec![], effects, returning_zero());
        run_ok(&program, &call_under_test(vec![]), store)
    }

    fn insert_edge(src: i64, dst: i64) -> Effect {
        Effect::Insert {
            table: table("edges"),
            key: vec![literal(src), literal(dst)],
            columns: columns_of([("weight", literal(1))]),
        }
    }

    #[test]
    fn an_inserted_key_column_is_checked_by_each_kind_of_constraint() {
        let store = node_store(&[1, 2]);

        let negative = run_edges(vec![insert_edge(1, -1)], &store);
        let dangling = run_edges(vec![insert_edge(9, 3)], &store);
        let duplicate = run_edges(vec![insert_edge(1, 5), insert_edge(2, 5)], &store);

        assert_eq!(
            aborted_reason(&duplicate),
            &AbortReason::UniqueViolation {
                constraint: dst_unique(),
                tuple: vec![integer(5)],
            }
        );
        assert_eq!(
            aborted_reason(&dangling),
            &AbortReason::DanglingReference {
                foreign_key: src_references_nodes(),
                referenced: Key(vec![integer(9)]),
            }
        );
        assert_eq!(
            aborted_reason(&negative),
            &AbortReason::CheckViolation {
                constraint: dst_non_negative(),
                key: edge_key(1, -1)
            }
        );
    }

    #[test]
    fn the_unique_check_of_an_inserted_key_column_logs_a_lookup_of_the_key_component() {
        let store = node_store(&[1]);

        let execution = run_edges(vec![insert_edge(1, 5)], &store);

        let lookups: Vec<_> = constraint_entries(&execution)
            .into_iter()
            .map(|entry| entry.target)
            .collect();
        assert_eq!(
            lookups,
            vec![
                AccessTarget::ConstraintLookup {
                    constraint: ConstraintRef::Unique(dst_unique()),
                    tuple: vec![integer(5)]
                },
                AccessTarget::Point(Key(vec![integer(1)])),
            ]
        );
    }

    #[test]
    fn an_update_never_checks_a_key_column() {
        let store = Store::containing([
            (table("nodes"), Key(vec![integer(1)]), Row::default()),
            (
                table("edges"),
                edge_key(1, 5),
                Row {
                    columns: columns_of_values([("weight", integer(1))]),
                },
            ),
        ]);
        let update = Effect::Update {
            table: table("edges"),
            key: vec![literal(1), literal(5)],
            columns: columns_of([("weight", literal(2))]),
        };

        let execution = run_edges(vec![update], &store);

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        assert!(constraint_entries(&execution).is_empty());
    }
}

mod kiln_ir_45 {
    use super::*;

    fn reference_to(parent: &str) -> Schema {
        Schema {
            foreign_keys: vec![ForeignKey {
                table: table("lines"),
                columns: vec![column("qty")],
                references: table(parent),
            }],
            ..world_schema()
        }
    }

    #[test]
    fn a_foreign_key_onto_a_table_keyed_by_several_columns_or_none_is_rejected() {
        for parent in ["seen", "events"] {
            let schema = reference_to(parent);

            assert_eq!(
                schema.validate(),
                Err(SchemaError::ForeignKeyKeyArityMismatch(
                    schema.foreign_keys[0].clone()
                )),
                "{parent}"
            );
        }
    }
}

mod kiln_ir_57 {
    use super::*;

    #[test]
    fn a_foreign_key_column_must_have_the_kind_of_the_referenced_key_column() {
        let foreign_key = |from: &str, to: &str| ForeignKey {
            table: table("labels"),
            columns: vec![column(from)],
            references: table(to),
        };
        let schema_with = |foreign_key: &ForeignKey| Schema {
            foreign_keys: vec![foreign_key.clone()],
            tables: {
                let mut tables = world_schema().tables;
                tables.push(Table {
                    name: table("tags"),
                    columns: vec![text_column("name")],
                    key: key_of(&["name"]),
                });
                tables.push(Table {
                    name: table("numbers"),
                    columns: vec![int_column("n", IntegerWidth::I32)],
                    key: key_of(&["n"]),
                });
                tables
            },
            ..Default::default()
        };
        let text_onto_text = foreign_key("note", "tags");
        let text_onto_integer = foreign_key("note", "numbers");

        assert_eq!(schema_with(&text_onto_text).validate(), Ok(()));
        assert_eq!(
            schema_with(&text_onto_integer).validate(),
            Err(SchemaError::ForeignKeyTypeMismatch(text_onto_integer))
        );
    }
}
