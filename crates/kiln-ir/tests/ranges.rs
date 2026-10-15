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
    single_function_program, table, typed_parameter, untyped_parameter,
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
fn a_parameter_with_no_declared_width_never_aborts_on_its_argument() {
    let program = single_function_program(
        vec![untyped_parameter("amount")],
        vec![],
        Terminal::Return(Expr::Parameter(parameter("amount"))),
    );
    let store = Store::empty();

    let execution = run_ok(
        &program,
        &call_under_test(vec![Value::Integer(i64::MAX)]),
        &store,
    );

    assert_eq!(committed_response(&execution), &Value::Integer(i64::MAX));
}
