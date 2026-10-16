//! Check constraints checked at transaction end, driven through
//! `kiln_ir::run` (`specs/kiln-ir.spec.md`).

mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns,
    CheckConstraint, ClampDomain, Column, ColumnType, Comparison, Effect, Execution, Expr,
    IntegerWidth, Outcome, OverflowReason, Schema, SchemaError, Store, Terminal, Value,
};
use support::{
    aborted_reason, call_under_test, column, constraint_entries, insert_row, integer, keyed_by_id,
    one_part, program_with_schema, run_ok, store_of_rows, table, text, update_row,
};

fn balance_check(comparison: Comparison, operand: i64) -> CheckConstraint {
    CheckConstraint {
        table: table("accounts"),
        column: column("balance"),
        comparison,
        operand: integer(operand),
    }
}

fn accounts_schema(balance: ColumnType, check_constraints: Vec<CheckConstraint>) -> Schema {
    Schema {
        tables: vec![keyed_by_id(
            "accounts",
            vec![Column {
                name: column("balance"),
                kind: balance,
            }],
        )],
        check_constraints,
        ..Default::default()
    }
}

fn plain_balance() -> ColumnType {
    ColumnType::Integer {
        width: IntegerWidth::I8,
        domain: None,
    }
}

fn run_accounts(schema: Schema, effects: Vec<Effect>, store: &Store) -> Execution {
    let program = program_with_schema(
        schema,
        vec![],
        effects,
        Terminal::Return(Expr::Literal(integer(0))),
    );
    run_ok(&program, &call_under_test(vec![]), store)
}

fn set_balance(key: i64, value: i64) -> Effect {
    update_row("accounts", key, [("balance", integer(value))])
}

fn store_with_balance(balance: i64) -> Store {
    store_of_rows(&[("accounts", 1, vec![("balance", integer(balance))])])
}

mod kiln_ir_51 {
    use super::*;

    #[test]
    fn check_constraint_must_name_a_declared_table_and_column() {
        let declared = balance_check(Comparison::AtLeast, 0);
        let undeclared_column = CheckConstraint {
            column: column("credit"),
            ..declared.clone()
        };
        let undeclared_table = CheckConstraint {
            table: table("people"),
            ..declared.clone()
        };
        let cases = [
            (declared, Ok(())),
            (
                undeclared_column.clone(),
                Err(SchemaError::UnknownCheckTarget(undeclared_column)),
            ),
            (
                undeclared_table.clone(),
                Err(SchemaError::UnknownCheckTarget(undeclared_table)),
            ),
        ];

        for (constraint, expected) in cases {
            assert_eq!(
                accounts_schema(plain_balance(), vec![constraint]).validate(),
                expected
            );
        }
    }
}

mod kiln_ir_52 {
    use super::*;

    #[test]
    fn row_violating_the_check_in_the_final_state_aborts_naming_the_constraint() {
        let cases = [
            (Comparison::AtLeast, 0, -1, true),
            (Comparison::AtLeast, 0, 0, false),
            (Comparison::AtLeast, 0, 1, false),
            (Comparison::AtMost, 10, 11, true),
            (Comparison::AtMost, 10, 10, false),
            (Comparison::AtMost, 10, 9, false),
        ];

        for (comparison, operand, written, violates) in cases {
            let constraint = balance_check(comparison, operand);
            let store = store_with_balance(5);

            let execution = run_accounts(
                accounts_schema(plain_balance(), vec![constraint.clone()]),
                vec![set_balance(1, written)],
                &store,
            );

            if violates {
                assert_eq!(
                    aborted_reason(&execution),
                    &AbortReason::CheckViolation {
                        constraint,
                        key: one_part(1),
                    },
                    "{comparison:?} {operand} with {written}"
                );
                assert_eq!(execution.store, store);
            } else {
                assert!(matches!(execution.outcome, Outcome::Committed { .. }));
            }
        }
    }

    #[test]
    fn inserted_row_violating_the_check_aborts_naming_its_key() {
        let constraint = balance_check(Comparison::AtLeast, 0);

        let execution = run_accounts(
            accounts_schema(plain_balance(), vec![constraint.clone()]),
            vec![insert_row("accounts", 7, [("balance", integer(-1))])],
            &Store::empty(),
        );

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::CheckViolation {
                constraint,
                key: one_part(7),
            }
        );
    }

    #[test]
    fn violation_repaired_before_the_end_commits() {
        let schema = accounts_schema(plain_balance(), vec![balance_check(Comparison::AtLeast, 0)]);

        let execution = run_accounts(
            schema,
            vec![set_balance(1, -50), set_balance(1, 50)],
            &store_with_balance(50),
        );

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
    }
}

mod kiln_ir_53 {
    use super::*;

    #[test]
    fn check_aborts_where_a_clamp_writes_the_bound_and_an_overflow_aborts() {
        let clamped = ColumnType::Integer {
            width: IntegerWidth::I8,
            domain: Some(ClampDomain { min: 0, max: 100 }),
        };

        let checked = run_accounts(
            accounts_schema(plain_balance(), vec![balance_check(Comparison::AtLeast, 0)]),
            vec![set_balance(1, -5)],
            &store_with_balance(1),
        );
        let clamping = run_accounts(
            accounts_schema(clamped, vec![]),
            vec![set_balance(1, -5)],
            &store_with_balance(1),
        );
        let overflowing = run_accounts(
            accounts_schema(plain_balance(), vec![]),
            vec![set_balance(1, 300)],
            &store_with_balance(1),
        );

        assert!(matches!(
            aborted_reason(&checked),
            AbortReason::CheckViolation { .. }
        ));
        assert!(matches!(clamping.outcome, Outcome::Committed { .. }));
        assert_eq!(clamping.store, store_with_balance(0));
        assert!(matches!(
            aborted_reason(&overflowing),
            AbortReason::Overflow(OverflowReason::Width { .. })
        ));
    }
}

mod kiln_ir_54 {
    use super::*;

    #[test]
    fn check_logs_nothing_beyond_the_rows_own_write() {
        let execution = run_accounts(
            accounts_schema(plain_balance(), vec![balance_check(Comparison::AtLeast, 0)]),
            vec![set_balance(1, 7)],
            &store_with_balance(5),
        );

        assert!(constraint_entries(&execution).is_empty());
        assert_eq!(
            execution.access_log,
            vec![AccessLogEntry {
                table: table("accounts"),
                kind: AccessKind::Update,
                target: AccessTarget::Point(one_part(1)),
                columns: AccessedColumns::Named(vec![column("balance")]),
                origin: AccessOrigin::TransactionLogic,
            }]
        );
    }
}

mod kiln_ir_56 {
    use super::*;

    fn check_with(operand: Value) -> CheckConstraint {
        CheckConstraint {
            operand,
            ..balance_check(Comparison::AtLeast, 0)
        }
    }

    #[test]
    fn operand_must_have_the_type_of_its_column() {
        let text_balance = ColumnType::Text;
        let cases = [
            (plain_balance(), integer(0), true),
            (plain_balance(), text("0"), false),
            (text_balance, text("a"), true),
            (text_balance, integer(0), false),
        ];

        for (column_type, operand, accepted) in cases {
            let constraint = check_with(operand);
            let expected = if accepted {
                Ok(())
            } else {
                Err(SchemaError::CheckOperandTypeMismatch(constraint.clone()))
            };

            assert_eq!(
                accounts_schema(column_type, vec![constraint]).validate(),
                expected
            );
        }
    }
}
