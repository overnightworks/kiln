//! Foreign keys checked at transaction end, and the order constraints are
//! checked in, driven through `kiln_ir::run` (`specs/kiln-ir.spec.md`).

mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns,
    CheckConstraint, Column, ColumnType, Comparison, Effect, Execution, ForeignKey, IntegerWidth,
    Outcome, Program, Schema, SchemaError, Store, Table, Terminal, UniqueConstraint,
};
use support::{
    aborted_reason, call_under_test, column, constraint_entries, delete_row, insert_row, integer,
    program_with_schema, run_ok, store_of_rows, table, text, update_row, RowSpec,
};

fn integer_column(name: &str) -> Column {
    Column {
        name: column(name),
        kind: ColumnType::Integer {
            width: IntegerWidth::I32,
            domain: None,
        },
    }
}

fn orders_customer() -> ForeignKey {
    ForeignKey {
        table: table("orders"),
        column: column("customer"),
        references: table("customers"),
    }
}

fn orders_code_unique() -> UniqueConstraint {
    UniqueConstraint {
        table: table("orders"),
        column: column("code"),
    }
}

fn orders_at_least_zero(name: &str) -> CheckConstraint {
    CheckConstraint {
        table: table("orders"),
        column: column(name),
        comparison: Comparison::AtLeast,
        operand: integer(0),
    }
}

fn shop_schema() -> Schema {
    Schema {
        tables: vec![
            Table {
                name: table("customers"),
                columns: vec![Column {
                    name: column("name"),
                    kind: ColumnType::Text,
                }],
            },
            Table {
                name: table("orders"),
                columns: vec![
                    integer_column("customer"),
                    Column {
                        name: column("code"),
                        kind: ColumnType::Text,
                    },
                    integer_column("quantity"),
                    integer_column("discount"),
                ],
            },
        ],
        unique_constraints: vec![orders_code_unique()],
        foreign_keys: vec![orders_customer()],
        check_constraints: vec![
            orders_at_least_zero("quantity"),
            orders_at_least_zero("discount"),
        ],
    }
}

fn run_shop(effects: Vec<Effect>, store: &Store) -> Execution {
    let program: Program = program_with_schema(
        shop_schema(),
        vec![],
        effects,
        Terminal::Return(kiln_ir::Expr::Literal(integer(0))),
    );
    run_ok(&program, &call_under_test(vec![]), store)
}

fn customers(keys: &[i64]) -> Vec<RowSpec> {
    keys.iter().map(|&key| ("customers", key, vec![])).collect()
}

fn order_of(key: i64, customer: i64) -> RowSpec {
    ("orders", key, vec![("customer", integer(customer))])
}

fn store_with_orders(customer_keys: &[i64], orders: &[(i64, i64)]) -> Store {
    let mut rows = customers(customer_keys);
    rows.extend(
        orders
            .iter()
            .map(|&(key, customer)| order_of(key, customer)),
    );
    store_of_rows(&rows)
}

fn assign_customer(order: i64, customer: i64) -> Effect {
    update_row("orders", order, [("customer", integer(customer))])
}

fn parent_read(key: i64) -> AccessLogEntry {
    AccessLogEntry {
        table: table("customers"),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(integer(key)),
        columns: AccessedColumns::Named(vec![]),
        origin: AccessOrigin::ConstraintCheck,
    }
}

fn children_lookup(key: i64) -> AccessLogEntry {
    AccessLogEntry {
        table: table("orders"),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(integer(key)),
        columns: AccessedColumns::Named(vec![column("customer")]),
        origin: AccessOrigin::ConstraintCheck,
    }
}

fn code_check(code: &str) -> AccessLogEntry {
    AccessLogEntry {
        table: table("orders"),
        kind: AccessKind::PointRead,
        target: AccessTarget::Point(text(code)),
        columns: AccessedColumns::Named(vec![column("code")]),
        origin: AccessOrigin::ConstraintCheck,
    }
}

mod kiln_ir_45 {
    use super::*;

    #[test]
    fn foreign_key_must_name_declared_tables_and_a_declared_column() {
        let declared = orders_customer();
        let undeclared_column = ForeignKey {
            column: column("buyer"),
            ..declared.clone()
        };
        let undeclared_child_table = ForeignKey {
            table: table("invoices"),
            ..declared.clone()
        };
        let undeclared_parent_table = ForeignKey {
            references: table("people"),
            ..declared.clone()
        };
        let cases = [
            (declared, Ok(())),
            (
                undeclared_column.clone(),
                Err(SchemaError::UnknownForeignKeyTarget(undeclared_column)),
            ),
            (
                undeclared_child_table.clone(),
                Err(SchemaError::UnknownForeignKeyTarget(undeclared_child_table)),
            ),
            (
                undeclared_parent_table.clone(),
                Err(SchemaError::UnknownForeignKeyTarget(
                    undeclared_parent_table,
                )),
            ),
        ];

        for (foreign_key, expected) in cases {
            let schema = Schema {
                foreign_keys: vec![foreign_key],
                ..shop_schema()
            };

            assert_eq!(schema.validate(), expected);
        }
    }
}

mod kiln_ir_46 {
    use super::*;

    #[test]
    fn dangling_reference_in_the_final_state_aborts_naming_the_foreign_key() {
        let store = store_with_orders(&[1], &[(10, 1)]);
        let dangling_writes = [
            insert_row("orders", 11, [("customer", integer(2))]),
            assign_customer(10, 2),
        ];

        for dangling_write in dangling_writes {
            let execution = run_shop(
                vec![
                    insert_row("orders", 12, [("customer", integer(1))]),
                    dangling_write,
                ],
                &store,
            );

            assert_eq!(
                aborted_reason(&execution),
                &AbortReason::DanglingReference {
                    foreign_key: orders_customer(),
                    value: integer(2),
                }
            );
            assert_eq!(execution.store, store);
        }
    }
}

mod kiln_ir_47 {
    use super::*;

    #[test]
    fn reference_that_is_fixed_before_the_end_commits() {
        let store = store_with_orders(&[1], &[]);
        let bodies = [
            vec![
                insert_row("orders", 10, [("customer", integer(2))]),
                insert_row("customers", 2, []),
            ],
            vec![
                insert_row("orders", 10, [("customer", integer(2))]),
                assign_customer(10, 1),
            ],
            vec![
                insert_row("orders", 10, [("customer", integer(2))]),
                delete_row("orders", 10),
            ],
        ];

        for body in bodies {
            let execution = run_shop(body, &store);

            assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        }
    }
}

mod kiln_ir_48 {
    use super::*;

    #[test]
    fn parent_deleted_while_a_child_references_it_aborts_naming_the_foreign_key() {
        let store = store_with_orders(&[1, 2], &[(10, 1)]);

        let execution = run_shop(
            vec![delete_row("customers", 2), delete_row("customers", 1)],
            &store,
        );

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::ReferencedParentDeleted {
                foreign_key: orders_customer(),
                key: integer(1),
            }
        );
        assert_eq!(execution.store, store);
    }
}

mod kiln_ir_49 {
    use super::*;

    #[test]
    fn parent_deleted_without_remaining_references_commits() {
        let store = store_with_orders(&[1, 2], &[(10, 1)]);
        let bodies = [
            ("never referenced", vec![delete_row("customers", 2)]),
            (
                "children removed first",
                vec![delete_row("orders", 10), delete_row("customers", 1)],
            ),
            (
                "children repointed first",
                vec![assign_customer(10, 2), delete_row("customers", 1)],
            ),
            (
                "parent inserted again",
                vec![delete_row("customers", 1), insert_row("customers", 1, [])],
            ),
        ];

        for (case, body) in bodies {
            let execution = run_shop(body, &store);

            assert!(
                matches!(execution.outcome, Outcome::Committed { .. }),
                "{case}"
            );
        }
    }
}

mod kiln_ir_50 {
    use super::*;

    #[test]
    fn checks_log_point_reads_and_lookups_by_key_and_never_scans() {
        let store = store_with_orders(&[1, 2, 3], &[(10, 1)]);
        let cases = [
            (
                "one child row written",
                vec![insert_row("orders", 11, [("customer", integer(2))])],
                vec![parent_read(2)],
            ),
            (
                "child row written twice",
                vec![
                    insert_row("orders", 11, [("customer", integer(2))]),
                    assign_customer(11, 3),
                ],
                vec![parent_read(3)],
            ),
            (
                "two child rows written",
                vec![
                    insert_row("orders", 12, [("customer", integer(3))]),
                    insert_row("orders", 11, [("customer", integer(2))]),
                ],
                vec![parent_read(2), parent_read(3)],
            ),
            (
                "child row deleted again",
                vec![
                    insert_row("orders", 11, [("customer", integer(2))]),
                    delete_row("orders", 11),
                ],
                vec![],
            ),
            (
                "other column written",
                vec![update_row("orders", 10, [("quantity", integer(5))])],
                vec![],
            ),
            (
                "parent deleted",
                vec![delete_row("customers", 3)],
                vec![children_lookup(3)],
            ),
            (
                "parent inserted again",
                vec![delete_row("customers", 3), insert_row("customers", 3, [])],
                vec![],
            ),
            ("no write", vec![], vec![]),
        ];

        for (case, body, expected_checks) in cases {
            let execution = run_shop(body, &store);

            assert_eq!(constraint_entries(&execution), expected_checks, "{case}");
        }
    }
}

mod kiln_ir_55 {
    use super::*;

    fn bad_unique_and_dangling() -> Vec<Effect> {
        vec![insert_row(
            "orders",
            11,
            [("code", text("a")), ("customer", integer(9))],
        )]
    }

    #[test]
    fn unique_violation_stops_the_foreign_key_check_from_logging() {
        let store = store_of_rows(&[
            (
                "orders",
                10,
                vec![("code", text("a")), ("customer", integer(1))],
            ),
            ("customers", 1, vec![]),
        ]);

        let execution = run_shop(bad_unique_and_dangling(), &store);

        assert!(matches!(
            aborted_reason(&execution),
            AbortReason::UniqueViolation { .. }
        ));
        assert_eq!(constraint_entries(&execution), vec![code_check("a")]);
    }

    #[test]
    fn foreign_key_violation_is_reported_before_a_check_violation() {
        let execution = run_shop(
            vec![insert_row(
                "orders",
                11,
                [("customer", integer(9)), ("quantity", integer(-1))],
            )],
            &Store::empty(),
        );

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::DanglingReference {
                foreign_key: orders_customer(),
                value: integer(9),
            }
        );
    }

    #[test]
    fn first_declared_check_violation_is_reported() {
        let store = store_with_orders(&[1], &[]);
        let both_violated = vec![insert_row(
            "orders",
            11,
            [
                ("customer", integer(1)),
                ("quantity", integer(-1)),
                ("discount", integer(-1)),
            ],
        )];

        let execution = run_shop(both_violated, &store);

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::CheckViolation {
                constraint: orders_at_least_zero("quantity"),
                key: integer(11),
            }
        );
    }
}
