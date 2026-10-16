//! The concept's example transactions (§1 sign-up, §2.2 replay, §3 doctors
//! on call), built from the IR types and replayed serially through
//! `kiln_ir::replay` (`specs/kiln-ir.spec.md`).

mod support;

use kiln_ir::{
    AbortName, AbortReason, ArithmeticOp, Block, Call, CheckConstraint, Column, ColumnType,
    Comparison, Condition, Effect, Execution, Expr, Function, FunctionName, IntegerWidth, Outcome,
    ParameterName, Program, Schema, Slot, Store, Terminal, UniqueConstraint, Value,
};
use support::{
    column, columns_of, integer, keyed_by_id, one_part, replayed, store_of_rows, table, text,
    text_parameter, typed_parameter,
};

fn named_call(function: &str, args: Vec<Value>) -> Call {
    Call {
        function: FunctionName(function.to_owned()),
        args,
    }
}

fn function(name: &str, parameters: Vec<kiln_ir::Parameter>, body: Block) -> Function {
    Function {
        name: FunctionName(name.to_owned()),
        parameters,
        body,
    }
}

fn argument(name: &str) -> Expr {
    Expr::Parameter(ParameterName(name.to_owned()))
}

fn declared_abort(name: &str) -> Terminal {
    Terminal::Abort(AbortName(name.to_owned()))
}

fn integer_column(name: &str) -> Column {
    Column {
        name: column(name),
        kind: ColumnType::Integer {
            width: IntegerWidth::I32,
            domain: None,
        },
    }
}

fn committed_with(response: i64) -> Outcome {
    Outcome::Committed {
        response: integer(response),
    }
}

fn outcomes(executions: &[Execution]) -> Vec<Outcome> {
    executions
        .iter()
        .map(|execution| execution.outcome.clone())
        .collect()
}

fn final_store(executions: &[Execution]) -> &Store {
    &executions.last().expect("a non-empty replay").store
}

mod kiln_ir_61 {
    use super::*;

    const QUOTA: i64 = 2;

    fn sold_at_most_quota() -> CheckConstraint {
        CheckConstraint {
            table: table("events"),
            column: column("sold"),
            comparison: Comparison::AtMost,
            operand: integer(QUOTA),
        }
    }

    fn sold_plus_one() -> Expr {
        Expr::Arithmetic(
            ArithmeticOp::Add,
            Box::new(Expr::Projection {
                slot: Slot(0),
                column: column("sold"),
            }),
            Box::new(Expr::Literal(integer(1))),
        )
    }

    fn ticket_program() -> Program {
        let schema = Schema {
            tables: vec![keyed_by_id("events", vec![integer_column("sold")])],
            check_constraints: vec![sold_at_most_quota()],
            ..Default::default()
        };
        let buy = Block {
            effects: vec![Effect::Read {
                table: table("events"),
                key: vec![argument("event")],
                into: Slot(0),
            }],
            terminal: Terminal::Branch {
                condition: Condition::RowPresent(Slot(0)),
                when_present: Box::new(Block {
                    effects: vec![Effect::Update {
                        table: table("events"),
                        key: vec![argument("event")],
                        columns: columns_of([("sold", sold_plus_one())]),
                    }],
                    terminal: Terminal::Return(sold_plus_one()),
                }),
                when_absent: Box::new(Block {
                    effects: vec![],
                    terminal: declared_abort("no_such_event"),
                }),
            },
        };
        Program {
            schema,
            functions: vec![function(
                "buy_ticket",
                vec![typed_parameter("event", IntegerWidth::I64)],
                buy,
            )],
        }
    }

    fn sold(event: i64, count: i64) -> support::RowSpec {
        ("events", event, vec![("sold", integer(count))])
    }

    #[test]
    fn purchase_past_the_quota_aborts_and_the_others_commit() {
        let calls: Vec<Call> = [1, 1, 1, 2]
            .into_iter()
            .map(|event| named_call("buy_ticket", vec![integer(event)]))
            .collect();

        let executions = replayed(
            &ticket_program(),
            &calls,
            &store_of_rows(&[sold(1, 0), sold(2, 0)]),
        );

        assert_eq!(
            outcomes(&executions),
            vec![
                committed_with(1),
                committed_with(2),
                Outcome::Aborted {
                    reason: AbortReason::CheckViolation {
                        constraint: sold_at_most_quota(),
                        key: one_part(1),
                    }
                },
                committed_with(1),
            ]
        );
        assert_eq!(
            final_store(&executions),
            &store_of_rows(&[sold(1, QUOTA), sold(2, 1)])
        );
    }
}

mod kiln_ir_62 {
    use super::*;

    pub(super) fn email_unique() -> UniqueConstraint {
        UniqueConstraint {
            table: table("users"),
            columns: vec![column("email")],
        }
    }

    pub(super) fn sign_up_program() -> Program {
        let schema = Schema {
            tables: vec![keyed_by_id(
                "users",
                vec![Column {
                    name: column("email"),
                    kind: ColumnType::Text,
                }],
            )],
            unique_constraints: vec![email_unique()],
            ..Default::default()
        };
        let sign_up = Block {
            effects: vec![Effect::Insert {
                table: table("users"),
                key: vec![argument("id")],
                columns: columns_of([("email", argument("email"))]),
            }],
            terminal: Terminal::Return(argument("id")),
        };
        Program {
            schema,
            functions: vec![function(
                "sign_up",
                vec![
                    typed_parameter("id", IntegerWidth::I64),
                    text_parameter("email"),
                ],
                sign_up,
            )],
        }
    }

    pub(super) fn user(id: i64, email: &str) -> support::RowSpec {
        ("users", id, vec![("email", text(email))])
    }

    #[test]
    fn taken_email_aborts_and_the_other_sign_ups_commit() {
        let calls = [
            named_call("sign_up", vec![integer(1), text("ada@example.org")]),
            named_call("sign_up", vec![integer(2), text("ada@example.org")]),
            named_call("sign_up", vec![integer(3), text("grace@example.org")]),
        ];

        let executions = replayed(&sign_up_program(), &calls, &Store::empty());

        assert_eq!(
            outcomes(&executions),
            vec![
                committed_with(1),
                Outcome::Aborted {
                    reason: AbortReason::UniqueViolation {
                        constraint: email_unique(),
                        tuple: vec![text("ada@example.org")],
                    }
                },
                committed_with(3),
            ]
        );
        assert_eq!(
            final_store(&executions),
            &store_of_rows(&[user(1, "ada@example.org"), user(3, "grace@example.org")])
        );
    }
}

mod kiln_ir_63 {
    use super::*;

    const LAST_DOCTOR: &str = "last_doctor_on_call";

    fn others_in(start: Expr, end: Expr, into: Slot) -> Effect {
        Effect::ReadRange {
            table: table("on_call"),
            start: vec![start],
            end: vec![end],
            bound: Expr::Literal(integer(100)),
            into,
        }
    }

    fn shifted(operator: ArithmeticOp) -> Expr {
        Expr::Arithmetic(
            operator,
            Box::new(argument("doctor")),
            Box::new(Expr::Literal(integer(1))),
        )
    }

    fn leave_and_return_doctor() -> Block {
        Block {
            effects: vec![Effect::Delete {
                table: table("on_call"),
                key: vec![argument("doctor")],
            }],
            terminal: Terminal::Return(argument("doctor")),
        }
    }

    fn go_off_call() -> Block {
        let any_after = Block {
            effects: vec![],
            terminal: Terminal::Branch {
                condition: Condition::RowPresent(Slot(1)),
                when_present: Box::new(leave_and_return_doctor()),
                when_absent: Box::new(Block {
                    effects: vec![],
                    terminal: declared_abort(LAST_DOCTOR),
                }),
            },
        };
        Block {
            effects: vec![
                others_in(
                    Expr::Literal(integer(i64::MIN)),
                    shifted(ArithmeticOp::Subtract),
                    Slot(0),
                ),
                others_in(
                    shifted(ArithmeticOp::Add),
                    Expr::Literal(integer(i64::MAX)),
                    Slot(1),
                ),
            ],
            terminal: Terminal::Branch {
                condition: Condition::RowPresent(Slot(0)),
                when_present: Box::new(leave_and_return_doctor()),
                when_absent: Box::new(any_after),
            },
        }
    }

    fn come_on_call() -> Block {
        Block {
            effects: vec![Effect::Insert {
                table: table("on_call"),
                key: vec![argument("doctor")],
                columns: Default::default(),
            }],
            terminal: Terminal::Return(argument("doctor")),
        }
    }

    fn rota_program() -> Program {
        let doctor = || vec![typed_parameter("doctor", IntegerWidth::I64)];
        Program {
            schema: Schema {
                tables: vec![keyed_by_id("on_call", vec![])],
                ..Default::default()
            },
            functions: vec![
                function("go_off_call", doctor(), go_off_call()),
                function("come_on_call", doctor(), come_on_call()),
            ],
        }
    }

    fn on_call(doctors: &[i64]) -> Store {
        let rows: Vec<support::RowSpec> = doctors
            .iter()
            .map(|&doctor| ("on_call", doctor, vec![]))
            .collect();
        store_of_rows(&rows)
    }

    #[test]
    fn leaving_nobody_on_call_aborts_and_the_other_calls_commit() {
        let calls = [
            named_call("go_off_call", vec![integer(1)]),
            named_call("go_off_call", vec![integer(2)]),
            named_call("come_on_call", vec![integer(3)]),
            named_call("go_off_call", vec![integer(2)]),
        ];

        let executions = replayed(&rota_program(), &calls, &on_call(&[1, 2]));

        assert_eq!(
            outcomes(&executions),
            vec![
                committed_with(1),
                Outcome::Aborted {
                    reason: AbortReason::Declared(AbortName(LAST_DOCTOR.to_owned()))
                },
                committed_with(3),
                committed_with(2),
            ]
        );
        assert_eq!(final_store(&executions), &on_call(&[3]));
    }

    #[test]
    fn a_doctor_before_the_leaver_keeps_the_rota_staffed() {
        let call = named_call("go_off_call", vec![integer(2)]);

        let executions = replayed(&rota_program(), &[call], &on_call(&[1, 2]));

        assert_eq!(final_store(&executions), &on_call(&[1]));
    }
}

mod kiln_ir_64 {
    use super::kiln_ir_62::{email_unique, sign_up_program, user};
    use super::*;

    fn balance(account: i64, amount: i64) -> support::RowSpec {
        ("accounts", account, vec![("balance", integer(amount))])
    }

    fn balance_moved(slot: Slot, operator: ArithmeticOp) -> Expr {
        Expr::Arithmetic(
            operator,
            Box::new(Expr::Projection {
                slot,
                column: column("balance"),
            }),
            Box::new(argument("amount")),
        )
    }

    fn transfer() -> Function {
        let body = Block {
            effects: vec![
                Effect::Read {
                    table: table("accounts"),
                    key: vec![argument("from")],
                    into: Slot(0),
                },
                Effect::Read {
                    table: table("accounts"),
                    key: vec![argument("to")],
                    into: Slot(1),
                },
                Effect::Update {
                    table: table("accounts"),
                    key: vec![argument("from")],
                    columns: columns_of([(
                        "balance",
                        balance_moved(Slot(0), ArithmeticOp::Subtract),
                    )]),
                },
                Effect::Update {
                    table: table("accounts"),
                    key: vec![argument("to")],
                    columns: columns_of([("balance", balance_moved(Slot(1), ArithmeticOp::Add))]),
                },
            ],
            terminal: Terminal::Return(argument("amount")),
        };
        function(
            "transfer",
            vec![
                typed_parameter("from", IntegerWidth::I64),
                typed_parameter("to", IntegerWidth::I64),
                typed_parameter("amount", IntegerWidth::I32),
            ],
            body,
        )
    }

    fn bank_with_sign_up() -> Program {
        let mut program = sign_up_program();
        program
            .schema
            .tables
            .push(keyed_by_id("accounts", vec![integer_column("balance")]));
        program.functions.push(transfer());
        program
    }

    #[test]
    fn an_aborted_sign_up_leaves_the_committed_transfer_before_it() {
        let calls = [
            named_call("transfer", vec![integer(1), integer(2), integer(10)]),
            named_call("sign_up", vec![integer(7), text("ada@example.org")]),
        ];
        let start = store_of_rows(&[balance(1, 100), balance(2, 0), user(5, "ada@example.org")]);

        let executions = replayed(&bank_with_sign_up(), &calls, &start);

        assert_eq!(
            outcomes(&executions),
            vec![
                committed_with(10),
                Outcome::Aborted {
                    reason: AbortReason::UniqueViolation {
                        constraint: email_unique(),
                        tuple: vec![text("ada@example.org")],
                    }
                },
            ]
        );
        assert_eq!(
            final_store(&executions),
            &store_of_rows(&[balance(1, 90), balance(2, 10), user(5, "ada@example.org")])
        );
    }
}
