//! D9's text forms that cannot work, rejected before any call, and those left
//! to the run (`specs/kiln-ir.spec.md`, KILN-IR-96).

mod support;

use kiln_ir::{
    run, AbortReason, Column, ColumnType, Effect, Expr, FunctionFault, IntegerWidth, Outcome,
    ProgramError, Row, Schema, Slot, Store, Table, Terminal, TextFormFault,
};
use support::text_forms::*;
use support::{
    call_under_test, column, columns_of, columns_of_values, integer, one_part, program_with_schema,
    run_ok, single_function_program, table, text, text_parameter, typed_parameter,
};

mod kiln_ir_96 {
    use super::*;

    fn rejected(fault: TextFormFault) -> Result<Outcome, ProgramError> {
        Err(ProgramError::Function {
            function: kiln_ir::FunctionName("under_test".to_owned()),
            fault: FunctionFault::TextForm(fault),
        })
    }

    fn response_declaring_s_and_n(response: Expr) -> Result<Outcome, ProgramError> {
        response_result(
            vec![text_parameter("s"), typed_parameter("n", IntegerWidth::I64)],
            vec![text("a"), integer(1)],
            response,
        )
    }

    #[test]
    fn a_literal_negative_truncation_length_is_rejected_before_the_call() {
        let result = response_declaring_s_and_n(truncate(argument("s"), integer_literal(-1)));

        assert_eq!(result, rejected(TextFormFault::NegativeLength));
    }

    #[test]
    fn a_text_form_operand_that_cannot_be_text_is_rejected_before_the_call() {
        let integer_sum = Expr::Arithmetic(
            kiln_ir::ArithmeticOp::Add,
            Box::new(integer_literal(1)),
            Box::new(argument("n")),
        );
        let not_text = [integer_literal(1), argument("n"), integer_sum];
        for operand in not_text {
            let forms = [
                concat(operand.clone(), text_literal("x")),
                concat(text_literal("x"), operand.clone()),
                truncate(operand.clone(), integer_literal(1)),
                when(
                    contains(operand.clone(), text_literal("x")),
                    argument("s"),
                    argument("s"),
                ),
                when(
                    contains(text_literal("x"), operand.clone()),
                    argument("s"),
                    argument("s"),
                ),
            ];
            for form in forms {
                let result = response_declaring_s_and_n(form.clone());

                assert_eq!(result, rejected(TextFormFault::OperandNotText), "{form:?}");
            }
        }
    }

    #[test]
    fn a_truncation_length_that_cannot_be_an_integer_is_rejected_before_the_call() {
        for length in [
            text_literal("2"),
            argument("s"),
            concat(argument("s"), argument("s")),
        ] {
            let result = response_declaring_s_and_n(truncate(argument("s"), length.clone()));

            assert_eq!(
                result,
                rejected(TextFormFault::LengthNotInteger),
                "{length:?}"
            );
        }
    }

    #[test]
    fn a_text_form_that_cannot_work_is_rejected_wherever_an_expression_stands() {
        let failing = || concat(integer_literal(1), text_literal("x"));
        let bodies = [
            single_function_program(vec![], vec![], Terminal::Return(failing())),
            single_function_program(
                vec![],
                vec![Effect::Read {
                    table: table("t"),
                    key: vec![Expr::Arithmetic(
                        kiln_ir::ArithmeticOp::Add,
                        Box::new(integer_literal(1)),
                        Box::new(Expr::Conditional {
                            condition: Box::new(contains(failing(), text_literal("x"))),
                            when_true: Box::new(integer_literal(1)),
                            when_false: Box::new(integer_literal(2)),
                        }),
                    )],
                    into: Slot(1),
                }],
                Terminal::Return(integer_literal(0)),
            ),
            single_function_program(
                vec![],
                vec![Effect::Insert {
                    table: table("t"),
                    key: support::literal_key(1),
                    columns: columns_of([("data", failing())]),
                }],
                Terminal::Return(integer_literal(0)),
            ),
            single_function_program(
                vec![],
                vec![],
                Terminal::Branch {
                    condition: contains(failing(), text_literal("x")),
                    when_present: Box::new(kiln_ir::Block {
                        effects: vec![],
                        terminal: Terminal::Return(integer_literal(1)),
                    }),
                    when_absent: Box::new(kiln_ir::Block {
                        effects: vec![],
                        terminal: Terminal::Return(integer_literal(0)),
                    }),
                },
            ),
        ];
        for program in bodies {
            let result = run(&program, &call_under_test(vec![]), &Store::empty());

            assert!(
                matches!(
                    result,
                    Err(ProgramError::Function {
                        fault: FunctionFault::TextForm(TextFormFault::OperandNotText),
                        ..
                    })
                ),
                "{result:?}"
            );
        }
    }

    #[test]
    fn a_projection_of_a_column_of_the_wrong_kind_in_a_text_form_is_rejected_before_the_call() {
        let projected = |name: &str| Expr::Projection {
            slot: Slot(1),
            column: column(name),
        };
        let forms = [
            (
                concat(projected("qty"), text_literal("x")),
                TextFormFault::OperandNotText,
            ),
            (
                truncate(text_literal("abc"), projected("data")),
                TextFormFault::LengthNotInteger,
            ),
        ];
        for (form, fault) in forms {
            let program = program_with_schema(
                Schema {
                    tables: vec![support::keyed_by_id(
                        "t",
                        vec![
                            Column {
                                name: column("qty"),
                                kind: ColumnType::Integer {
                                    width: IntegerWidth::I64,
                                    domain: None,
                                },
                            },
                            Column {
                                name: column("data"),
                                kind: ColumnType::Text,
                            },
                        ],
                    )],
                    ..Default::default()
                },
                vec![],
                vec![reading_slot_1_from("t")],
                Terminal::Return(form),
            );

            let result = run(&program, &call_under_test(vec![]), &Store::empty());

            assert_eq!(result.map(|_| ()), rejected(fault).map(|_| ()));
        }
    }

    fn table_with_column(name: &str, kind: ColumnType) -> Table {
        support::keyed_by_id(
            name,
            vec![Column {
                name: column("data"),
                kind,
            }],
        )
    }

    fn reading_slot_1_from(table: &str) -> Effect {
        Effect::Read {
            table: self::table(table),
            key: support::literal_key(1),
            into: Slot(1),
        }
    }

    fn truncating_the_data_of_slot_1() -> Expr {
        truncate(
            Expr::Projection {
                slot: Slot(1),
                column: column("data"),
            },
            integer_literal(1),
        )
    }

    fn arm_returning_data_read_from(table: &str) -> Box<kiln_ir::Block> {
        Box::new(kiln_ir::Block {
            effects: vec![reading_slot_1_from(table)],
            terminal: Terminal::Return(truncating_the_data_of_slot_1()),
        })
    }

    fn program_binding_slot_1_from_a_text_or_an_integer_column() -> kiln_ir::Program {
        program_with_schema(
            Schema {
                tables: vec![
                    table_with_column("words", ColumnType::Text),
                    table_with_column(
                        "counts",
                        ColumnType::Integer {
                            width: IntegerWidth::I64,
                            domain: None,
                        },
                    ),
                ],
                ..Default::default()
            },
            vec![text_parameter("s")],
            vec![],
            Terminal::Branch {
                condition: contains(argument("s"), text_literal("a")),
                when_present: arm_returning_data_read_from("words"),
                when_absent: arm_returning_data_read_from("counts"),
            },
        )
    }

    #[test]
    fn a_projection_whose_kind_depends_on_the_path_is_left_to_the_run() {
        let program = program_binding_slot_1_from_a_text_or_an_integer_column();
        let store = Store::containing([
            (
                table("words"),
                one_part(1),
                Row {
                    columns: columns_of_values([("data", text("hello"))]),
                },
            ),
            (
                table("counts"),
                one_part(1),
                Row {
                    columns: columns_of_values([("data", integer(7))]),
                },
            ),
        ]);
        let outcome_given =
            |s: &str| run_ok(&program, &call_under_test(vec![text(s)]), &store).outcome;

        assert_eq!(outcome_given("a"), committed_text("h"));
        assert_eq!(outcome_given("b"), aborted(AbortReason::NonTextOperand));
    }

    #[test]
    fn a_failing_form_in_an_arm_of_a_conditional_is_left_to_the_run() {
        let unchosen = concat(integer_literal(1), text_literal("x"));
        let response = when(
            contains(argument("s"), text_literal("a")),
            argument("s"),
            unchosen,
        );

        let outcome = response_with(vec![text_parameter("s")], vec![text("a")], response);

        assert_eq!(outcome, committed_text("a"));
    }
}
