//! Composite foreign keys and composite unique constraints (`docs/concept.md`
//! §2.2 D8) driven through `kiln_ir::run` (`specs/kiln-ir.spec.md`,
//! KILN-IR-87 to KILN-IR-90).
mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns,
    ConstraintRef, Effect, Execution, Expr, ForeignKey, IntegerWidth, Key, Outcome, Row, Schema,
    SchemaError, Store, Table, Terminal, UniqueConstraint, Value,
};
use support::world::{int_column, key_of, text_column};
use support::{
    aborted_reason, call_under_test, column, columns_of, columns_of_values, constraint_entries,
    integer, program_with_schema, run_ok, table,
};

/// `districts` keyed by `(w, d)`, `customers` keyed by `(w, d, c)` and
/// `orders` keyed by `id`, which holds the customer it names in its
/// non-key columns `w`, `d` and `c`.
fn shop_tables() -> Vec<Table> {
    let numbered = |name: &str, key: &[&str], others: &[&str]| Table {
        name: table(name),
        columns: key
            .iter()
            .chain(others)
            .map(|name| int_column(name, IntegerWidth::I32))
            .collect(),
        key: key_of(key),
    };
    vec![
        numbered("districts", &["w", "d"], &[]),
        numbered("customers", &["w", "d", "c"], &[]),
        numbered("orders", &["id"], &["w", "d", "c"]),
    ]
}

fn orders_customer() -> ForeignKey {
    foreign_key("orders", &["w", "d", "c"], "customers")
}

fn foreign_key(from: &str, columns: &[&str], to: &str) -> ForeignKey {
    ForeignKey {
        table: table(from),
        columns: columns.iter().map(|name| column(name)).collect(),
        references: table(to),
    }
}

fn unique(on: &str, columns: &[&str]) -> UniqueConstraint {
    UniqueConstraint {
        table: table(on),
        columns: columns.iter().map(|name| column(name)).collect(),
    }
}

fn shop_schema() -> Schema {
    Schema {
        tables: shop_tables(),
        foreign_keys: vec![
            orders_customer(),
            foreign_key("customers", &["w", "d"], "districts"),
        ],
        unique_constraints: vec![unique("orders", &["w", "d", "c"])],
        ..Default::default()
    }
}

fn numbers(values: &[i64]) -> Vec<Value> {
    values.iter().copied().map(integer).collect()
}

fn literals(values: &[i64]) -> Vec<Expr> {
    numbers(values).into_iter().map(Expr::Literal).collect()
}

/// A table, a key and the non-key values of one row to put in a store.
type StoredRow = (&'static str, Vec<i64>, Vec<(&'static str, i64)>);

fn customer(w: i64, d: i64, c: i64) -> StoredRow {
    ("customers", vec![w, d, c], vec![])
}

fn store_of(rows: Vec<StoredRow>) -> Store {
    Store::containing(rows.into_iter().map(|(name, key, values)| {
        (
            table(name),
            Key(numbers(&key)),
            Row {
                columns: columns_of_values(values.into_iter().map(|(c, v)| (c, integer(v)))),
            },
        )
    }))
}

fn insert(into: &str, key: &[i64], values: &[(&'static str, i64)]) -> Effect {
    Effect::Insert {
        table: table(into),
        key: literals(key),
        columns: columns_of(values.iter().map(|&(c, v)| (c, Expr::Literal(integer(v))))),
    }
}

fn update(of: &str, key: &[i64], values: &[(&'static str, i64)]) -> Effect {
    Effect::Update {
        table: table(of),
        key: literals(key),
        columns: columns_of(values.iter().map(|&(c, v)| (c, Expr::Literal(integer(v))))),
    }
}

fn delete(from: &str, key: &[i64]) -> Effect {
    Effect::Delete {
        table: table(from),
        key: literals(key),
    }
}

fn run_shop(effects: Vec<Effect>, store: &Store) -> Execution {
    let program = program_with_schema(
        shop_schema(),
        vec![],
        effects,
        Terminal::Return(Expr::Literal(integer(0))),
    );
    run_ok(&program, &call_under_test(vec![]), store)
}

fn assert_commits(execution: &Execution) {
    assert!(
        matches!(execution.outcome, Outcome::Committed { .. }),
        "{:?}",
        execution.outcome
    );
}

fn stored_customers(rows: &[(i64, i64, i64)]) -> Store {
    store_of(rows.iter().map(|&(w, d, c)| customer(w, d, c)).collect())
}

fn constraint_lookup(
    on: &str,
    constraint: ConstraintRef,
    tuple: &[i64],
    columns: &[&str],
) -> AccessLogEntry {
    AccessLogEntry {
        table: table(on),
        kind: AccessKind::ConstraintLookup,
        target: AccessTarget::ConstraintLookup {
            constraint,
            tuple: numbers(tuple),
        },
        columns: AccessedColumns::Named(columns.iter().map(|name| column(name)).collect()),
        origin: AccessOrigin::ConstraintCheck,
    }
}

mod kiln_ir_87 {
    use super::*;

    #[test]
    fn a_child_whose_full_tuple_names_a_parent_commits_and_reads_that_parent_once() {
        let store = stored_customers(&[(1, 2, 3)]);

        let execution = run_shop(
            vec![insert("orders", &[10], &[("w", 1), ("d", 2), ("c", 3)])],
            &store,
        );

        assert_commits(&execution);
        assert_eq!(
            constraint_entries(&execution)
                .into_iter()
                .filter(|entry| entry.table == table("customers"))
                .collect::<Vec<_>>(),
            vec![AccessLogEntry {
                table: table("customers"),
                kind: AccessKind::PointRead,
                target: AccessTarget::Point(Key(numbers(&[1, 2, 3]))),
                columns: AccessedColumns::Named(vec![]),
                origin: AccessOrigin::ConstraintCheck,
            }]
        );
    }

    #[test]
    fn a_full_tuple_that_matches_no_parent_aborts_naming_the_tuple_even_when_most_components_match()
    {
        let store = stored_customers(&[(1, 2, 3)]);
        let near_misses = [[9, 2, 3], [1, 9, 3], [1, 2, 9]];

        for [w, d, c] in near_misses {
            let execution = run_shop(
                vec![insert("orders", &[10], &[("w", w), ("d", d), ("c", c)])],
                &store,
            );

            assert_eq!(
                aborted_reason(&execution),
                &AbortReason::DanglingReference {
                    foreign_key: orders_customer(),
                    referenced: Key(numbers(&[w, d, c])),
                }
            );
            assert_eq!(execution.store, store);
        }
    }

    #[test]
    fn a_child_row_lacking_any_foreign_key_column_references_nothing() {
        let store = stored_customers(&[]);
        let incomplete_rows = [
            [("w", 1), ("d", 2)],
            [("w", 1), ("c", 3)],
            [("d", 2), ("c", 3)],
        ];

        for values in incomplete_rows {
            let execution = run_shop(vec![insert("orders", &[10], &values)], &store);

            assert_commits(&execution);
            assert!(constraint_entries(&execution)
                .iter()
                .all(|entry| entry.table != table("customers")));
        }
    }

    #[test]
    fn updating_one_foreign_key_column_checks_the_new_tuple_with_the_row_s_other_final_columns() {
        let order = ("orders", vec![10], vec![("w", 1), ("d", 2), ("c", 3)]);
        let store = store_of(vec![customer(1, 2, 3), customer(1, 2, 4), order]);

        let to_existing = run_shop(vec![update("orders", &[10], &[("c", 4)])], &store);
        let to_missing = run_shop(vec![update("orders", &[10], &[("c", 5)])], &store);

        assert_commits(&to_existing);
        assert_eq!(
            aborted_reason(&to_missing),
            &AbortReason::DanglingReference {
                foreign_key: orders_customer(),
                referenced: Key(numbers(&[1, 2, 5])),
            }
        );
    }

    #[test]
    fn a_parent_inserted_by_the_same_call_satisfies_the_reference() {
        let execution = run_shop(
            vec![
                insert("orders", &[10], &[("w", 1), ("d", 2), ("c", 3)]),
                insert("districts", &[1, 2], &[]),
                insert("customers", &[1, 2, 3], &[]),
            ],
            &Store::empty(),
        );

        assert_commits(&execution);
    }

    #[test]
    fn a_key_column_of_the_child_is_read_from_its_key() {
        let schema = Schema {
            tables: shop_tables(),
            foreign_keys: vec![foreign_key("customers", &["w", "d"], "districts")],
            ..Default::default()
        };
        let program = program_with_schema(
            schema,
            vec![],
            vec![insert("customers", &[1, 2, 3], &[])],
            Terminal::Return(Expr::Literal(integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &Store::empty());

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::DanglingReference {
                foreign_key: foreign_key("customers", &["w", "d"], "districts"),
                referenced: Key(numbers(&[1, 2])),
            }
        );
    }
}

mod kiln_ir_88 {
    use super::*;

    fn store_with_an_order_of_customer_3() -> Store {
        store_of(vec![
            customer(1, 2, 3),
            customer(1, 2, 4),
            ("orders", vec![10], vec![("w", 1), ("d", 2), ("c", 3)]),
        ])
    }

    #[test]
    fn deleting_a_parent_a_child_still_names_aborts_naming_its_key_and_logs_one_children_lookup() {
        let store = store_with_an_order_of_customer_3();

        let execution = run_shop(vec![delete("customers", &[1, 2, 3])], &store);

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::ReferencedParentDeleted {
                foreign_key: orders_customer(),
                key: Key(numbers(&[1, 2, 3])),
            }
        );
        assert_eq!(execution.store, store);
        assert_eq!(
            constraint_entries(&execution),
            vec![constraint_lookup(
                "orders",
                ConstraintRef::ForeignKey(orders_customer()),
                &[1, 2, 3],
                &["w", "d", "c"],
            )]
        );
    }

    #[test]
    fn deleting_a_parent_whose_key_shares_most_components_with_a_child_s_commits() {
        let store = store_with_an_order_of_customer_3();

        let execution = run_shop(vec![delete("customers", &[1, 2, 4])], &store);

        assert_commits(&execution);
    }

    #[test]
    fn repointing_the_child_first_and_then_deleting_the_parent_commits() {
        let store = store_with_an_order_of_customer_3();

        let execution = run_shop(
            vec![
                update("orders", &[10], &[("c", 4)]),
                delete("customers", &[1, 2, 3]),
            ],
            &store,
        );

        assert_commits(&execution);
    }

    #[test]
    fn deleting_the_child_together_with_its_parent_commits() {
        let store = store_with_an_order_of_customer_3();

        let execution = run_shop(
            vec![delete("customers", &[1, 2, 3]), delete("orders", &[10])],
            &store,
        );

        assert_commits(&execution);
    }
}

mod kiln_ir_89 {
    use super::*;

    fn store_with_order(w: i64, d: i64, c: i64) -> Store {
        store_of(vec![
            customer(1, 2, 3),
            customer(1, 2, 4),
            customer(5, 2, 3),
            ("orders", vec![10], vec![("w", w), ("d", d), ("c", c)]),
        ])
    }

    #[test]
    fn two_rows_holding_the_same_tuple_abort_naming_the_tuple_and_log_one_lookup() {
        let store = store_with_order(1, 2, 3);

        let execution = run_shop(
            vec![insert("orders", &[11], &[("w", 1), ("d", 2), ("c", 3)])],
            &store,
        );

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::UniqueViolation {
                constraint: unique("orders", &["w", "d", "c"]),
                tuple: numbers(&[1, 2, 3]),
            }
        );
        assert_eq!(execution.store, store);
        assert_eq!(
            constraint_entries(&execution),
            vec![constraint_lookup(
                "orders",
                ConstraintRef::Unique(unique("orders", &["w", "d", "c"])),
                &[1, 2, 3],
                &["w", "d", "c"],
            )]
        );
    }

    #[test]
    fn a_tuple_differing_in_one_component_is_distinct() {
        let store = store_with_order(1, 2, 3);
        let neighbours = [[5, 2, 3], [1, 2, 4]];

        for [w, d, c] in neighbours {
            let execution = run_shop(
                vec![insert("orders", &[11], &[("w", w), ("d", d), ("c", c)])],
                &store,
            );

            assert_commits(&execution);
        }
    }

    #[test]
    fn updating_a_row_into_a_held_tuple_aborts() {
        let store = store_of(vec![
            customer(1, 2, 3),
            customer(1, 2, 4),
            ("orders", vec![10], vec![("w", 1), ("d", 2), ("c", 3)]),
            ("orders", vec![11], vec![("w", 1), ("d", 2), ("c", 4)]),
        ]);

        let execution = run_shop(vec![update("orders", &[11], &[("c", 3)])], &store);

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::UniqueViolation {
                constraint: unique("orders", &["w", "d", "c"]),
                tuple: numbers(&[1, 2, 3]),
            }
        );
    }

    #[test]
    fn rows_lacking_a_column_of_the_tuple_are_bound_by_nothing() {
        let schema = Schema {
            tables: shop_tables(),
            unique_constraints: vec![unique("orders", &["w", "d"])],
            ..Default::default()
        };
        let program = program_with_schema(
            schema,
            vec![],
            vec![
                insert("orders", &[10], &[("w", 1)]),
                insert("orders", &[11], &[("w", 1)]),
            ],
            Terminal::Return(Expr::Literal(integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &Store::empty());

        assert_commits(&execution);
        assert_eq!(constraint_entries(&execution), vec![]);
    }

    #[test]
    fn a_unique_constraint_on_the_key_columns_of_a_composite_key_binds_the_inserted_row() {
        let schema = Schema {
            tables: shop_tables(),
            unique_constraints: vec![unique("customers", &["d", "c"])],
            ..Default::default()
        };
        let program = program_with_schema(
            schema,
            vec![],
            vec![insert("customers", &[2, 2, 3], &[])],
            Terminal::Return(Expr::Literal(integer(0))),
        );

        let execution = run_ok(
            &program,
            &call_under_test(vec![]),
            &stored_customers(&[(1, 2, 3)]),
        );

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::UniqueViolation {
                constraint: unique("customers", &["d", "c"]),
                tuple: numbers(&[2, 3]),
            }
        );
    }
}

mod kiln_ir_90 {
    use super::*;

    fn schema_with(
        foreign_keys: Vec<ForeignKey>,
        unique_constraints: Vec<UniqueConstraint>,
    ) -> Schema {
        Schema {
            tables: [
                shop_tables(),
                vec![Table {
                    name: table("notes"),
                    columns: vec![text_column("w"), int_column("d", IntegerWidth::I32)],
                    key: key_of(&["w"]),
                }],
            ]
            .concat(),
            foreign_keys,
            unique_constraints,
            ..Default::default()
        }
    }

    fn with_foreign_key(foreign_key: &ForeignKey) -> Result<(), SchemaError> {
        schema_with(vec![foreign_key.clone()], vec![]).validate()
    }

    #[test]
    fn a_foreign_key_whose_column_count_differs_from_the_parent_key_s_is_rejected() {
        let cases = [
            foreign_key("orders", &["w", "d"], "customers"),
            foreign_key("orders", &["w", "d", "c", "id"], "customers"),
            foreign_key("orders", &[], "customers"),
            foreign_key("orders", &["w", "d"], "orders"),
        ];

        for case in cases {
            assert_eq!(
                with_foreign_key(&case),
                Err(SchemaError::ForeignKeyKeyArityMismatch(case.clone()))
            );
        }
    }

    #[test]
    fn a_foreign_key_whose_column_differs_in_kind_from_the_parent_key_column_at_its_position_is_rejected(
    ) {
        let text_first = ForeignKey {
            table: table("notes"),
            columns: vec![column("w")],
            references: table("orders"),
        };
        let same_kinds_in_another_order = foreign_key("orders", &["d", "w", "c"], "customers");
        let in_order = foreign_key("orders", &["w", "d", "c"], "customers");
        let text_at_second = ForeignKey {
            table: table("notes"),
            columns: vec![column("d"), column("w")],
            references: table("districts"),
        };

        assert_eq!(
            with_foreign_key(&text_first),
            Err(SchemaError::ForeignKeyTypeMismatch(text_first.clone()))
        );
        assert_eq!(
            with_foreign_key(&text_at_second),
            Err(SchemaError::ForeignKeyTypeMismatch(text_at_second.clone()))
        );
        assert_eq!(with_foreign_key(&same_kinds_in_another_order), Ok(()));
        assert_eq!(with_foreign_key(&in_order), Ok(()));
    }

    #[test]
    fn a_foreign_key_or_unique_constraint_naming_an_undeclared_column_is_rejected() {
        let foreign_key = foreign_key("orders", &["w", "d", "nope"], "customers");
        let unique_constraint = unique("orders", &["w", "nope"]);

        assert_eq!(
            with_foreign_key(&foreign_key),
            Err(SchemaError::UnknownForeignKeyTarget(foreign_key.clone()))
        );
        assert_eq!(
            schema_with(vec![], vec![unique_constraint.clone()]).validate(),
            Err(SchemaError::UnknownUniqueTarget(unique_constraint))
        );
    }

    #[test]
    fn a_unique_constraint_naming_no_column_is_rejected() {
        let nothing = unique("orders", &[]);

        assert_eq!(
            schema_with(vec![], vec![nothing.clone()]).validate(),
            Err(SchemaError::UniqueWithoutColumns(nothing))
        );
    }

    #[test]
    fn the_shop_schema_is_valid() {
        assert_eq!(shop_schema().validate(), Ok(()));
    }
}
