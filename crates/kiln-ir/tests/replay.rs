//! `replay` over a call sequence, driven through `kiln_ir::replay`
//! (`specs/kiln-ir.spec.md`).

mod support;

use kiln_ir::{
    replay, AbortName, AbortReason, Call, Column, ColumnType, Effect, Execution, Expr,
    IntegerWidth, Outcome, ProgramError, Schema, SchemaError, Store, Terminal, Value,
};
use proptest::prelude::*;
use support::{
    call_under_test, column, insert_row, integer, keyed_by_id, program_with_schema, replayed,
    store_of_rows, table, typed_parameter,
};

fn echo_program() -> kiln_ir::Program {
    let argument = kiln_ir::ParameterName("n".to_owned());
    program_with_schema(
        Schema::default(),
        vec![typed_parameter("n", IntegerWidth::I8)],
        vec![],
        Terminal::Return(Expr::Parameter(argument)),
    )
}

fn insert_by_key_program() -> kiln_ir::Program {
    let key = vec![Expr::Parameter(kiln_ir::ParameterName("key".to_owned()))];
    let schema = Schema {
        tables: vec![keyed_by_id(
            "items",
            vec![Column {
                name: column("n"),
                kind: ColumnType::Integer {
                    width: IntegerWidth::I8,
                    domain: None,
                },
            }],
        )],
        ..Default::default()
    };
    program_with_schema(
        schema,
        vec![typed_parameter("key", IntegerWidth::I64)],
        vec![Effect::Insert {
            table: table("items"),
            key,
            columns: support::columns_of([("n", Expr::Literal(integer(1)))]),
        }],
        Terminal::Return(Expr::Literal(integer(0))),
    )
}

fn response_of(execution: &Execution) -> Option<&Value> {
    match &execution.outcome {
        Outcome::Committed { response } => Some(response),
        Outcome::Aborted { .. } => None,
    }
}

mod kiln_ir_58 {
    use super::*;

    #[test]
    fn empty_sequence_returns_no_executions() {
        assert!(replayed(&echo_program(), &[], &Store::empty()).is_empty());
    }

    #[test]
    fn each_call_keeps_its_position_when_one_aborts() {
        let calls: Vec<Call> = [3, 500, 7]
            .into_iter()
            .map(|n| call_under_test(vec![integer(n)]))
            .collect();

        let executions = replayed(&echo_program(), &calls, &Store::empty());

        let responses: Vec<Option<&Value>> = executions.iter().map(response_of).collect();
        assert_eq!(responses, vec![Some(&integer(3)), None, Some(&integer(7))]);
    }

    proptest! {
        #[test]
        fn results_line_up_with_their_calls(arguments in prop::collection::vec(-200i64..200, 0..12)) {
            let calls: Vec<Call> = arguments
                .iter()
                .map(|&n| call_under_test(vec![integer(n)]))
                .collect();

            let executions = replayed(&echo_program(), &calls, &Store::empty());

            prop_assert_eq!(executions.len(), calls.len());
            for (argument, execution) in arguments.iter().zip(&executions) {
                let expected = i8::try_from(*argument).is_ok().then(|| integer(*argument));
                prop_assert_eq!(response_of(execution), expected.as_ref());
            }
        }
    }
}

mod kiln_ir_59 {
    use super::*;

    fn insert_call(key: i64) -> Call {
        call_under_test(vec![integer(key)])
    }

    #[test]
    fn committed_changes_are_visible_to_later_calls_and_aborts_change_nothing() {
        let calls = [insert_call(1), insert_call(1), insert_call(2)];

        let executions = replayed(&insert_by_key_program(), &calls, &Store::empty());

        let one = store_of_rows(&[("items", 1, vec![("n", integer(1))])]);
        let one_and_two = store_of_rows(&[
            ("items", 1, vec![("n", integer(1))]),
            ("items", 2, vec![("n", integer(1))]),
        ]);
        assert_eq!(executions[0].store, one);
        assert!(matches!(
            executions[1].outcome,
            Outcome::Aborted {
                reason: AbortReason::DuplicateKey { .. }
            }
        ));
        assert_eq!(executions[1].store, one);
        assert_eq!(executions[2].store, one_and_two);
    }

    proptest! {
        #[test]
        fn every_call_runs_as_run_would_on_the_previous_store(
            keys in prop::collection::vec(0i64..4, 0..10)
        ) {
            let program = insert_by_key_program();
            let calls: Vec<Call> = keys.iter().map(|&key| insert_call(key)).collect();

            let executions = replayed(&program, &calls, &Store::empty());

            let mut previous = Store::empty();
            for (call, execution) in calls.iter().zip(&executions) {
                let expected = kiln_ir::run(&program, call, &previous).expect("valid schema");
                prop_assert_eq!(execution, &expected);
                previous = expected.store;
            }
        }
    }
}

mod kiln_ir_60 {
    use super::*;

    #[test]
    fn invalid_schema_is_a_typed_error() {
        let reversed = kiln_ir::ClampDomain { min: 5, max: 1 };
        let schema = Schema {
            tables: vec![keyed_by_id(
                "t",
                vec![Column {
                    name: column("c"),
                    kind: ColumnType::Integer {
                        width: IntegerWidth::I8,
                        domain: Some(reversed),
                    },
                }],
            )],
            ..Default::default()
        };
        let program = program_with_schema(
            schema,
            vec![],
            vec![insert_row("t", 1, [("c", integer(1))])],
            Terminal::Abort(AbortName("never".to_owned())),
        );

        for calls in [vec![], vec![call_under_test(vec![])]] {
            let result = replay(&program, &calls, &Store::empty());

            assert!(matches!(
                result,
                Err(ProgramError::Schema(SchemaError::ReversedDomain { .. }))
            ));
        }
    }
}
