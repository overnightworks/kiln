//! D9's text concatenation, truncation and containment, driven through
//! `kiln_ir::run` (`specs/kiln-ir.spec.md`, KILN-IR-94 to KILN-IR-96).

mod support;

use kiln_ir::{
    AbortReason, Column, ColumnType, Condition, Effect, Expr, FunctionFault, IntegerWidth,
    KeyComponentFault, KeyDeclaration, Outcome, ProgramError, Row, Schema, Slot, Store, Table,
    Terminal, Value,
};
use support::{
    call_under_test, column, columns_of, columns_of_values, integer, one_part, parameter,
    program_with_schema, run_ok, single_function_program, table, text, text_parameter,
    typed_parameter,
};

fn text_literal(value: &str) -> Expr {
    Expr::Literal(text(value))
}

fn integer_literal(value: i64) -> Expr {
    Expr::Literal(integer(value))
}

fn argument(name: &str) -> Expr {
    Expr::Parameter(parameter(name))
}

fn concat(left: Expr, right: Expr) -> Expr {
    Expr::Concat(Box::new(left), Box::new(right))
}

fn truncate(text: Expr, length: Expr) -> Expr {
    Expr::Truncate {
        text: Box::new(text),
        length: Box::new(length),
    }
}

fn contains(text: Expr, part: Expr) -> Condition {
    Condition::Contains { text, part }
}

fn when(condition: Condition, when_true: Expr, when_false: Expr) -> Expr {
    Expr::Conditional {
        condition: Box::new(condition),
        when_true: Box::new(when_true),
        when_false: Box::new(when_false),
    }
}

fn response_of(response: Expr) -> Outcome {
    let program = single_function_program(vec![], vec![], Terminal::Return(response));
    run_ok(&program, &call_under_test(vec![]), &Store::empty()).outcome
}

fn committed_text(value: &str) -> Outcome {
    Outcome::Committed {
        response: text(value),
    }
}

fn aborted(reason: AbortReason) -> Outcome {
    Outcome::Aborted { reason }
}

mod kiln_ir_94 {
    use super::*;

    #[test]
    fn concatenation_joins_its_operands_in_order() {
        let program = single_function_program(
            vec![text_parameter("name")],
            vec![],
            Terminal::Return(concat(
                concat(text_literal("Hello, "), argument("name")),
                text_literal("!"),
            )),
        );

        let execution = run_ok(
            &program,
            &call_under_test(vec![text("Ada")]),
            &Store::empty(),
        );

        assert_eq!(execution.outcome, committed_text("Hello, Ada!"));
    }

    #[test]
    fn concatenating_the_empty_text_changes_nothing() {
        let outcome = response_of(concat(text_literal(""), text_literal("x")));

        assert_eq!(outcome, committed_text("x"));
    }

    #[test]
    fn truncation_keeps_the_first_n_unicode_scalar_values() {
        let two_bytes_each = ("héllo", 2, "hé");
        let four_bytes_for_one = ("a😀b", 2, "a😀");
        let combining_mark_is_its_own_scalar = ("e\u{301}x", 1, "e");
        for (source, length, kept) in [
            two_bytes_each,
            four_bytes_for_one,
            combining_mark_is_its_own_scalar,
        ] {
            let outcome = response_of(truncate(text_literal(source), integer_literal(length)));

            assert_eq!(outcome, committed_text(kept), "{source:?} cut to {length}");
        }
    }

    #[test]
    fn truncation_leaves_a_text_no_longer_than_the_length_whole() {
        for length in [3, 4, 100] {
            let outcome = response_of(truncate(text_literal("abc"), integer_literal(length)));

            assert_eq!(outcome, committed_text("abc"), "cut to {length}");
        }
    }

    #[test]
    fn truncation_to_length_zero_is_the_empty_text() {
        let outcome = response_of(truncate(text_literal("abc"), integer_literal(0)));

        assert_eq!(outcome, committed_text(""));
    }

    #[test]
    fn truncation_to_a_negative_length_aborts_typed() {
        let outcome = response_of(truncate(text_literal("abc"), integer_literal(-1)));

        assert_eq!(outcome, aborted(AbortReason::NegativeTextLength));
    }

    #[test]
    fn a_non_text_operand_of_a_text_form_aborts_typed() {
        let forms = [
            concat(integer_literal(1), text_literal("x")),
            concat(text_literal("x"), integer_literal(1)),
            truncate(integer_literal(1), integer_literal(1)),
        ];
        for form in forms {
            assert_eq!(response_of(form), aborted(AbortReason::NonTextOperand));
        }
    }

    #[test]
    fn a_non_integer_truncation_length_aborts_typed() {
        let outcome = response_of(truncate(text_literal("abc"), text_literal("2")));

        assert_eq!(outcome, aborted(AbortReason::NonIntegerOperand));
    }

    #[test]
    fn a_concatenation_longer_than_either_operand_is_written_whole_to_a_text_column() {
        let notes = Table {
            name: table("notes"),
            columns: vec![
                support::id_column(),
                Column {
                    name: column("data"),
                    kind: ColumnType::Text,
                },
            ],
            key: KeyDeclaration::Columns(vec![column("id")]),
        };
        let long = "x".repeat(600);
        let program = program_with_schema(
            Schema {
                tables: vec![notes],
                ..Default::default()
            },
            vec![text_parameter("history")],
            vec![Effect::Insert {
                table: table("notes"),
                key: support::literal_key(1),
                columns: columns_of([("data", concat(argument("history"), text_literal("|new")))]),
            }],
            Terminal::Return(integer_literal(0)),
        );

        let execution = run_ok(
            &program,
            &call_under_test(vec![text(&long)]),
            &Store::empty(),
        );

        let written = execution.store.row(&table("notes"), &one_part(1));
        assert_eq!(
            written.map(|row| row.columns.clone()),
            Some(columns_of_values([("data", text(&format!("{long}|new")))])),
            "D3 declares no text width, so nothing clamps or aborts"
        );
    }
}

mod kiln_ir_95 {
    use super::*;

    fn decided(condition: Condition) -> Outcome {
        response_of(when(condition, integer_literal(1), integer_literal(0)))
    }

    fn holds() -> Outcome {
        Outcome::Committed {
            response: integer(1),
        }
    }

    fn fails() -> Outcome {
        Outcome::Committed {
            response: integer(0),
        }
    }

    #[test]
    fn containment_holds_when_the_part_occurs_anywhere_in_the_text() {
        for (source, part) in [
            ("PART ORIGINAL X", "ORIGINAL"),
            ("ORIGINAL", "ORIGINAL"),
            ("a😀b", "😀"),
        ] {
            let outcome = decided(contains(text_literal(source), text_literal(part)));

            assert_eq!(outcome, holds(), "{part:?} in {source:?}");
        }
    }

    #[test]
    fn containment_fails_when_the_part_does_not_occur_and_is_case_sensitive() {
        for (source, part) in [("original", "ORIGINAL"), ("ORIG", "ORIGINAL"), ("", "x")] {
            let outcome = decided(contains(text_literal(source), text_literal(part)));

            assert_eq!(outcome, fails(), "{part:?} in {source:?}");
        }
    }

    #[test]
    fn every_text_contains_the_empty_text() {
        let outcome = decided(contains(text_literal(""), text_literal("")));

        assert_eq!(outcome, holds());
    }

    #[test]
    fn containment_decides_a_branch_like_any_other_condition() {
        let branch = |arm: i64| kiln_ir::Block {
            effects: vec![],
            terminal: Terminal::Return(integer_literal(arm)),
        };
        let program = single_function_program(
            vec![text_parameter("data")],
            vec![],
            Terminal::Branch {
                condition: contains(argument("data"), text_literal("ORIGINAL")),
                when_present: Box::new(branch(1)),
                when_absent: Box::new(branch(0)),
            },
        );
        for (data, arm) in [("an ORIGINAL part", 1), ("a copy", 0)] {
            let execution = run_ok(
                &program,
                &call_under_test(vec![text(data)]),
                &Store::empty(),
            );

            assert_eq!(
                execution.outcome,
                Outcome::Committed {
                    response: integer(arm)
                }
            );
        }
    }

    #[test]
    fn a_non_text_operand_of_containment_aborts_typed() {
        let conditions = [
            contains(integer_literal(1), text_literal("x")),
            contains(text_literal("x"), integer_literal(1)),
        ];
        for condition in conditions {
            assert_eq!(decided(condition), aborted(AbortReason::NonTextOperand));
        }
    }

    #[test]
    fn a_failing_projection_in_a_containment_operand_aborts_with_its_own_reason() {
        let unbound = Expr::Projection {
            slot: Slot(0),
            column: column("c"),
        };

        let outcome = decided(contains(unbound, text_literal("x")));

        assert_eq!(
            outcome,
            aborted(AbortReason::ProjectionSourceAbsent { slot: Slot(0) })
        );
    }
}

mod kiln_ir_96 {
    use super::*;

    fn text_keyed_table() -> Table {
        Table {
            name: table("t"),
            columns: vec![Column {
                name: column("id"),
                kind: ColumnType::Text,
            }],
            key: KeyDeclaration::Columns(vec![column("id")]),
        }
    }

    fn integer_keyed_table() -> Table {
        support::keyed_by_id("t", vec![])
    }

    fn read_by(
        table: Table,
        component: Expr,
        arguments: Vec<Value>,
    ) -> Result<Outcome, ProgramError> {
        let program = program_with_schema(
            Schema {
                tables: vec![table],
                ..Default::default()
            },
            vec![text_parameter("s"), typed_parameter("n", IntegerWidth::I64)],
            vec![Effect::Read {
                table: self::table("t"),
                key: vec![component],
                into: Slot(1),
            }],
            Terminal::Return(integer_literal(0)),
        );
        kiln_ir::run(&program, &call_under_test(arguments), &Store::empty())
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

    fn text_forms() -> Vec<Expr> {
        vec![
            concat(argument("s"), text_literal("x")),
            truncate(argument("s"), argument("n")),
        ]
    }

    #[test]
    fn a_text_form_is_rejected_as_an_integer_key_component() {
        for form in text_forms() {
            let result = read_by(integer_keyed_table(), form, vec![text("a"), integer(1)]);

            assert_eq!(result, Err(wrong_kind()));
        }
    }

    #[test]
    fn a_conditional_choosing_between_text_forms_is_rejected_as_an_integer_key_component() {
        let component = when(
            contains(argument("s"), text_literal("a")),
            concat(argument("s"), text_literal("x")),
            truncate(argument("s"), integer_literal(1)),
        );

        let result = read_by(
            integer_keyed_table(),
            component,
            vec![text("a"), integer(1)],
        );

        assert_eq!(result, Err(wrong_kind()));
    }

    #[test]
    fn a_text_form_is_accepted_as_a_text_key_component_and_runs() {
        for form in text_forms() {
            let result = read_by(text_keyed_table(), form, vec![text("a"), integer(1)]);

            assert!(result.is_ok(), "{result:?}");
        }
    }

    #[test]
    fn a_text_form_over_non_text_arguments_still_validates_and_aborts_at_run() {
        let result = read_by(
            text_keyed_table(),
            concat(argument("n"), argument("n")),
            vec![text("a"), integer(1)],
        );

        assert_eq!(result, Ok(aborted(AbortReason::NonTextOperand)));
    }

    #[test]
    fn a_text_form_in_a_text_column_row_key_finds_the_row() {
        let program = program_with_schema(
            Schema {
                tables: vec![text_keyed_table()],
                ..Default::default()
            },
            vec![],
            vec![Effect::Read {
                table: table("t"),
                key: vec![concat(text_literal("ab"), text_literal("c"))],
                into: Slot(1),
            }],
            Terminal::Branch {
                condition: Condition::RowPresent(Slot(1)),
                when_present: Box::new(kiln_ir::Block {
                    effects: vec![],
                    terminal: Terminal::Return(integer_literal(1)),
                }),
                when_absent: Box::new(kiln_ir::Block {
                    effects: vec![],
                    terminal: Terminal::Return(integer_literal(0)),
                }),
            },
        );
        let store =
            Store::containing([(table("t"), kiln_ir::Key(vec![text("abc")]), Row::default())]);

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Committed {
                response: integer(1)
            }
        );
    }
}
