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
    AbortReason, AccessKind, AccessTarget, AccessedColumns, Condition, Effect, Expr, IntegerWidth,
    Outcome, OverflowReason, Row, Slot, Store, Terminal, Value,
};
use proptest::prelude::*;
use support::{
    call_under_test, column, columns_of_values, committed_response, parameter, run_ok,
    single_function_program, table, text_parameter, typed_parameter,
};

fn branch_on_slot_zero_present() -> Terminal {
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
    }
}

// --- an empty range reads as an empty, branchable sequence, the same as an absent point read
// (docs/concept.md §2.2, D6) ---

proptest! {
    #[test]
    fn range_read_absence_is_branchable_like_a_point_reads(
        key in any::<i64>(),
        row_present in any::<bool>(),
    ) {
        let store = if row_present {
            Store::containing([(table("t"), Value::Integer(key), Row::default())])
        } else {
            Store::empty()
        };

        let program = single_function_program(
            vec![],
            vec![Effect::ReadRange {
                table: table("t"),
                start: Expr::Literal(Value::Integer(i64::MIN)),
                end: Expr::Literal(Value::Integer(i64::MAX)),
                bound: Expr::Literal(Value::Integer(1)),
                into: Slot(0),
            }],
            branch_on_slot_zero_present(),
        );

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        let expected = Value::Integer(if row_present { 1 } else { 0 });
        prop_assert_eq!(committed_response(&execution), &expected);
    }
}

// --- bounded iteration never runs open-ended: a range read matching more rows than its
// declared bound aborts and discards every effect of the call, not only the range read itself
// (docs/concept.md §2.1, §2.2 D6) ---

proptest! {
    #[test]
    fn range_read_exceeding_its_declared_bound_aborts_and_discards_every_effect(
        matched_count in 2i64..8,
    ) {
        let store = Store::containing(
            (0..matched_count).map(|key| (table("matched"), Value::Integer(key), Row::default())),
        );
        let bound = matched_count - 1; // one less than what the range actually matches

        let program = single_function_program(
            vec![],
            vec![
                Effect::Insert {
                    table: table("other"),
                    key: Expr::Literal(Value::Integer(0)),
                    columns: BTreeMap::new(),
                },
                Effect::ReadRange {
                    table: table("matched"),
                    start: Expr::Literal(Value::Integer(0)),
                    end: Expr::Literal(Value::Integer(matched_count - 1)),
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

// --- the access log records a range read's bounds, including when the range matches no row
// (docs/concept.md §2.2, D5) ---

#[test]
fn access_log_records_a_range_reads_bounds_including_when_the_range_is_empty() {
    for store in [
        Store::containing([(table("t"), Value::Integer(5), Row::default())]),
        Store::empty(),
    ] {
        let program = single_function_program(
            vec![],
            vec![Effect::ReadRange {
                table: table("t"),
                start: Expr::Literal(Value::Integer(0)),
                end: Expr::Literal(Value::Integer(10)),
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
                start: Value::Integer(0),
                end: Value::Integer(10),
            }
        );
        assert_eq!(entry.columns, AccessedColumns::EntireRow);
    }
}

// --- a dependent read's key is a value projected from an earlier read in the same call, and its
// own absence is branchable exactly like any other read's (docs/concept.md §2.1, §2.2 D6) ---

proptest! {
    #[test]
    fn dependent_read_follows_a_key_projected_from_an_earlier_read(
        source_key in any::<i64>(),
        linked_key in any::<i64>(),
        linked_row_present in any::<bool>(),
    ) {
        let mut rows = vec![(
            table("source"),
            Value::Integer(source_key),
            Row { columns: columns_of_values([("linked_key", Value::Integer(linked_key))]) },
        )];
        if linked_row_present {
            rows.push((table("linked"), Value::Integer(linked_key), Row::default()));
        }
        let store = Store::containing(rows);

        let program = single_function_program(
            vec![],
            vec![
                Effect::Read {
                    table: table("source"),
                    key: Expr::Literal(Value::Integer(source_key)),
                    into: Slot(0),
                },
                Effect::Read {
                    table: table("linked"),
                    key: Expr::Projection { slot: Slot(0), column: column("linked_key") },
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

// --- function parameters carry a declared width, and an argument is checked against it exactly
// as a write is checked against its target column's declared width (docs/concept.md §2.2, D3)
// ---

#[test]
fn argument_outside_its_parameters_declared_width_aborts_with_a_typed_reason() {
    let program = single_function_program(
        vec![typed_parameter("amount", IntegerWidth::I8)],
        vec![],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );
    let store = Store::empty();

    let execution = run_ok(
        &program,
        &call_under_test(vec![Value::Integer(200)]),
        &store,
    );

    assert_eq!(
        execution.outcome,
        Outcome::Aborted {
            reason: AbortReason::Overflow(OverflowReason::Argument {
                parameter: parameter("amount"),
            }),
        }
    );
    assert!(execution.access_log.is_empty());
    assert_eq!(&execution.store, &store);
}

#[test]
fn argument_inside_its_parameters_declared_width_commits() {
    let program = single_function_program(
        vec![typed_parameter("amount", IntegerWidth::I8)],
        vec![],
        Terminal::Return(Expr::Parameter(parameter("amount"))),
    );
    let store = Store::empty();

    let execution = run_ok(
        &program,
        &call_under_test(vec![Value::Integer(100)]),
        &store,
    );

    assert_eq!(committed_response(&execution), &Value::Integer(100));
}

#[test]
fn a_text_argument_matching_its_parameters_declared_type_commits() {
    let program = single_function_program(
        vec![text_parameter("name")],
        vec![],
        Terminal::Return(Expr::Parameter(parameter("name"))),
    );
    let store = Store::empty();

    let execution = run_ok(
        &program,
        &call_under_test(vec![Value::Text("alice".to_owned())]),
        &store,
    );

    assert_eq!(
        committed_response(&execution),
        &Value::Text("alice".to_owned())
    );
}

// --- every parameter declares a mandatory type (Integer(width) or Text); an argument of the
// other kind aborts with a typed mismatch instead of resolving unchecked or panicking
// (docs/concept.md §2.2, D3) ---

#[test]
fn a_text_argument_against_an_integer_parameter_aborts_with_a_typed_mismatch() {
    let program = single_function_program(
        vec![typed_parameter("amount", IntegerWidth::I8)],
        vec![],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );
    let store = Store::empty();

    let execution = run_ok(
        &program,
        &call_under_test(vec![Value::Text("x".to_owned())]),
        &store,
    );

    assert_eq!(
        execution.outcome,
        Outcome::Aborted {
            reason: AbortReason::ArgumentTypeMismatch {
                parameter: parameter("amount"),
            },
        }
    );
    assert!(execution.access_log.is_empty());
    assert_eq!(&execution.store, &store);
}

#[test]
fn an_integer_argument_against_a_text_parameter_aborts_with_a_typed_mismatch() {
    let program = single_function_program(
        vec![text_parameter("name")],
        vec![],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );
    let store = Store::empty();

    let execution = run_ok(&program, &call_under_test(vec![Value::Integer(1)]), &store);

    assert_eq!(
        execution.outcome,
        Outcome::Aborted {
            reason: AbortReason::ArgumentTypeMismatch {
                parameter: parameter("name"),
            },
        }
    );
}

// --- a range read's bound expression must resolve to an integer — it counts rows — and a
// non-integer bound aborts with a typed reason instead of panicking (finding 3) ---

#[test]
fn range_read_bound_resolving_to_a_non_integer_value_aborts_with_a_typed_reason() {
    let program = single_function_program(
        vec![],
        vec![Effect::ReadRange {
            table: table("t"),
            start: Expr::Literal(Value::Integer(0)),
            end: Expr::Literal(Value::Integer(10)),
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

// --- projecting a column from a slot a range read bound has no single current row to read from,
// outside any iteration over it; this is a typed abort, not a panic (finding 1) ---

#[test]
fn projecting_from_a_range_bound_slot_outside_iteration_aborts_with_a_typed_reason() {
    let store = Store::containing([(table("t"), Value::Integer(5), Row::default())]);
    let program = single_function_program(
        vec![],
        vec![Effect::ReadRange {
            table: table("t"),
            start: Expr::Literal(Value::Integer(0)),
            end: Expr::Literal(Value::Integer(10)),
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

// --- bounded iteration runs its body once per row a range read matched, in key order, with the
// current row bound to a slot so the body's own dependent reads and writes can use it
// (docs/concept.md §2.1 "bounded iteration", §2.2 D6: "bounded iteration proceeds in key order")
// ---

#[test]
fn bounded_iteration_runs_its_body_once_per_row_in_key_order_with_dependent_reads_and_writes() {
    let orders = [(1, 10), (2, 20), (3, 30)]; // (order id, customer id)
    let store = Store::containing(
        orders
            .into_iter()
            .map(|(id, customer)| {
                (
                    table("orders"),
                    Value::Integer(id),
                    Row {
                        columns: columns_of_values([
                            ("id", Value::Integer(id)),
                            ("customer", Value::Integer(customer)),
                        ]),
                    },
                )
            })
            .chain(orders.into_iter().map(|(_, customer)| {
                (table("customers"), Value::Integer(customer), Row::default())
            })),
    );

    let program = single_function_program(
        vec![],
        vec![
            Effect::ReadRange {
                table: table("orders"),
                start: Expr::Literal(Value::Integer(1)),
                end: Expr::Literal(Value::Integer(3)),
                bound: Expr::Literal(Value::Integer(10)),
                into: Slot(0),
            },
            Effect::Iterate {
                source: Slot(0),
                row: Slot(1),
                body: vec![
                    Effect::Read {
                        table: table("customers"),
                        key: Expr::Projection {
                            slot: Slot(1),
                            column: column("customer"),
                        },
                        into: Slot(2),
                    },
                    Effect::Insert {
                        table: table("visited"),
                        key: Expr::Projection {
                            slot: Slot(1),
                            column: column("id"),
                        },
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
                .row(&table("visited"), &Value::Integer(id))
                .is_some(),
            "order {id} was not visited"
        );
    }

    // The dependent read on "customers" and the insert into "visited" each ran once per row, in
    // key order — the same order `rows_in_range` returns its matches in.
    let point_reads_on_customers: Vec<Value> = execution
        .access_log
        .iter()
        .filter(|entry| entry.table == table("customers") && entry.kind == AccessKind::PointRead)
        .map(|entry| match &entry.target {
            AccessTarget::Point(key) => key.clone(),
            other => panic!("expected a point read target, got {other:?}"),
        })
        .collect();
    assert_eq!(
        point_reads_on_customers,
        vec![Value::Integer(10), Value::Integer(20), Value::Integer(30)]
    );

    let visited_inserts: Vec<Value> = execution
        .access_log
        .iter()
        .filter(|entry| entry.table == table("visited") && entry.kind == AccessKind::Insert)
        .map(|entry| match &entry.target {
            AccessTarget::Point(key) => key.clone(),
            other => panic!("expected a point read target, got {other:?}"),
        })
        .collect();
    assert_eq!(
        visited_inserts,
        vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)]
    );
}
