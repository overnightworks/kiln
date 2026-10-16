//! Property tests driving `kiln_ir::run` — the crate's one public entry
//! point — against the reference meaning `docs/concept.md` §2.2 records.
//! Each test pins exactly one bullet of that meaning.
//!
//! D3's checked arithmetic, width, and clamping-domain behaviour lives in
//! `tests/arithmetic.rs` instead: this file covers everything else `run`
//! does — key reads, writes, branching, aborts, and the access log.

mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessLogEntry, AccessOrigin, AccessTarget, AccessedColumns,
    Condition, Effect, Expr, KeyInterval, Outcome, Row, Slot, Store, Terminal, Value,
};
use proptest::prelude::*;
use support::{
    arbitrary_key, arbitrary_value, call_under_test, column, columns_of, columns_of_values,
    committed_response, key_expr, literal_key, one_part, parameter, run_ok,
    single_function_program, table, typed_parameter,
};

// --- a projection has no single current row to read from when its source read found none; this
// is a typed abort, not a panic (finding 4) ---

mod kiln_ir_15 {
    use super::*;

    #[test]
    fn projecting_from_a_slot_whose_read_found_no_row_aborts_with_a_typed_reason() {
        let store = Store::empty();
        let program = single_function_program(
            vec![],
            vec![Effect::Read {
                table: table("t"),
                key: literal_key(1),
                into: Slot(0),
            }],
            Terminal::Return(Expr::Projection {
                slot: Slot(0),
                column: column("c"),
            }),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::ProjectionSourceAbsent { slot: Slot(0) },
            }
        );
        assert_eq!(&execution.store, &store);
    }

    #[test]
    fn projecting_a_column_absent_from_the_bound_row_aborts_with_a_typed_reason() {
        let store = Store::containing([(table("t"), one_part(1), Row::default())]);
        let program = single_function_program(
            vec![],
            vec![Effect::Read {
                table: table("t"),
                key: literal_key(1),
                into: Slot(0),
            }],
            Terminal::Return(Expr::Projection {
                slot: Slot(0),
                column: column("missing"),
            }),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::MissingColumn {
                    slot: Slot(0),
                    column: column("missing"),
                },
            }
        );
        assert_eq!(&execution.store, &store);
    }
}

// --- projecting a column the bound row does not carry is a typed abort, not a panic
// (finding 4) ---

// --- a call must supply exactly the arguments its function declares, referenced or not; a
// missing or surplus one aborts with a typed reason instead of panicking or being ignored ---

mod kiln_ir_06 {
    use super::*;

    #[test]
    fn a_call_whose_argument_count_differs_from_the_parameters_aborts() {
        let cases = [
            (
                vec![],
                AbortReason::MissingArgument {
                    parameter: parameter("amount"),
                },
            ),
            (
                vec![Value::Integer(1), Value::Integer(2)],
                AbortReason::UnexpectedArgument { position: 1 },
            ),
        ];
        for (arguments, reason) in cases {
            let store = Store::empty();
            // The body never references `amount`: arity is checked even for unused parameters.
            let program = single_function_program(
                vec![typed_parameter("amount", kiln_ir::IntegerWidth::I8)],
                vec![],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_ok(&program, &call_under_test(arguments), &store);

            assert_eq!(execution.outcome, Outcome::Aborted { reason });
            assert_eq!(&execution.store, &store);
        }
    }
}

// --- an absent key is a branchable value (docs/concept.md §2.2) ---

mod kiln_ir_08 {
    use super::*;

    proptest! {
        #[test]
        fn absent_key_is_branchable(
            key in arbitrary_key(),
            row_present in any::<bool>(),
        ) {
            let store = if row_present {
                Store::containing([(table("t"), key.clone(), Row::default())])
            } else {
                Store::empty()
            };

            let program = single_function_program(vec![],
                vec![Effect::Read { table: table("t"), key: key_expr(&key), into: Slot(0) }],
                Terminal::Branch {
                    condition: Condition::RowPresent(Slot(0)),
                    when_present: Box::new(kiln_ir::Block {
                        effects: vec![],
                        terminal: Terminal::Return(Expr::Literal(Value::Integer(1))),
                    }),
                    when_absent: Box::new(kiln_ir::Block {
                        effects: vec![],
                        terminal: Terminal::Return(Expr::Literal(Value::Integer(0))),
                    }),
                },
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            let expected = Value::Integer(if row_present { 1 } else { 0 });
            prop_assert_eq!(committed_response(&execution), &expected);
        }
    }
}

// --- update or delete of an absent row aborts (docs/concept.md §2.2) ---

#[derive(Debug, Clone, Copy)]
enum WriteKind {
    Update,
    Delete,
}

mod kiln_ir_17 {
    use super::*;

    proptest! {
        #[test]
        fn write_to_absent_row_aborts(
            key in arbitrary_key(),
            kind in prop_oneof![Just(WriteKind::Update), Just(WriteKind::Delete)],
        ) {
            let store = Store::empty();
            let effect = match kind {
                WriteKind::Update => Effect::Update {
                    table: table("t"),
                    key: key_expr(&key),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(0)))]),
                },
                WriteKind::Delete => Effect::Delete { table: table("t"), key: key_expr(&key) },
            };
            let program = single_function_program(vec![],
                vec![effect],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            match execution.outcome {
                Outcome::Aborted { reason: AbortReason::MissingRow { table: t, key: k } } => {
                    prop_assert_eq!(t, table("t"));
                    prop_assert_eq!(k, key);
                }
                other => prop_assert!(false, "expected a MissingRow abort, got {other:?}"),
            }
            prop_assert_eq!(&execution.store, &store);
        }
    }
}

// --- inserting an existing primary key aborts (docs/concept.md §2.2) ---

mod kiln_ir_18 {
    use super::*;

    proptest! {
        #[test]
        fn inserting_existing_key_aborts(
            key in arbitrary_key(),
            existing_value in arbitrary_value(),
            new_value in arbitrary_value(),
        ) {
            let store = Store::containing([(
                table("t"),
                key.clone(),
                Row { columns: columns_of_values([("c", existing_value)]) },
            )]);

            let program = single_function_program(vec![],
                vec![Effect::Insert {
                    table: table("t"),
                    key: key_expr(&key),
                    columns: columns_of([("c", Expr::Literal(new_value))]),
                }],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            match execution.outcome {
                Outcome::Aborted { reason: AbortReason::DuplicateKey { table: t, key: k } } => {
                    prop_assert_eq!(t, table("t"));
                    prop_assert_eq!(k, key);
                }
                other => prop_assert!(false, "expected a DuplicateKey abort, got {other:?}"),
            }
            prop_assert_eq!(&execution.store, &store);
        }
    }
}

// --- effects apply in program order and later reads see earlier writes ---

mod kiln_ir_07 {
    use super::*;

    proptest! {
        #[test]
        fn effects_apply_in_order_and_reads_see_earlier_writes(
            key in arbitrary_key(),
            first_write in any::<i64>(),
        ) {
            let second_write = first_write.wrapping_add(1); // always distinct from first_write
            let store = Store::empty();

            let program = single_function_program(vec![],
                vec![
                    Effect::Insert {
                        table: table("t"),
                        key: key_expr(&key),
                        columns: columns_of([("c", Expr::Literal(Value::Integer(first_write)))]),
                    },
                    // A read right after the insert, still inside this same, uncommitted call.
                    Effect::Read { table: table("t"), key: key_expr(&key), into: Slot(0) },
                    Effect::Update {
                        table: table("t"),
                        key: key_expr(&key),
                        columns: columns_of([("c", Expr::Literal(Value::Integer(second_write)))]),
                    },
                ],
                Terminal::Branch {
                    condition: Condition::RowPresent(Slot(0)),
                    when_present: Box::new(kiln_ir::Block {
                        effects: vec![],
                        terminal: Terminal::Return(Expr::Literal(Value::Integer(1))),
                    }),
                    when_absent: Box::new(kiln_ir::Block {
                        effects: vec![],
                        terminal: Terminal::Return(Expr::Literal(Value::Integer(0))),
                    }),
                },
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            // The read right after the insert saw it, before the call ever committed.
            prop_assert_eq!(committed_response(&execution), &Value::Integer(1));
            // The final state reflects the update that ran after it, not the insert it overwrote.
            let row = execution.store.row(&table("t"), &key).expect("row exists after commit");
            prop_assert_eq!(row.columns.get(&column("c")), Some(&Value::Integer(second_write)));
        }
    }
}

// --- an abort discards all effects (docs/concept.md §2.2) ---

mod kiln_ir_21 {
    use super::*;

    proptest! {
        #[test]
        fn abort_discards_all_effects(
            key in arbitrary_key(),
            value in any::<i64>(),
        ) {
            let store = Store::empty();

            let program = single_function_program(vec![],
                vec![Effect::Insert {
                    table: table("t"),
                    key: key_expr(&key),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(value)))]),
                }],
                Terminal::Abort(kiln_ir::AbortName("rejected".to_owned())),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            let aborted_by_declared_reason = matches!(
                execution.outcome,
                Outcome::Aborted { reason: AbortReason::Declared(_) }
            );
            prop_assert!(aborted_by_declared_reason);
            prop_assert_eq!(&execution.store, &store);
        }
    }
}

// --- the access log records every attempted access, including on abort ---

mod kiln_ir_34 {
    use super::*;

    proptest! {
        #[test]
        fn access_log_records_every_attempt_including_the_one_that_aborts(
            inserted_key in arbitrary_key(),
            missing_key in arbitrary_key(),
            value in any::<i64>(),
        ) {
            prop_assume!(inserted_key != missing_key);
            let store = Store::empty();

            let program = single_function_program(vec![],
                vec![
                    Effect::Insert {
                        table: table("t"),
                        key: key_expr(&inserted_key),
                        columns: columns_of([("c", Expr::Literal(Value::Integer(value)))]),
                    },
                    // This update targets a different, never-inserted key: it aborts the call,
                    // but it must still show up in the access log.
                    Effect::Update {
                        table: table("t"),
                        key: key_expr(&missing_key),
                        columns: columns_of([("c", Expr::Literal(Value::Integer(value)))]),
                    },
                ],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            let aborted_on_the_missing_row = matches!(
                execution.outcome,
                Outcome::Aborted { reason: AbortReason::MissingRow { .. } }
            );
            prop_assert!(aborted_on_the_missing_row);
            prop_assert_eq!(execution.access_log.len(), 2);
            prop_assert_eq!(execution.access_log[0].table.clone(), table("t"));
            prop_assert_eq!(execution.access_log[0].kind.clone(), AccessKind::Insert);
            prop_assert_eq!(
                execution.access_log[0].target.clone(),
                AccessTarget::Point(inserted_key)
            );
            prop_assert_eq!(
                execution.access_log[0].columns.clone(),
                AccessedColumns::Named(vec![column("c")])
            );
            prop_assert_eq!(execution.access_log[1].kind.clone(), AccessKind::Update);
            prop_assert_eq!(
                execution.access_log[1].target.clone(),
                AccessTarget::Point(missing_key)
            );
            // The whole call aborted, so the insert the log remembers never actually happened.
            prop_assert_eq!(&execution.store, &store);
        }
    }
}

// --- the access log entry is complete for every kind of access (docs/concept.md §2.2, D5) ---

mod kiln_ir_35 {
    use super::*;

    #[test]
    fn access_log_entry_is_complete_for_every_access_kind() {
        let int = |value| Expr::Literal(Value::Integer(value));
        let entry = |kind, target, columns| AccessLogEntry {
            table: table("t"),
            kind,
            target,
            columns,
            origin: AccessOrigin::TransactionLogic,
        };
        let point = |key| AccessTarget::Point(one_part(key));
        let cases = [
            (
                Effect::Read {
                    table: table("t"),
                    key: vec![int(1)],
                    into: Slot(0),
                },
                entry(AccessKind::PointRead, point(1), AccessedColumns::EntireRow),
            ),
            (
                Effect::ReadRange {
                    table: table("t"),
                    prefix: vec![],
                    interval: Some(KeyInterval {
                        from: int(0),
                        to: int(9),
                    }),
                    bound: int(9),
                    into: Slot(0),
                },
                entry(
                    AccessKind::RangeRead,
                    AccessTarget::Range {
                        prefix: vec![],
                        interval: Some((Value::Integer(0), Value::Integer(9))),
                    },
                    AccessedColumns::EntireRow,
                ),
            ),
            (
                Effect::Insert {
                    table: table("t"),
                    key: vec![int(2)],
                    columns: columns_of([("c", int(1))]),
                },
                entry(
                    AccessKind::Insert,
                    point(2),
                    AccessedColumns::Named(vec![column("c")]),
                ),
            ),
            (
                Effect::Update {
                    table: table("t"),
                    key: vec![int(1)],
                    columns: columns_of([("c", int(1))]),
                },
                entry(
                    AccessKind::Update,
                    point(1),
                    AccessedColumns::Named(vec![column("c")]),
                ),
            ),
            (
                Effect::Delete {
                    table: table("t"),
                    key: vec![int(1)],
                },
                entry(AccessKind::Delete, point(1), AccessedColumns::EntireRow),
            ),
        ];
        for (effect, expected) in cases {
            let store = Store::containing([(
                table("t"),
                one_part(1),
                Row {
                    columns: columns_of_values([("c", Value::Integer(0))]),
                },
            )]);
            let program = single_function_program(vec![], vec![effect], Terminal::Return(int(0)));

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            assert_eq!(execution.access_log, vec![expected]);
        }
    }
}

// --- a successful delete removes the row (docs/concept.md §2.2, D6) ---

mod kiln_ir_19 {
    use super::*;

    #[test]
    fn successful_delete_removes_the_row() {
        let key = one_part(1);
        let store = Store::containing([(
            table("t"),
            key.clone(),
            Row {
                columns: columns_of_values([("c", Value::Integer(5))]),
            },
        )]);

        let program = single_function_program(
            vec![],
            vec![Effect::Delete {
                table: table("t"),
                key: key_expr(&key),
            }],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));
        assert_eq!(execution.store.row(&table("t"), &key), None);
    }
}

// --- an update overwrites only its named columns (docs/concept.md §2.2) ---

mod kiln_ir_20 {
    use super::*;

    #[test]
    fn update_preserves_untouched_columns() {
        let key = one_part(1);
        let store = Store::containing([(
            table("t"),
            key.clone(),
            Row {
                columns: columns_of_values([("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            },
        )]);

        let program = single_function_program(
            vec![],
            vec![Effect::Update {
                table: table("t"),
                key: key_expr(&key),
                columns: columns_of([("a", Expr::Literal(Value::Integer(99)))]),
            }],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        let row = execution
            .store
            .row(&table("t"), &key)
            .expect("row still exists after the update");
        assert_eq!(row.columns.get(&column("a")), Some(&Value::Integer(99)));
        assert_eq!(row.columns.get(&column("b")), Some(&Value::Integer(2)));
    }
}
