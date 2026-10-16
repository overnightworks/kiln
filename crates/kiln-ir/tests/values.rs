//! D9's checked multiply and conditional value, driven through `kiln_ir::run`
//! (`specs/kiln-ir.spec.md`, KILN-IR-84 to KILN-IR-86).

mod support;

use kiln_ir::{
    AbortReason, ArithmeticOp, Column, ColumnType, Condition, Effect, Expr, FunctionFault,
    IntegerWidth, KeyComponentFault, KeyDeclaration, Outcome, OverflowReason, ProgramError,
    Relation, Row, Schema, Slot, Store, Table, Terminal, Value,
};
use support::{
    aborted_reason, call_under_test, column, columns_of, columns_of_values, committed_response,
    integer, key_expr, keyed_by_id, literal_key, one_part, parameter, program_with_schema, run_ok,
    run_write, single_function_program, table, text, typed_parameter, WriteVia,
};

fn int_column(name: &str) -> Column {
    Column {
        name: column(name),
        kind: ColumnType::Integer {
            width: IntegerWidth::I64,
            domain: None,
        },
    }
}

fn literal(value: i64) -> Expr {
    Expr::Literal(integer(value))
}

fn text_literal(value: &str) -> Expr {
    Expr::Literal(text(value))
}

fn times(left: Expr, right: Expr) -> Expr {
    Expr::Arithmetic(ArithmeticOp::Multiply, Box::new(left), Box::new(right))
}

fn argument(name: &str) -> Expr {
    Expr::Parameter(parameter(name))
}

fn when(condition: Condition, when_true: Expr, when_false: Expr) -> Expr {
    Expr::Conditional {
        condition: Box::new(condition),
        when_true: Box::new(when_true),
        when_false: Box::new(when_false),
    }
}

fn equal(left: Expr, right: Expr) -> Condition {
    Condition::Compare {
        left,
        relation: Relation::Equal,
        right,
    }
}

/// A projection of a slot no read ever bound, which aborts when it runs.
fn projection_that_aborts() -> Expr {
    Expr::Projection {
        slot: Slot(0),
        column: column("c"),
    }
}

fn response_of(response: Expr, arguments: Vec<Value>) -> Outcome {
    let parameters = ["a", "b"]
        .iter()
        .take(arguments.len())
        .map(|name| typed_parameter(name, IntegerWidth::I64))
        .collect();
    let program = single_function_program(parameters, vec![], Terminal::Return(response));
    run_ok(&program, &call_under_test(arguments), &Store::empty()).outcome
}

fn committed(value: Value) -> Outcome {
    Outcome::Committed { response: value }
}

mod kiln_ir_84 {
    use super::*;

    #[test]
    fn a_product_is_exact_and_a_scaled_product_adds_the_scales() {
        let order_line_amount_at_scale_2 = (integer(3), integer(1999), integer(5997));
        let price_at_scale_2_times_rate_at_scale_2 =
            (integer(1999), integer(825), integer(1_649_175));
        let negative = (integer(-4), integer(25), integer(-100));
        for (quantity, price, amount) in [
            order_line_amount_at_scale_2,
            price_at_scale_2_times_rate_at_scale_2,
            negative,
        ] {
            let outcome = response_of(times(argument("a"), argument("b")), vec![quantity, price]);

            assert_eq!(outcome, committed(amount));
        }
    }

    #[test]
    fn an_order_line_amount_is_the_exact_product_of_quantity_and_a_scale_2_price_read_from_the_item(
    ) {
        let item_price_19_99_at_scale_2 = Row {
            columns: columns_of_values([("i_price", integer(1999))]),
        };
        let item = keyed_by_id("item", vec![int_column("i_price")]);
        let order_line = keyed_by_id("order_line", vec![int_column("ol_amount")]);
        let amount = times(
            argument("quantity"),
            Expr::Projection {
                slot: Slot(0),
                column: column("i_price"),
            },
        );
        let program = program_with_schema(
            Schema {
                tables: vec![item, order_line],
                ..Default::default()
            },
            vec![typed_parameter("quantity", IntegerWidth::I64)],
            vec![
                Effect::Read {
                    table: table("item"),
                    key: literal_key(1),
                    into: Slot(0),
                },
                Effect::Insert {
                    table: table("order_line"),
                    key: literal_key(1),
                    columns: columns_of([("ol_amount", amount)]),
                },
            ],
            Terminal::Return(literal(0)),
        );
        let store = Store::containing([(table("item"), one_part(1), item_price_19_99_at_scale_2)]);

        let execution = run_ok(&program, &call_under_test(vec![integer(3)]), &store);

        let line = execution.store.row(&table("order_line"), &one_part(1));
        assert_eq!(
            line.map(|row| row.columns.clone()),
            Some(columns_of_values([("ol_amount", integer(5997))])),
            "3 x 19.99 is 59.97, an integer at scale 2"
        );
    }

    #[test]
    fn a_product_beyond_the_integer_range_aborts_instead_of_wrapping() {
        for (left, right) in [(i64::MAX, 2), (i64::MIN, -1), (1 << 32, 1 << 32)] {
            let outcome = response_of(times(literal(left), literal(right)), vec![]);

            assert_eq!(
                outcome,
                Outcome::Aborted {
                    reason: AbortReason::Overflow(OverflowReason::Arithmetic)
                }
            );
        }
    }

    #[test]
    fn a_product_beyond_the_declared_width_aborts_without_a_domain_and_is_not_clamped() {
        let narrow = Column {
            name: column("c"),
            kind: ColumnType::Integer {
                width: IntegerWidth::I8,
                domain: None,
            },
        };

        let execution = run_write(narrow, times(literal(16), literal(16)), WriteVia::Insert);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::Overflow(OverflowReason::Width {
                    table: table("t"),
                    column: column("c"),
                    key: one_part(1),
                })
            }
        );
        assert_eq!(execution.store.row(&table("t"), &one_part(1)), None);
    }

    #[test]
    fn a_product_with_a_text_operand_aborts_typed() {
        let outcome = response_of(times(text_literal("x"), literal(2)), vec![]);

        assert_eq!(
            outcome,
            Outcome::Aborted {
                reason: AbortReason::NonIntegerOperand
            }
        );
    }
}

mod kiln_ir_85 {
    use super::*;

    #[test]
    fn a_conditional_value_is_the_arm_its_condition_chooses() {
        for (left, expected) in [(1, 10), (2, 20)] {
            let value = when(equal(literal(left), literal(1)), literal(10), literal(20));

            assert_eq!(response_of(value, vec![]), committed(integer(expected)));
        }
    }

    #[test]
    fn the_unchosen_arm_never_runs_so_its_failing_projection_never_aborts() {
        let unchosen_false = when(
            equal(literal(1), literal(1)),
            literal(7),
            projection_that_aborts(),
        );
        let unchosen_true = when(
            equal(literal(1), literal(2)),
            projection_that_aborts(),
            literal(8),
        );

        assert_eq!(response_of(unchosen_false, vec![]), committed(integer(7)));
        assert_eq!(response_of(unchosen_true, vec![]), committed(integer(8)));
    }

    #[test]
    fn the_chosen_arm_aborts_on_its_failing_projection() {
        let value = when(
            equal(literal(1), literal(1)),
            projection_that_aborts(),
            literal(8),
        );

        assert_eq!(
            response_of(value, vec![]),
            Outcome::Aborted {
                reason: AbortReason::ProjectionSourceAbsent { slot: Slot(0) }
            }
        );
    }

    #[test]
    fn a_condition_on_a_read_chooses_by_whether_the_row_was_found() {
        let value = when(Condition::RowPresent(Slot(0)), literal(1), literal(2));
        let read = Effect::Read {
            table: table("t"),
            key: key_expr(&one_part(1)),
            into: Slot(0),
        };
        let program = single_function_program(vec![], vec![read], Terminal::Return(value));

        let outcome = run_ok(&program, &call_under_test(vec![]), &Store::empty());

        assert_eq!(committed_response(&outcome), &integer(2));
    }

    #[test]
    fn a_condition_comparing_an_integer_with_text_aborts_typed() {
        let value = when(equal(literal(1), text_literal("1")), literal(1), literal(2));
        let program = single_function_program(vec![], vec![], Terminal::Return(value));

        let execution = run_ok(&program, &call_under_test(vec![]), &Store::empty());

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::ComparisonTypeMismatch
        );
    }
}

mod kiln_ir_86 {
    use super::*;

    fn read_keyed_by(component: Expr) -> Vec<Effect> {
        vec![Effect::Read {
            table: table("t"),
            key: vec![component],
            into: Slot(1),
        }]
    }

    fn validation_of(component: Expr) -> Result<Outcome, ProgramError> {
        let program = single_function_program(
            vec![typed_parameter("n", IntegerWidth::I64)],
            read_keyed_by(component),
            Terminal::Return(literal(0)),
        );
        kiln_ir::run(
            &program,
            &call_under_test(vec![integer(1)]),
            &Store::empty(),
        )
        .map(|execution| execution.outcome)
    }

    fn wrong_kind() -> ProgramError {
        ProgramError::Function {
            function: kiln_ir::FunctionName("under_test".to_owned()),
            fault: FunctionFault::KeyComponent {
                table: table("t"),
                column: column("id"),
                fault: KeyComponentFault::WrongKind,
            },
        }
    }

    fn condition() -> Condition {
        equal(argument("n"), literal(1))
    }

    #[test]
    fn a_product_is_an_integer_so_it_is_rejected_as_a_text_key_component() {
        let text_keyed = Table {
            name: table("t"),
            columns: vec![Column {
                name: column("id"),
                kind: ColumnType::Text,
            }],
            key: KeyDeclaration::Columns(vec![column("id")]),
        };
        let schema = Schema {
            tables: vec![text_keyed],
            ..Default::default()
        };
        let program = program_with_schema(
            schema,
            vec![],
            read_keyed_by(times(literal(2), literal(3))),
            Terminal::Return(literal(0)),
        );

        let result = kiln_ir::run(&program, &call_under_test(vec![]), &Store::empty());

        assert_eq!(result.map(|execution| execution.outcome), Err(wrong_kind()));
    }

    #[test]
    fn a_conditional_of_text_arms_is_rejected_as_an_integer_key_component() {
        let component = when(condition(), text_literal("a"), text_literal("b"));

        assert_eq!(validation_of(component), Err(wrong_kind()));
    }

    #[test]
    fn a_conditional_of_integer_arms_is_accepted_as_an_integer_key_component() {
        let component = when(condition(), literal(1), times(argument("n"), literal(2)));

        assert!(validation_of(component).is_ok());
    }

    #[test]
    fn a_conditional_whose_arms_differ_in_kind_is_left_to_the_run() {
        let component = when(condition(), literal(1), text_literal("b"));

        assert!(validation_of(component).is_ok());
    }
}
