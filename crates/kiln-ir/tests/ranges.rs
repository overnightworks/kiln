//! Range reads, bounded iteration, dependent reads, and typed function
//! parameters (`docs/concept.md` §2.2, D5, D6) — slice 3 of the reference
//! interpreter. `Store::rows_in_range`'s own key-order and empty-range
//! invariants live next to it in `src/store.rs`; this file covers what
//! `kiln_ir::run` does with those rows: branchable absence, a bound it
//! never exceeds, the access log entry a range read leaves, dependent
//! reads, and the argument-width check every other behaviour of `run`
//! lives in `tests/evaluator.rs` and `tests/arithmetic.rs` instead.

mod support;

use std::collections::BTreeMap;

use kiln_ir::{
    AbortReason, AccessKind, AccessTarget, AccessedColumns, Condition, Effect, Execution, Expr,
    IntegerWidth, Key, Outcome, OverflowReason, Row, Slot, Store, TableName, Terminal, Value,
};
use proptest::prelude::*;
use support::{
    call_under_test, column, columns_of_values, committed_response, literal_key, one_part,
    parameter, run_ok, single_function_program, table, text_parameter, typed_parameter,
};

fn branch_on_slot_present(slot: Slot) -> Terminal {
    Terminal::Branch {
        condition: Condition::RowPresent(slot),
        when_present: Box::new(kiln_ir::Block {
            effects: vec![],
            terminal: Terminal::Return(Expr::Literal(Value::Integer(1))),
        }),
        when_absent: Box::new(kiln_ir::Block {
            effects: vec![],
            terminal: Terminal::Return(Expr::Literal(Value::Integer(0))),
        }),
    }
}

/// The keys of `execution`'s logged `kind` accesses on `table_name`, in log order.
fn logged_keys(execution: &Execution, table_name: &str, kind: AccessKind) -> Vec<Key> {
    execution
        .access_log
        .iter()
        .filter(|entry| entry.table == table(table_name) && entry.kind == kind)
        .map(|entry| match &entry.target {
            AccessTarget::Point(key) => key.clone(),
            other => panic!("expected a point target, got {other:?}"),
        })
        .collect()
}

fn range_read(table_name: &str, start: i64, end: i64, bound: i64, into: Slot) -> Effect {
    Effect::ReadRange {
        table: table(table_name),
        start: literal_key(start),
        end: literal_key(end),
        bound: Expr::Literal(Value::Integer(bound)),
        into,
    }
}

/// A store whose table "t" holds one row per id, keyed by it.
fn rows_of_t(ids: &[i64]) -> Store {
    Store::containing(t_rows(ids))
}

fn t_rows(ids: &[i64]) -> impl Iterator<Item = (TableName, Key, Row)> + '_ {
    ids.iter().map(|id| {
        (
            table("t"),
            one_part(*id),
            Row {
                columns: BTreeMap::new(),
            },
        )
    })
}

/// Reads `[start, end]` of table "t", then iterates over it inserting each row's id into
/// "visited", which already holds the `preexisting` keys (so inserting one of them aborts).
fn visit_range(rows: &[i64], preexisting: &[i64], start: i64, end: i64) -> Execution {
    let store = Store::containing(
        t_rows(rows).chain(
            preexisting
                .iter()
                .map(|key| (table("visited"), one_part(*key), Row::default())),
        ),
    );
    let program = single_function_program(
        vec![],
        vec![
            range_read("t", start, end, 10, Slot(0)),
            Effect::Iterate {
                source: Slot(0),
                row: Slot(1),
                body: vec![Effect::Insert {
                    table: table("visited"),
                    key: vec![Expr::Projection {
                        slot: Slot(1),
                        column: column("id"),
                    }],
                    columns: BTreeMap::new(),
                }],
            },
        ],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );
    run_ok(&program, &call_under_test(vec![]), &store)
}

/// Iterates all of "t" (row slot 1), with a point read of the row itself bound to slot 2 inside
/// the body, then ends in `terminal`.
fn looped_program(terminal: Terminal) -> kiln_ir::Program {
    single_function_program(
        vec![],
        vec![
            range_read("t", 0, 10, 10, Slot(0)),
            Effect::Iterate {
                source: Slot(0),
                row: Slot(1),
                body: vec![Effect::Read {
                    table: table("t"),
                    key: vec![Expr::Projection {
                        slot: Slot(1),
                        column: column("id"),
                    }],
                    into: Slot(2),
                }],
            },
        ],
        terminal,
    )
}

fn one_parts(keys: &[i64]) -> Vec<Key> {
    keys.iter().copied().map(one_part).collect()
}

// --- an empty range reads as an empty, branchable sequence, the same as an absent point read
// (docs/concept.md §2.2, D6) ---

mod kiln_ir_11 {
    use super::*;

    proptest! {
        #[test]
        fn range_read_absence_is_branchable_like_a_point_reads(
            key in any::<i64>(),
            row_present in any::<bool>(),
        ) {
            let store = if row_present {
                Store::containing([(table("t"), one_part(key), Row::default())])
            } else {
                Store::empty()
            };

            let program = single_function_program(
                vec![],
                vec![Effect::ReadRange {
                    table: table("t"),
                    start: literal_key(i64::MIN),
                    end: literal_key(i64::MAX),
                    bound: Expr::Literal(Value::Integer(1)),
                    into: Slot(0),
                }],
                branch_on_slot_present(Slot(0)),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            let expected = Value::Integer(if row_present { 1 } else { 0 });
            prop_assert_eq!(committed_response(&execution), &expected);
        }
    }
}

// --- bounded iteration never runs open-ended: a range read matching more rows than its
// declared bound aborts and discards every effect of the call, not only the range read itself
// (docs/concept.md §2.1, §2.2 D6) ---

mod kiln_ir_12 {
    use super::*;

    proptest! {
        #[test]
        fn range_read_exceeding_its_declared_bound_aborts_and_discards_every_effect(
            matched_count in 2i64..8,
        ) {
            let store = Store::containing(
                (0..matched_count).map(|key| (table("matched"), one_part(key), Row::default())),
            );
            let bound = matched_count - 1; // one less than what the range actually matches

            let program = single_function_program(
                vec![],
                vec![
                    Effect::Insert {
                        table: table("other"),
                        key: literal_key(0),
                        columns: BTreeMap::new(),
                    },
                    Effect::ReadRange {
                        table: table("matched"),
                        start: literal_key(0),
                        end: literal_key(matched_count - 1),
                        bound: Expr::Literal(Value::Integer(bound)),
                        into: Slot(0),
                    },
                ],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            prop_assert_eq!(
                execution.outcome,
                Outcome::Aborted {
                    reason: AbortReason::BoundExceeded {
                        table: table("matched"),
                        bound,
                        matched: matched_count as usize,
                    },
                }
            );
            // The insert into "other" ran locally but is discarded along with the range read that
            // then exceeded its bound.
            prop_assert_eq!(&execution.store, &store);
        }
    }

    #[test]
    fn a_negative_bound_aborts_even_when_the_range_matches_no_row() {
        let program = single_function_program(
            vec![],
            vec![range_read("t", 0, 10, -1, Slot(0))],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &Store::empty());

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::BoundExceeded {
                    table: table("t"),
                    bound: -1,
                    matched: 0
                },
            }
        );
    }
}

// --- the access log records a range read's bounds, including when the range matches no row
// (docs/concept.md §2.2, D5) ---

mod kiln_ir_36 {
    use super::*;

    #[test]
    fn access_log_records_a_range_reads_bounds_including_when_the_range_is_empty() {
        for store in [
            Store::containing([(table("t"), one_part(5), Row::default())]),
            Store::empty(),
        ] {
            let program = single_function_program(
                vec![],
                vec![Effect::ReadRange {
                    table: table("t"),
                    start: literal_key(0),
                    end: literal_key(10),
                    bound: Expr::Literal(Value::Integer(10)),
                    into: Slot(0),
                }],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );

            let execution = run_ok(&program, &call_under_test(vec![]), &store);

            assert_eq!(execution.access_log.len(), 1);
            let entry = &execution.access_log[0];
            assert_eq!(entry.table, table("t"));
            assert_eq!(entry.kind, AccessKind::RangeRead);
            assert_eq!(
                entry.target,
                AccessTarget::Range {
                    start: one_part(0),
                    end: one_part(10),
                }
            );
            assert_eq!(entry.columns, AccessedColumns::EntireRow);
        }
    }
}

// --- a dependent read's key is a value projected from an earlier read in the same call, and its
// own absence is branchable exactly like any other read's (docs/concept.md §2.1, §2.2 D6) ---

mod kiln_ir_13 {
    use super::*;

    proptest! {
        #[test]
        fn dependent_read_follows_a_key_projected_from_an_earlier_read(
            source_key in any::<i64>(),
            linked_key in any::<i64>(),
            linked_row_present in any::<bool>(),
        ) {
            let mut rows = vec![(
                table("source"),
                one_part(source_key),
                Row { columns: columns_of_values([("linked_key", Value::Integer(linked_key))]) },
            )];
            if linked_row_present {
                rows.push((table("linked"), one_part(linked_key), Row::default()));
            }
            let store = Store::containing(rows);

            let program = single_function_program(
                vec![],
                vec![
                    Effect::Read {
                        table: table("source"),
                        key: literal_key(source_key),
                        into: Slot(0),
                    },
                    Effect::Read {
                        table: table("linked"),
                        key: vec![Expr::Projection { slot: Slot(0), column: column("linked_key") }],
                        into: Slot(1),
                    },
                ],
                Terminal::Branch {
                    condition: Condition::RowPresent(Slot(1)),
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

            let expected = Value::Integer(if linked_row_present { 1 } else { 0 });
            prop_assert_eq!(committed_response(&execution), &expected);
        }
    }
}

// --- function parameters carry a declared type and width, and an argument is checked against
// them exactly as a write is checked against its target column's declared width
// (docs/concept.md §2.2, D3) ---

mod kiln_ir_04 {
    use super::*;

    #[test]
    fn argument_not_fitting_its_parameter_aborts_with_a_typed_reason() {
        let cases = [
            (
                typed_parameter("amount", IntegerWidth::I8),
                Value::Integer(200),
                AbortReason::Overflow(OverflowReason::Argument {
                    parameter: parameter("amount"),
                }),
            ),
            (
                typed_parameter("amount", IntegerWidth::I8),
                Value::Text("x".to_owned()),
                AbortReason::ArgumentTypeMismatch {
                    parameter: parameter("amount"),
                },
            ),
            (
                text_parameter("name"),
                Value::Integer(1),
                AbortReason::ArgumentTypeMismatch {
                    parameter: parameter("name"),
                },
            ),
        ];
        for (declared, argument, reason) in cases {
            let program = single_function_program(
                vec![declared],
                vec![],
                Terminal::Return(Expr::Literal(Value::Integer(0))),
            );
            let store = Store::empty();

            let execution = run_ok(&program, &call_under_test(vec![argument]), &store);

            assert_eq!(execution.outcome, Outcome::Aborted { reason });
            assert!(execution.access_log.is_empty());
            assert_eq!(&execution.store, &store);
        }
    }
}

mod kiln_ir_05 {
    use super::*;

    #[test]
    fn argument_fitting_its_parameter_commits() {
        let cases = [
            (
                typed_parameter("amount", IntegerWidth::I8),
                Value::Integer(100),
            ),
            (text_parameter("name"), Value::Text("alice".to_owned())),
        ];
        for (declared, argument) in cases {
            let name = declared.name.clone();
            let program = single_function_program(
                vec![declared],
                vec![],
                Terminal::Return(Expr::Parameter(name)),
            );

            let execution = run_ok(
                &program,
                &call_under_test(vec![argument.clone()]),
                &Store::empty(),
            );

            assert_eq!(committed_response(&execution), &argument);
        }
    }
}

// --- a range read's bound expression must resolve to an integer — it counts rows — and a
// non-integer bound aborts with a typed reason instead of panicking (finding 3) ---

mod kiln_ir_16 {
    use super::*;

    #[test]
    fn range_read_bound_resolving_to_a_non_integer_value_aborts_with_a_typed_reason() {
        let program = single_function_program(
            vec![],
            vec![Effect::ReadRange {
                table: table("t"),
                start: literal_key(0),
                end: literal_key(10),
                bound: Expr::Literal(Value::Text("not a number".to_owned())),
                into: Slot(0),
            }],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );
        let store = Store::empty();

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::NonIntegerBound { table: table("t") },
            }
        );
        assert_eq!(&execution.store, &store);
    }
}

// --- projecting a column from a slot a range read bound has no single current row to read from,
// outside any iteration over it; this is a typed abort, not a panic (finding 1) ---

mod kiln_ir_15 {
    use super::*;

    #[test]
    fn projecting_from_a_range_bound_slot_outside_iteration_aborts_with_a_typed_reason() {
        let store = Store::containing([(table("t"), one_part(5), Row::default())]);
        let program = single_function_program(
            vec![],
            vec![Effect::ReadRange {
                table: table("t"),
                start: literal_key(0),
                end: literal_key(10),
                bound: Expr::Literal(Value::Integer(10)),
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
    fn the_iteration_row_slot_projects_as_absent_after_the_loop() {
        for rows in [vec![], vec![1, 2]] {
            let program = looped_program(Terminal::Return(Expr::Projection {
                slot: Slot(1),
                column: column("id"),
            }));
            let execution = run_ok(&program, &call_under_test(vec![]), &rows_of_t(&rows));

            assert_eq!(
                execution.outcome,
                Outcome::Aborted {
                    reason: AbortReason::ProjectionSourceAbsent { slot: Slot(1) }
                }
            );
        }
    }

    #[test]
    fn a_slot_bound_inside_the_body_branches_as_absent_after_the_loop() {
        for rows in [vec![], vec![1, 2]] {
            let program = looped_program(branch_on_slot_present(Slot(2)));
            let execution = run_ok(&program, &call_under_test(vec![]), &rows_of_t(&rows));

            assert_eq!(committed_response(&execution), &Value::Integer(0));
        }
    }
}

// --- bounded iteration runs its body once per row a range read matched, in key order, with the
// current row bound to a slot so the body's own dependent reads and writes can use it
// (docs/concept.md §2.1 "bounded iteration", §2.2 D6: "bounded iteration proceeds in key order")
// ---

mod kiln_ir_14 {
    use super::*;

    #[test]
    fn bounded_iteration_runs_its_body_once_per_row_in_key_order_with_dependent_reads_and_writes() {
        let orders = [(1, 10), (2, 20), (3, 30)]; // (order id, customer id)
        let store = Store::containing(
            orders
                .into_iter()
                .map(|(id, customer)| {
                    (
                        table("orders"),
                        one_part(id),
                        Row {
                            columns: columns_of_values([("customer", Value::Integer(customer))]),
                        },
                    )
                })
                .chain(
                    orders.into_iter().map(|(_, customer)| {
                        (table("customers"), one_part(customer), Row::default())
                    }),
                ),
        );

        let program = single_function_program(
            vec![],
            vec![
                Effect::ReadRange {
                    table: table("orders"),
                    start: literal_key(1),
                    end: literal_key(3),
                    bound: Expr::Literal(Value::Integer(10)),
                    into: Slot(0),
                },
                Effect::Iterate {
                    source: Slot(0),
                    row: Slot(1),
                    body: vec![
                        Effect::Read {
                            table: table("customers"),
                            key: vec![Expr::Projection {
                                slot: Slot(1),
                                column: column("customer"),
                            }],
                            into: Slot(2),
                        },
                        Effect::Insert {
                            table: table("visited"),
                            key: vec![Expr::Projection {
                                slot: Slot(1),
                                column: column("id"),
                            }],
                            columns: BTreeMap::new(),
                        },
                    ],
                },
            ],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert!(matches!(execution.outcome, Outcome::Committed { .. }));

        // Every matched order was visited exactly once: a "visited" row now exists for each.
        for (id, _) in orders {
            assert!(
                execution
                    .store
                    .row(&table("visited"), &one_part(id))
                    .is_some(),
                "order {id} was not visited"
            );
        }

        // The dependent read on "customers" and the insert into "visited" each ran once per row, in
        // key order — the same order `rows_in_range` returns its matches in.
        assert_eq!(
            logged_keys(&execution, "customers", AccessKind::PointRead),
            one_parts(&[10, 20, 30])
        );
        assert_eq!(
            logged_keys(&execution, "visited", AccessKind::Insert),
            one_parts(&[1, 2, 3])
        );
    }

    #[test]
    fn an_abort_inside_the_body_discards_earlier_iterations_effects() {
        let execution = visit_range(&[1, 2, 3], &[2], 1, 3);

        assert_eq!(
            execution.outcome,
            Outcome::Aborted {
                reason: AbortReason::DuplicateKey {
                    table: table("visited"),
                    key: one_part(2)
                },
            }
        );
        assert!(execution
            .store
            .row(&table("visited"), &one_part(1))
            .is_none());
    }
}

// --- run-level range reads: single-row and empty ranges iterate as exactly what they hold ---

mod kiln_ir_09 {
    use super::*;

    #[test]
    fn a_range_of_one_row_iterates_that_row_and_bounds_are_inclusive() {
        let rows = [1, 2, 3, 4];
        for (start, end, expected) in [
            (2, 2, vec![2]),
            (2, 3, vec![2, 3]),
            (1, 4, vec![1, 2, 3, 4]),
        ] {
            let execution = visit_range(&rows, &[], start, end);

            assert_eq!(
                logged_keys(&execution, "visited", AccessKind::Insert),
                one_parts(&expected)
            );
        }
    }
}

mod kiln_ir_10 {
    use super::*;

    #[test]
    fn an_empty_or_reversed_range_iterates_nothing_and_commits() {
        for (rows, start, end) in [(vec![], 0, 10), (vec![1, 2], 5, 9), (vec![1, 2], 2, 1)] {
            let execution = visit_range(&rows, &[], start, end);

            assert!(matches!(execution.outcome, Outcome::Committed { .. }));
            assert!(logged_keys(&execution, "visited", AccessKind::Insert).is_empty());
        }
    }
}

// --- a bound that cannot be met aborts: a negative one even when no row matches ---

// --- an abort inside an iteration body discards the earlier iterations' effects ---

// --- nothing an iteration bound outlives it: its row slot and body-bound slots read as absent
// afterwards, whether the range held rows or not ---
