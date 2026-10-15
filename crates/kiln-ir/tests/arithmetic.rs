//! D3's ordinary checked-arithmetic, width, and clamping-domain behaviour
//! (`docs/concept.md` §2.2): overflow, clamping, and the D1 declaration
//! rules arithmetic interacts with. Every width's own exact boundary and a
//! clamp domain's own validity live in `tests/arithmetic_boundaries.rs`
//! instead, and every other behaviour of `kiln_ir::run` lives in
//! `tests/evaluator.rs`.

mod support;

use kiln_ir::{
    AbortReason, AccessTarget, ArithmeticOp, ClampDomain, Column, Effect, Expr, IntegerWidth,
    Outcome, OverflowReason, Row, Store, Terminal, Value,
};
use proptest::prelude::*;
use support::{
    call_under_test, column, columns_of, program_with_column, run_ok, run_write, table,
    typed_parameter, WriteVia,
};

fn add(lhs: i64, rhs: i64) -> Expr {
    Expr::Arithmetic(
        ArithmeticOp::Add,
        Box::new(Expr::Literal(Value::Integer(lhs))),
        Box::new(Expr::Literal(Value::Integer(rhs))),
    )
}

// --- D3: arithmetic runs at a column's declared width; overflow aborts unless the column
// clamps (docs/concept.md §2.2) ---

mod kiln_ir_24 {
    use super::*;

    proptest! {
        #[test]
        fn computed_value_outside_declared_width_aborts_without_a_domain(
            lhs in -128i64..=127i64,
            rhs in -128i64..=127i64,
        ) {
            let declared_column = Column { name: column("c"), width: IntegerWidth::I8, domain: None };
            let execution = run_write(declared_column, add(lhs, rhs), WriteVia::Insert);

            let sum = lhs + rhs;
            if (i8::MIN as i64..=i8::MAX as i64).contains(&sum) {
                let row = execution
                    .store
                    .row(&table("t"), &Value::Integer(1))
                    .expect("a sum within the declared width commits");
                prop_assert_eq!(row.columns.get(&column("c")), Some(&Value::Integer(sum)));
            } else {
                prop_assert_eq!(
                    execution.outcome,
                    Outcome::Aborted {
                        reason: AbortReason::Overflow(OverflowReason::Width {
                            table: table("t"),
                            column: column("c"),
                            key: Value::Integer(1),
                        }),
                    }
                );
                // Overflow aborts like any other abort: every effect of this call is discarded.
                prop_assert_eq!(&execution.store, &Store::empty());
            }
        }
    }
}

mod kiln_ir_25 {
    use super::*;

    #[test]
    fn literal_write_outside_declared_width_aborts_without_a_domain() {
        let declared_column = Column {
            name: column("c"),
            width: IntegerWidth::I8,
            domain: None,
        };
        // 200 is outside i8's -128..=127, with no arithmetic involved at all.
        let execution = run_write(
            declared_column,
            Expr::Literal(Value::Integer(200)),
            WriteVia::Insert,
        );

        assert!(matches!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::Overflow(OverflowReason::Width { .. })
            }
        ));
        assert_eq!(&execution.store, &Store::empty());
    }
}

// --- D3: a value whose nested sub-expression already overflows i64 aborts arithmetically —
// never clamped, even into a column that declares a domain, since a clamp only ever judges a
// write's one final value (docs/concept.md §2.2) ---

mod kiln_ir_26 {
    use super::*;

    #[test]
    fn nested_i64_overflow_aborts_even_when_the_target_column_clamps() {
        for via in [WriteVia::Insert, WriteVia::Update] {
            let declared_column = Column {
                name: column("c"),
                width: IntegerWidth::I32,
                domain: Some(ClampDomain { min: 0, max: 1000 }),
            };
            // (i64::MAX + 1) + 0: the inner addition overflows i64 before the outer
            // addition, or the column's clamp, ever sees a value.
            let nested = Expr::Arithmetic(
                ArithmeticOp::Add,
                Box::new(add(i64::MAX, 1)),
                Box::new(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_write(declared_column, nested, via);

            assert_eq!(
                execution.outcome,
                Outcome::Aborted {
                    reason: AbortReason::Overflow(OverflowReason::Arithmetic),
                },
                "case {via:?}"
            );
        }
    }
}

// --- D3: intermediate arithmetic is checked at i64 only; a nested result that itself stays
// inside i64 but briefly exceeds the column's width still commits once the expression's one
// final value returns inside it (docs/concept.md §2.2) ---

mod kiln_ir_27 {
    use super::*;

    #[test]
    fn a_nested_result_that_only_briefly_exceeds_the_width_still_commits() {
        let declared_column = Column {
            name: column("c"),
            width: IntegerWidth::I8,
            domain: None,
        };
        // (127 + 1) - 1: the inner sum (128) exceeds i8's width, but the outer
        // subtraction brings the expression's one final value (127) back inside it.
        let nested = Expr::Arithmetic(
            ArithmeticOp::Subtract,
            Box::new(add(127, 1)),
            Box::new(Expr::Literal(Value::Integer(1))),
        );

        let execution = run_write(declared_column, nested, WriteVia::Insert);

        let row = execution
            .store
            .row(&table("t"), &Value::Integer(1))
            .expect("the final value is within the declared width");
        assert_eq!(row.columns.get(&column("c")), Some(&Value::Integer(127)));
    }
}

mod kiln_ir_29 {
    use super::*;

    #[test]
    fn overflow_abort_discards_every_effect_and_retains_every_attempted_access() {
        let declared_column = Column {
            name: column("c"),
            width: IntegerWidth::I8,
            domain: None,
        };
        let program = program_with_column(
            declared_column,
            vec![],
            vec![
                Effect::Insert {
                    table: table("t"),
                    key: Expr::Literal(Value::Integer(1)),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(10)))]),
                },
                Effect::Insert {
                    table: table("t"),
                    key: Expr::Literal(Value::Integer(2)),
                    columns: columns_of([("c", add(100, 100))]),
                },
            ],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );
        let store = Store::empty();

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::Overflow(OverflowReason::Width {
                    table: table("t"),
                    column: column("c"),
                    key: Value::Integer(2),
                }),
            }
        );
        // The first insert succeeded locally but is discarded along with the one that overflowed.
        assert_eq!(&execution.store, &store);
        // Both attempted inserts are still in the access log — an abort keeps every access it
        // attempted, not only the one that triggered it (D5).
        assert_eq!(execution.access_log.len(), 2);
        assert_eq!(
            execution.access_log[0].target,
            AccessTarget::Point(Value::Integer(1))
        );
        assert_eq!(
            execution.access_log[1].target,
            AccessTarget::Point(Value::Integer(2))
        );
    }
}

// --- D3: arithmetic overflowing i64 in a response, outside any column write, aborts with a
// typed reason instead of panicking (docs/concept.md §2.2) ---

mod kiln_ir_28 {
    use super::*;

    #[test]
    fn response_arithmetic_overflow_aborts_with_a_typed_reason() {
        let program =
            support::single_function_program(vec![], vec![], Terminal::Return(add(i64::MAX, 1)));
        let store = Store::empty();

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::Overflow(OverflowReason::Arithmetic),
            }
        );
        assert!(execution.access_log.is_empty());
        assert_eq!(&execution.store, &store);
    }
}

// --- D3: an arithmetic operand, or a write's own final value, that resolves to a non-integer
// value aborts with a typed reason instead of panicking — checked arithmetic and a write's
// width-or-clamp decision both run only over integers (docs/concept.md §2.2) ---

mod kiln_ir_16 {
    use super::*;

    #[test]
    fn arithmetic_operand_resolving_to_a_non_integer_value_aborts_with_a_typed_reason() {
        let expr = Expr::Arithmetic(
            ArithmeticOp::Add,
            Box::new(Expr::Literal(Value::Text("not a number".to_owned()))),
            Box::new(Expr::Literal(Value::Integer(1))),
        );
        let program = support::single_function_program(vec![], vec![], Terminal::Return(expr));
        let store = Store::empty();

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::NonIntegerOperand,
            }
        );
        assert!(execution.access_log.is_empty());
        assert_eq!(&execution.store, &store);
    }
}

mod kiln_ir_22 {
    use super::*;

    #[test]
    fn text_literal_written_into_a_declared_column_aborts_with_a_typed_reason() {
        let declared_column = Column {
            name: column("c"),
            width: IntegerWidth::I8,
            domain: None,
        };

        let execution = run_write(
            declared_column,
            Expr::Literal(Value::Text("not a number".to_owned())),
            WriteVia::Insert,
        );

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::NonIntegerOperand,
            }
        );
        assert_eq!(&execution.store, &Store::empty());
    }
}

// --- D3: a clamping domain saturates computed and literal writes alike, across the whole
// domain, instead of aborting (docs/concept.md §2.2) ---

mod kiln_ir_30 {
    use super::*;

    proptest! {
        #[test]
        fn computed_and_literal_writes_clamp_to_their_columns_domain_alike(
            operand_a in -200i64..=200i64,
            operand_b in -200i64..=200i64,
            domain_bound_a in -128i64..=127i64,
            domain_bound_b in -128i64..=127i64,
            via_arithmetic in any::<bool>(),
        ) {
            let domain_min = domain_bound_a.min(domain_bound_b);
            let domain_max = domain_bound_a.max(domain_bound_b);
            let raw_value = operand_a + operand_b;

            let declared_column = Column {
                name: column("c"),
                width: IntegerWidth::I32,
                domain: Some(ClampDomain { min: domain_min, max: domain_max }),
            };
            let write_expr = if via_arithmetic {
                add(operand_a, operand_b)
            } else {
                Expr::Literal(Value::Integer(raw_value))
            };

            let execution = run_write(declared_column, write_expr, WriteVia::Insert);

            let committed = matches!(execution.outcome, Outcome::Committed { .. });
            prop_assert!(committed);
            let row = execution
                .store
                .row(&table("t"), &Value::Integer(1))
                .expect("a clamped write always commits");
            let expected = raw_value.clamp(domain_min, domain_max);
            prop_assert_eq!(row.columns.get(&column("c")), Some(&Value::Integer(expected)));
        }
    }
}

mod kiln_ir_31 {
    use super::*;

    #[test]
    fn value_already_inside_the_domain_is_left_unchanged() {
        let declared_column = Column {
            name: column("c"),
            width: IntegerWidth::I32,
            domain: Some(ClampDomain { min: 0, max: 100 }),
        };

        let execution = run_write(
            declared_column,
            Expr::Literal(Value::Integer(42)),
            WriteVia::Insert,
        );

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        let row = execution
            .store
            .row(&table("t"), &Value::Integer(1))
            .expect("row committed");
        assert_eq!(row.columns.get(&column("c")), Some(&Value::Integer(42)));
    }
}

mod kiln_ir_32 {
    use super::*;

    /// A value whose arithmetic exceeds the column's own declared width still clamps into the
    /// domain instead of aborting — proving the clamp branch never routes through a width check
    /// first (the mutation a prior review found surviving: "domain branch width-checks before
    /// clamp"). The domain itself stays inside the width (D3 / `Schema::validate`); only the raw,
    /// pre-clamp arithmetic result exceeds it.
    #[test]
    fn overflow_into_a_clamped_column_clamps_even_past_the_columns_own_width() {
        for via in [WriteVia::Insert, WriteVia::Update] {
            let declared_column = Column {
                name: column("c"),
                width: IntegerWidth::I8,
                domain: Some(ClampDomain { min: 0, max: 100 }),
            };
            // 4000 + 1000 = 5000, far outside i8's -128..=127, but the domain still
            // saturates it to its own max instead of aborting on the width.
            let execution = run_write(declared_column, add(4000, 1000), via);

            let outcome = execution.outcome.clone();
            assert!(
                matches!(outcome, Outcome::Committed { .. }),
                "case {via:?}: expected a clamped commit, got {outcome:?}"
            );
            let row = execution
                .store
                .row(&table("t"), &Value::Integer(1))
                .expect("a clamped write always commits");
            assert_eq!(
                row.columns.get(&column("c")),
                Some(&Value::Integer(100)),
                "case {via:?}"
            );
        }
    }
}

// --- D1: a column the schema does not declare carries no width or domain, so a write to it
// resolves exactly as it always has (docs/concept.md §2.2) ---

mod kiln_ir_33 {
    use super::*;

    #[test]
    fn write_to_a_column_the_schema_does_not_declare_is_never_clamped_or_aborted() {
        let program = support::single_function_program(
            vec![],
            vec![Effect::Insert {
                table: table("t"),
                key: Expr::Literal(Value::Integer(1)),
                columns: columns_of([("c", Expr::Literal(Value::Integer(i64::MAX)))]),
            }],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );
        let store = Store::empty();

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        let row = execution
            .store
            .row(&table("t"), &Value::Integer(1))
            .expect("row committed");
        assert_eq!(
            row.columns.get(&column("c")),
            Some(&Value::Integer(i64::MAX))
        );
    }
}

// --- D1: expressions may project a column from a row an earlier read bound, and use it as an
// arithmetic operand (docs/concept.md §2.2) ---

mod kiln_ir_13 {
    use super::*;

    #[test]
    fn arithmetic_can_use_a_value_projected_from_an_earlier_read() {
        let key = Value::Integer(1);
        let store = Store::containing([(
            table("t"),
            key.clone(),
            Row {
                columns: support::columns_of_values([("balance", Value::Integer(50))]),
            },
        )]);

        let declared_column = Column {
            name: column("balance"),
            width: IntegerWidth::I32,
            domain: None,
        };
        let program = program_with_column(
            declared_column,
            vec![typed_parameter("amount", IntegerWidth::I32)],
            vec![
                Effect::Read {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                    into: kiln_ir::Slot(0),
                },
                Effect::Update {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                    columns: columns_of([(
                        "balance",
                        Expr::Arithmetic(
                            ArithmeticOp::Add,
                            Box::new(Expr::Projection {
                                slot: kiln_ir::Slot(0),
                                column: column("balance"),
                            }),
                            Box::new(Expr::Parameter(support::parameter("amount"))),
                        ),
                    )]),
                },
            ],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![Value::Integer(25)]), &store);

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        let row = execution
            .store
            .row(&table("t"), &key)
            .expect("row still exists after the update");
        assert_eq!(
            row.columns.get(&column("balance")),
            Some(&Value::Integer(75))
        );
    }
}
