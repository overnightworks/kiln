//! Text columns, unique constraints checked at transaction end, and access
//! origin, driven through `kiln_ir::run` (`specs/kiln-ir.spec.md`).

mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns, Column,
    ColumnType, ConstraintRef, Effect, Execution, Expr, IntegerWidth, Outcome, Program, Row,
    Schema, SchemaError, Store, Terminal, UniqueConstraint, Value,
};
use support::{
    call_under_test, column, columns_of, columns_of_values, keyed_by_id, literal_interval,
    literal_key, one_part, program_with_schema, run_ok, table,
};

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

fn literal(value: Value) -> Expr {
    Expr::Literal(value)
}

fn accounts_email_unique() -> UniqueConstraint {
    UniqueConstraint {
        table: table("accounts"),
        columns: vec![column("email")],
    }
}

fn accounts_schema(unique_constraints: Vec<UniqueConstraint>) -> Schema {
    let text_column = |name| Column {
        name: column(name),
        kind: ColumnType::Text,
    };
    Schema {
        tables: vec![keyed_by_id(
            "accounts",
            vec![
                text_column("email"),
                text_column("name"),
                Column {
                    name: column("age"),
                    kind: ColumnType::Integer {
                        width: IntegerWidth::I8,
                        domain: None,
                    },
                },
            ],
        )],
        unique_constraints,
        ..Default::default()
    }
}

fn accounts_program(effects: Vec<Effect>) -> Program {
    program_with_schema(
        accounts_schema(vec![accounts_email_unique()]),
        vec![],
        effects,
        Terminal::Return(literal(Value::Integer(0))),
    )
}

fn insert_account(key: i64, name: &'static str, value: Value) -> Effect {
    Effect::Insert {
        table: table("accounts"),
        key: literal_key(key),
        columns: columns_of([(name, literal(value))]),
    }
}

fn update_account(key: i64, name: &'static str, value: Value) -> Effect {
    Effect::Update {
        table: table("accounts"),
        key: literal_key(key),
        columns: columns_of([(name, literal(value))]),
    }
}

fn delete_account(key: i64) -> Effect {
    Effect::Delete {
        table: table("accounts"),
        key: literal_key(key),
    }
}

fn account_with_email(email: &str) -> Row {
    Row {
        columns: columns_of_values([("email", text(email))]),
    }
}

fn store_of_accounts(accounts: &[(i64, &str)]) -> Store {
    Store::containing(
        accounts
            .iter()
            .map(|(key, email)| (table("accounts"), one_part(*key), account_with_email(email))),
    )
}

fn run_accounts(effects: Vec<Effect>, store: &Store) -> Execution {
    run_ok(&accounts_program(effects), &call_under_test(vec![]), store)
}

fn email_check(email: &str) -> AccessLogEntry {
    AccessLogEntry {
        table: table("accounts"),
        kind: AccessKind::ConstraintLookup,
        target: AccessTarget::ConstraintLookup {
            constraint: ConstraintRef::Unique(accounts_email_unique()),
            tuple: vec![text(email)],
        },
        columns: AccessedColumns::Named(vec![column("email")]),
        origin: AccessOrigin::ConstraintCheck,
    }
}

fn constraint_entries(execution: &Execution) -> Vec<AccessLogEntry> {
    execution
        .access_log
        .iter()
        .filter(|entry| entry.origin == AccessOrigin::ConstraintCheck)
        .cloned()
        .collect()
}

mod kiln_ir_37 {
    use super::*;

    #[test]
    fn text_value_written_to_a_text_column_commits() {
        let execution = run_accounts(
            vec![insert_account(1, "email", text("a@x"))],
            &Store::empty(),
        );

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        assert_eq!(execution.store, store_of_accounts(&[(1, "a@x")]));
    }
}

mod kiln_ir_38 {
    use super::*;

    #[test]
    fn integer_written_to_a_text_column_aborts_and_changes_no_state() {
        let store = store_of_accounts(&[(1, "a@x")]);
        let writes = [
            (insert_account(2, "email", Value::Integer(7)), 2),
            (update_account(1, "email", Value::Integer(7)), 1),
        ];

        for (write, key) in writes {
            let execution = run_accounts(vec![write], &store);

            assert_eq!(
                execution.outcome,
                Outcome::Aborted {
                    reason: AbortReason::NonTextValue {
                        table: table("accounts"),
                        column: column("email"),
                        key: one_part(key),
                    }
                }
            );
            assert_eq!(execution.store, store);
        }
    }
}

mod kiln_ir_39 {
    use super::*;

    #[test]
    fn unique_constraint_must_name_a_declared_table_and_column() {
        let declared = accounts_email_unique();
        let undeclared_column = UniqueConstraint {
            columns: vec![column("nickname")],
            ..declared.clone()
        };
        let undeclared_table = UniqueConstraint {
            table: table("people"),
            ..declared.clone()
        };
        let cases = [
            (declared, Ok(())),
            (
                undeclared_column.clone(),
                Err(SchemaError::UnknownUniqueTarget(undeclared_column)),
            ),
            (
                undeclared_table.clone(),
                Err(SchemaError::UnknownUniqueTarget(undeclared_table)),
            ),
        ];

        for (constraint, expected) in cases {
            assert_eq!(accounts_schema(vec![constraint]).validate(), expected);
        }
    }
}

mod kiln_ir_40 {
    use super::*;

    #[test]
    fn duplicate_that_is_gone_from_the_final_state_commits() {
        let store = store_of_accounts(&[(1, "a@x")]);
        let bodies = [
            vec![
                insert_account(2, "email", text("a@x")),
                update_account(2, "email", text("b@x")),
            ],
            vec![insert_account(2, "email", text("a@x")), delete_account(2)],
            vec![
                update_account(1, "email", text("b@x")),
                insert_account(2, "email", text("a@x")),
            ],
        ];

        for body in bodies {
            let execution = run_accounts(body, &store);

            assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        }
    }
}

mod kiln_ir_41 {
    use super::*;

    #[test]
    fn duplicate_in_the_final_state_aborts_naming_the_constraint_and_discards_every_effect() {
        let store = store_of_accounts(&[(1, "a@x"), (2, "b@x")]);
        let duplicating_writes = [
            insert_account(3, "email", text("a@x")),
            update_account(2, "email", text("a@x")),
        ];

        for duplicating_write in duplicating_writes {
            let execution = run_accounts(
                vec![insert_account(4, "email", text("c@x")), duplicating_write],
                &store,
            );

            assert_eq!(
                execution.outcome,
                Outcome::Aborted {
                    reason: AbortReason::UniqueViolation {
                        constraint: accounts_email_unique(),
                        tuple: vec![text("a@x")],
                    }
                }
            );
            assert_eq!(execution.store, store);
        }
    }
}

mod kiln_ir_42 {
    use super::*;

    #[test]
    fn check_logs_one_constraint_lookup_per_row_written_on_the_unique_column() {
        let store = store_of_accounts(&[(1, "a@x")]);
        let cases = [
            (
                "two rows inserted",
                vec![
                    insert_account(2, "email", text("b@x")),
                    insert_account(3, "email", text("c@x")),
                ],
                vec![email_check("b@x"), email_check("c@x")],
            ),
            (
                "one row written twice",
                vec![
                    insert_account(2, "email", text("b@x")),
                    update_account(2, "email", text("c@x")),
                ],
                vec![email_check("c@x")],
            ),
            (
                "row deleted again",
                vec![insert_account(2, "email", text("b@x")), delete_account(2)],
                vec![],
            ),
            (
                "other column written",
                vec![update_account(1, "name", text("Ann"))],
                vec![],
            ),
            (
                "insert omitting the unique column",
                vec![insert_account(2, "name", text("Ann"))],
                vec![],
            ),
            ("no write", vec![], vec![]),
        ];

        for (case, body, expected_checks) in cases {
            let execution = run_accounts(body, &store);

            assert_eq!(constraint_entries(&execution), expected_checks, "{case}");
        }
    }
}

mod kiln_ir_43 {
    use super::*;

    #[test]
    fn effects_of_the_transaction_log_logic_origin() {
        let store = store_of_accounts(&[(1, "a@x"), (2, "b@x")]);
        let body = vec![
            Effect::Read {
                table: table("accounts"),
                key: literal_key(1),
                into: kiln_ir::Slot(0),
            },
            Effect::ReadRange {
                table: table("accounts"),
                prefix: vec![],
                interval: literal_interval(0, 9),
                bound: literal(Value::Integer(9)),
                into: kiln_ir::Slot(1),
            },
            insert_account(3, "email", text("c@x")),
            update_account(3, "email", text("d@x")),
            delete_account(2),
        ];

        let execution = run_accounts(body, &store);

        let logic_kinds: Vec<_> = execution
            .access_log
            .iter()
            .filter(|entry| entry.origin == AccessOrigin::TransactionLogic)
            .map(|entry| entry.kind.clone())
            .collect();
        assert_eq!(
            logic_kinds,
            vec![
                AccessKind::PointRead,
                AccessKind::RangeRead,
                AccessKind::Insert,
                AccessKind::Update,
                AccessKind::Delete,
            ]
        );
    }
}

mod kiln_ir_44 {
    use super::*;

    #[test]
    fn constraint_accesses_carry_constraint_origin_and_stay_when_the_check_aborts() {
        let store = store_of_accounts(&[(1, "a@x")]);

        let execution = run_accounts(vec![insert_account(2, "email", text("a@x"))], &store);

        assert!(matches!(execution.outcome, Outcome::Aborted { .. }));
        assert_eq!(constraint_entries(&execution), vec![email_check("a@x")]);
    }
}
