//! Property tests driving `kiln_ir::run` — the crate's one public entry
//! point — against the reference meaning `docs/concept.md` §2.2 records.
//! Each test pins exactly one bullet of that meaning.

use std::collections::BTreeMap;

use kiln_ir::{
    run, AbortReason, AccessKind, AccessedColumns, Call, ColumnName, Condition, Effect, Expr,
    Function, FunctionName, Outcome, ParameterName, Program, Row, Slot, Store, TableName, Terminal,
    Value,
};
use proptest::prelude::*;

// --- small builders, so every test states its behaviour and not its AST plumbing ---

fn table(name: &str) -> TableName {
    TableName(name.to_owned())
}

fn column(name: &str) -> ColumnName {
    ColumnName(name.to_owned())
}

fn parameter(name: &str) -> ParameterName {
    ParameterName(name.to_owned())
}

fn columns_of(pairs: impl IntoIterator<Item = (&'static str, Expr)>) -> BTreeMap<ColumnName, Expr> {
    pairs
        .into_iter()
        .map(|(name, expr)| (column(name), expr))
        .collect()
}

fn single_function_program(
    parameters: Vec<ParameterName>,
    effects: Vec<Effect>,
    terminal: Terminal,
) -> Program {
    Program {
        schema: Default::default(),
        functions: vec![Function {
            name: FunctionName("under_test".to_owned()),
            parameters,
            body: kiln_ir::Block { effects, terminal },
        }],
    }
}

fn call_under_test(args: Vec<Value>) -> Call {
    Call {
        function: FunctionName("under_test".to_owned()),
        args,
    }
}

fn arbitrary_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(Value::Integer),
        "[a-z]{1,6}".prop_map(Value::Text),
    ]
}

fn committed_response(execution: &kiln_ir::Execution) -> &Value {
    match &execution.outcome {
        Outcome::Committed { response } => response,
        Outcome::Aborted { reason } => panic!("expected a commit, got an abort: {reason:?}"),
    }
}

// --- an absent key is a branchable value (docs/concept.md §2.2) ---

proptest! {
    #[test]
    fn absent_key_is_branchable(
        key in arbitrary_value(),
        row_present in any::<bool>(),
    ) {
        let store = if row_present {
            Store::containing([(table("t"), key.clone(), Row::default())])
        } else {
            Store::empty()
        };

        let program = single_function_program(
            vec![],
            vec![Effect::Read { table: table("t"), key: Expr::Literal(key), into: Slot(0) }],
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

        let execution = run(&program, &call_under_test(vec![]), &store);

        let expected = Value::Integer(if row_present { 1 } else { 0 });
        prop_assert_eq!(committed_response(&execution), &expected);
    }
}

// --- update or delete of an absent row aborts (docs/concept.md §2.2) ---

#[derive(Debug, Clone, Copy)]
enum WriteKind {
    Update,
    Delete,
}

proptest! {
    #[test]
    fn write_to_absent_row_aborts(
        key in arbitrary_value(),
        kind in prop_oneof![Just(WriteKind::Update), Just(WriteKind::Delete)],
    ) {
        let store = Store::empty();
        let effect = match kind {
            WriteKind::Update => Effect::Update {
                table: table("t"),
                key: Expr::Literal(key.clone()),
                columns: columns_of([("c", Expr::Literal(Value::Integer(0)))]),
            },
            WriteKind::Delete => Effect::Delete { table: table("t"), key: Expr::Literal(key.clone()) },
        };
        let program = single_function_program(
            vec![],
            vec![effect],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run(&program, &call_under_test(vec![]), &store);

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

// --- inserting an existing primary key aborts (docs/concept.md §2.2) ---

proptest! {
    #[test]
    fn inserting_existing_key_aborts(
        key in arbitrary_value(),
        existing_value in arbitrary_value(),
        new_value in arbitrary_value(),
    ) {
        let store = Store::containing([(
            table("t"),
            key.clone(),
            Row { columns: columns_of_values([("c", existing_value)]) },
        )]);

        let program = single_function_program(
            vec![parameter("arg")],
            vec![Effect::Insert {
                table: table("t"),
                key: Expr::Literal(key.clone()),
                columns: columns_of([("c", Expr::Parameter(parameter("arg")))]),
            }],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run(&program, &call_under_test(vec![new_value]), &store);

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

fn columns_of_values(
    pairs: impl IntoIterator<Item = (&'static str, Value)>,
) -> BTreeMap<ColumnName, Value> {
    pairs
        .into_iter()
        .map(|(name, value)| (column(name), value))
        .collect()
}

// --- effects apply in program order and later reads see earlier writes ---

proptest! {
    #[test]
    fn effects_apply_in_order_and_reads_see_earlier_writes(
        key in arbitrary_value(),
        first_write in any::<i64>(),
    ) {
        let second_write = first_write.wrapping_add(1); // always distinct from first_write
        let store = Store::empty();

        let program = single_function_program(
            vec![],
            vec![
                Effect::Insert {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(first_write)))]),
                },
                // A read right after the insert, still inside this same, uncommitted call.
                Effect::Read { table: table("t"), key: Expr::Literal(key.clone()), into: Slot(0) },
                Effect::Update {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
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

        let execution = run(&program, &call_under_test(vec![]), &store);

        // The read right after the insert saw it, before the call ever committed.
        prop_assert_eq!(committed_response(&execution), &Value::Integer(1));
        // The final state reflects the update that ran after it, not the insert it overwrote.
        let row = execution.store.row(&table("t"), &key).expect("row exists after commit");
        prop_assert_eq!(row.columns.get(&column("c")), Some(&Value::Integer(second_write)));
    }
}

// --- an abort discards all effects (docs/concept.md §2.2) ---

proptest! {
    #[test]
    fn abort_discards_all_effects(
        key in arbitrary_value(),
        value in any::<i64>(),
    ) {
        let store = Store::empty();

        let program = single_function_program(
            vec![],
            vec![Effect::Insert {
                table: table("t"),
                key: Expr::Literal(key),
                columns: columns_of([("c", Expr::Literal(Value::Integer(value)))]),
            }],
            Terminal::Abort(kiln_ir::AbortName("rejected".to_owned())),
        );

        let execution = run(&program, &call_under_test(vec![]), &store);

        let aborted_by_declared_reason = matches!(
            execution.outcome,
            Outcome::Aborted { reason: AbortReason::Declared(_) }
        );
        prop_assert!(aborted_by_declared_reason);
        prop_assert_eq!(&execution.store, &store);
    }
}

// --- the access log records every attempted access, including on abort ---

proptest! {
    #[test]
    fn access_log_records_every_attempt_including_the_one_that_aborts(
        inserted_key in arbitrary_value(),
        missing_key in arbitrary_value(),
        value in any::<i64>(),
    ) {
        prop_assume!(inserted_key != missing_key);
        let store = Store::empty();

        let program = single_function_program(
            vec![],
            vec![
                Effect::Insert {
                    table: table("t"),
                    key: Expr::Literal(inserted_key.clone()),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(value)))]),
                },
                // This update targets a different, never-inserted key: it aborts the call,
                // but it must still show up in the access log.
                Effect::Update {
                    table: table("t"),
                    key: Expr::Literal(missing_key.clone()),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(value)))]),
                },
            ],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );

        let execution = run(&program, &call_under_test(vec![]), &store);

        let aborted_on_the_missing_row = matches!(
            execution.outcome,
            Outcome::Aborted { reason: AbortReason::MissingRow { .. } }
        );
        prop_assert!(aborted_on_the_missing_row);
        prop_assert_eq!(execution.access_log.len(), 2);
        prop_assert_eq!(execution.access_log[0].table.clone(), table("t"));
        prop_assert_eq!(execution.access_log[0].kind.clone(), AccessKind::Insert);
        prop_assert_eq!(execution.access_log[0].key.clone(), inserted_key);
        prop_assert_eq!(
            execution.access_log[0].columns.clone(),
            AccessedColumns::Named(vec![column("c")])
        );
        prop_assert_eq!(execution.access_log[1].kind.clone(), AccessKind::Update);
        prop_assert_eq!(execution.access_log[1].key.clone(), missing_key);
        // The whole call aborted, so the insert the log remembers never actually happened.
        prop_assert_eq!(&execution.store, &store);
    }
}

// --- the access log entry is complete for every kind of access (docs/concept.md §2.2, D5) ---

#[derive(Debug, Clone, Copy)]
enum AccessCase {
    PointRead,
    Insert,
    Update,
    Delete,
}

#[test]
fn access_log_entry_is_complete_for_every_access_kind() {
    for case in [
        AccessCase::PointRead,
        AccessCase::Insert,
        AccessCase::Update,
        AccessCase::Delete,
    ] {
        let key = Value::Integer(1);
        let existing_row = Row {
            columns: columns_of_values([("c", Value::Integer(0))]),
        };
        let (store, effect, expected_kind, expected_columns) = match case {
            AccessCase::PointRead => (
                Store::containing([(table("t"), key.clone(), existing_row)]),
                Effect::Read {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                    into: Slot(0),
                },
                AccessKind::PointRead,
                AccessedColumns::EntireRow,
            ),
            AccessCase::Insert => (
                Store::empty(),
                Effect::Insert {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(1)))]),
                },
                AccessKind::Insert,
                AccessedColumns::Named(vec![column("c")]),
            ),
            AccessCase::Update => (
                Store::containing([(table("t"), key.clone(), existing_row)]),
                Effect::Update {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                    columns: columns_of([("c", Expr::Literal(Value::Integer(1)))]),
                },
                AccessKind::Update,
                AccessedColumns::Named(vec![column("c")]),
            ),
            AccessCase::Delete => (
                Store::containing([(table("t"), key.clone(), existing_row)]),
                Effect::Delete {
                    table: table("t"),
                    key: Expr::Literal(key.clone()),
                },
                AccessKind::Delete,
                AccessedColumns::EntireRow,
            ),
        };

        let program = single_function_program(
            vec![],
            vec![effect],
            Terminal::Return(Expr::Literal(Value::Integer(0))),
        );
        let execution = run(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            execution.access_log.len(),
            1,
            "case {case:?}: expected exactly one logged access"
        );
        let entry = &execution.access_log[0];
        assert_eq!(entry.table, table("t"), "case {case:?}");
        assert_eq!(entry.kind, expected_kind, "case {case:?}");
        assert_eq!(entry.key, key, "case {case:?}");
        assert_eq!(entry.columns, expected_columns, "case {case:?}");
    }
}

// --- a successful delete removes the row (docs/concept.md §2.2, D6) ---

#[test]
fn successful_delete_removes_the_row() {
    let key = Value::Integer(1);
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
            key: Expr::Literal(key.clone()),
        }],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );

    let execution = run(&program, &call_under_test(vec![]), &store);

    assert!(matches!(execution.outcome, Outcome::Committed { .. }));
    assert_eq!(execution.store.row(&table("t"), &key), None);
}

// --- an update overwrites only its named columns (docs/concept.md §2.2) ---

#[test]
fn update_preserves_untouched_columns() {
    let key = Value::Integer(1);
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
            key: Expr::Literal(key.clone()),
            columns: columns_of([("a", Expr::Literal(Value::Integer(99)))]),
        }],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );

    let execution = run(&program, &call_under_test(vec![]), &store);

    let row = execution
        .store
        .row(&table("t"), &key)
        .expect("row still exists after the update");
    assert_eq!(row.columns.get(&column("a")), Some(&Value::Integer(99)));
    assert_eq!(row.columns.get(&column("b")), Some(&Value::Integer(2)));
}
