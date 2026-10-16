//! D9's text concatenation, truncation and containment, driven through
//! `kiln_ir::run` (`specs/kiln-ir.spec.md`, KILN-IR-94 to KILN-IR-96).

mod support;

use kiln_ir::{
    run, AbortReason, Column, ColumnType, Condition, Effect, Expr, FunctionFault, IntegerWidth,
    KeyComponentFault, KeyDeclaration, Outcome, ProgramError, Row, Schema, Slot, Store, Table,
    Terminal, Value,
};
use support::text_forms::*;
use support::{
    call_under_test, column, columns_of, columns_of_values, integer, one_part, program_with_schema,
    run_ok, single_function_program, table, text, text_parameter, typed_parameter,
};

/// Integer when the argument `s` is "a", text otherwise: it has no static
/// kind, so only the run can tell it is no text.
fn integer_when_s_is_a() -> Expr {
    when(
        contains(argument("s"), text_literal("a")),
        integer_literal(1),
        text_literal("x"),
    )
}

/// Text when the argument `s` is "a", an integer otherwise.
fn text_when_s_is_a() -> Expr {
    when(
        contains(argument("s"), text_literal("a")),
        text_literal("2"),
        integer_literal(2),
    )
}

fn unbound_projection() -> Expr {
    Expr::Projection {
        slot: Slot(0),
        column: column("c"),
    }
}

fn response_given_s_is_a(response: Expr) -> Outcome {
    response_with(vec![text_parameter("s")], vec![text("a")], response)
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
        let outcome = response_with(
            vec![typed_parameter("n", IntegerWidth::I64)],
            vec![integer(-1)],
            truncate(text_literal("abc"), argument("n")),
        );

        assert_eq!(outcome, aborted(AbortReason::NegativeTextLength));
    }

    #[test]
    fn a_non_text_operand_of_a_text_form_aborts_typed() {
        let forms = [
            concat(integer_when_s_is_a(), text_literal("x")),
            concat(text_literal("x"), integer_when_s_is_a()),
            truncate(integer_when_s_is_a(), integer_literal(1)),
        ];
        for form in forms {
            assert_eq!(
                response_given_s_is_a(form),
                aborted(AbortReason::NonTextOperand)
            );
        }
    }

    #[test]
    fn a_non_integer_truncation_length_aborts_typed() {
        let outcome = response_given_s_is_a(truncate(text_literal("abc"), text_when_s_is_a()));

        assert_eq!(outcome, aborted(AbortReason::NonIntegerOperand));
    }

    #[test]
    fn the_first_failing_operand_of_a_text_form_decides_the_abort() {
        let non_text = AbortReason::NonTextOperand;
        let absent = AbortReason::ProjectionSourceAbsent { slot: Slot(0) };
        let cases = [
            (
                concat(integer_when_s_is_a(), unbound_projection()),
                non_text.clone(),
            ),
            (
                concat(unbound_projection(), integer_when_s_is_a()),
                absent.clone(),
            ),
            (
                truncate(integer_when_s_is_a(), unbound_projection()),
                non_text.clone(),
            ),
            (
                truncate(unbound_projection(), text_when_s_is_a()),
                absent.clone(),
            ),
            (
                truncate(integer_when_s_is_a(), argument("n")),
                non_text.clone(),
            ),
            (
                truncate(text_literal("abc"), text_when_s_is_a()),
                AbortReason::NonIntegerOperand,
            ),
        ];
        for (form, first_failure) in cases {
            let outcome = response_with(
                vec![text_parameter("s"), typed_parameter("n", IntegerWidth::I64)],
                vec![text("a"), integer(-1)],
                form.clone(),
            );

            assert_eq!(outcome, aborted(first_failure), "{form:?}");
        }
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
            contains(integer_when_s_is_a(), text_literal("x")),
            contains(text_literal("x"), integer_when_s_is_a()),
        ];
        for condition in conditions {
            let outcome =
                response_given_s_is_a(when(condition, integer_literal(1), integer_literal(0)));

            assert_eq!(outcome, aborted(AbortReason::NonTextOperand));
        }
    }

    #[test]
    fn a_failing_projection_in_a_containment_operand_aborts_with_its_own_reason() {
        let outcome = decided(contains(unbound_projection(), text_literal("x")));

        assert_eq!(
            outcome,
            aborted(AbortReason::ProjectionSourceAbsent { slot: Slot(0) })
        );
    }

    #[test]
    fn the_first_failing_operand_of_a_containment_decides_the_abort() {
        let cases = [
            (
                contains(integer_when_s_is_a(), unbound_projection()),
                AbortReason::NonTextOperand,
            ),
            (
                contains(unbound_projection(), integer_when_s_is_a()),
                AbortReason::ProjectionSourceAbsent { slot: Slot(0) },
            ),
        ];
        for (condition, first_failure) in cases {
            let outcome =
                response_given_s_is_a(when(condition, integer_literal(1), integer_literal(0)));

            assert_eq!(outcome, aborted(first_failure));
        }
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

    fn present_or_absent() -> Terminal {
        let returning = |response: i64| {
            Box::new(kiln_ir::Block {
                effects: vec![],
                terminal: Terminal::Return(integer_literal(response)),
            })
        };
        Terminal::Branch {
            condition: Condition::RowPresent(Slot(1)),
            when_present: returning(1),
            when_absent: returning(0),
        }
    }

    /// Reads `table` by `component` and answers 1 when a row is there; the
    /// store holds one row, keyed by the text `stored`.
    fn read_by_and_find(
        table: Table,
        component: Expr,
        arguments: Vec<Value>,
        stored: &str,
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
            present_or_absent(),
        );
        let store = Store::containing([(
            self::table("t"),
            kiln_ir::Key(vec![text(stored)]),
            Row::default(),
        )]);
        run(&program, &call_under_test(arguments), &store).map(|execution| execution.outcome)
    }

    fn read_by(
        table: Table,
        component: Expr,
        arguments: Vec<Value>,
    ) -> Result<Outcome, ProgramError> {
        read_by_and_find(table, component, arguments, "")
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

    fn found() -> Result<Outcome, ProgramError> {
        Ok(Outcome::Committed {
            response: integer(1),
        })
    }

    #[test]
    fn a_text_form_computed_from_parameters_finds_the_row_of_the_key_it_builds() {
        let forms = [
            (concat(argument("s"), text_literal("x")), "abx"),
            (truncate(argument("s"), argument("n")), "a"),
        ];
        for (form, stored) in forms {
            let result = read_by_and_find(
                text_keyed_table(),
                form,
                vec![text("ab"), integer(1)],
                stored,
            );

            assert_eq!(result, found());
        }
    }

    #[test]
    fn a_text_form_computed_from_parameters_misses_a_row_of_another_key() {
        let result = read_by_and_find(
            text_keyed_table(),
            truncate(argument("s"), argument("n")),
            vec![text("ab"), integer(1)],
            "ab",
        );

        assert_eq!(
            result,
            Ok(Outcome::Committed {
                response: integer(0)
            })
        );
    }
}
