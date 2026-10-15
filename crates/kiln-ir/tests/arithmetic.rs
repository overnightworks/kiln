//! D3's checked arithmetic, declared width, and clamping-domain behaviour
//! (`docs/concept.md` §2.2). Every other behaviour of `kiln_ir::run` lives
//! in `tests/evaluator.rs` instead.

mod support;

use kiln_ir::{
    run, AbortReason, ArithmeticOp, ClampDomain, Column, ColumnName, Effect, Expr, IntegerWidth,
    Outcome, OverflowReason, Row, SchemaError, Store, Table, TableName, Terminal, Value,
};
use proptest::prelude::*;
use support::{call_under_test, column, columns_of, program_with_column, run_ok, table};

fn add(lhs: i64, rhs: i64) -> Expr {
    Expr::Arithmetic(
        ArithmeticOp::Add,
        Box::new(Expr::Literal(Value::Integer(lhs))),
        Box::new(Expr::Literal(Value::Integer(rhs))),
    )
}

fn arithmetic(op: ArithmeticOp, lhs: i64, rhs: i64) -> Expr {
    Expr::Arithmetic(
        op,
        Box::new(Expr::Literal(Value::Integer(lhs))),
        Box::new(Expr::Literal(Value::Integer(rhs))),
    )
}

/// Builds a program with one declared column and one write effect of
/// `write_expr` into it, run either as an `Insert` of a fresh key or an
/// `Update` of a key the store already carries that same column on — so
/// each boundary case below proves itself through both write kinds (D3).
#[derive(Debug, Clone, Copy)]
enum WriteVia {
    Insert,
    Update,
}

fn run_write(declared_column: Column, write_expr: Expr, via: WriteVia) -> kiln_ir::Execution {
    assert_eq!(
        declared_column.name,
        column("c"),
        "run_write always writes into the column named \"c\""
    );
    let key = Value::Integer(1);
    let store = match via {
        WriteVia::Insert => Store::empty(),
        WriteVia::Update => Store::containing([(
            table("t"),
            key.clone(),
            Row {
                columns: support::columns_of_values([("c", Value::Integer(0))]),
            },
        )]),
    };
    let effect = match via {
        WriteVia::Insert => Effect::Insert {
            table: table("t"),
            key: Expr::Literal(key),
            columns: columns_of([("c", write_expr)]),
        },
        WriteVia::Update => Effect::Update {
            table: table("t"),
            key: Expr::Literal(key),
            columns: columns_of([("c", write_expr)]),
        },
    };
    let program = program_with_column(
        declared_column,
        vec![],
        vec![effect],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );

    run_ok(&program, &call_under_test(vec![]), &store)
}

// --- D3: arithmetic runs at a column's declared width; overflow aborts unless the column
// clamps (docs/concept.md §2.2) ---

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

// --- D3: a value whose nested sub-expression already overflows i64 aborts arithmetically —
// never clamped, even into a column that declares a domain, since a clamp only ever judges a
// write's one final value (docs/concept.md §2.2) ---

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

// --- D3: intermediate arithmetic is checked at i64 only; a nested result that itself stays
// inside i64 but briefly exceeds the column's width still commits once the expression's one
// final value returns inside it (docs/concept.md §2.2) ---

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
    assert_eq!(execution.access_log[0].key, Value::Integer(1));
    assert_eq!(execution.access_log[1].key, Value::Integer(2));
}

// --- D3: arithmetic overflowing i64 in a response, outside any column write, aborts with a
// typed reason instead of panicking (docs/concept.md §2.2) ---

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

// --- D3: a clamping domain saturates computed and literal writes alike, across the whole
// domain, instead of aborting (docs/concept.md §2.2) ---

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

// --- D1: a column the schema does not declare carries no width or domain, so a write to it
// resolves exactly as it always has (docs/concept.md §2.2) ---

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

// --- D1: expressions may project a column from a row an earlier read bound, and use it as an
// arithmetic operand (docs/concept.md §2.2) ---

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
        vec![support::parameter("amount")],
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

// --- D3: Add and Subtract are checked at each declared width's own boundary, for a computed
// write made through Insert and through Update alike (docs/concept.md §2.2) ---

#[derive(Debug, Clone, Copy)]
enum Expected {
    Committed(i64),
    WidthOverflow,
    ArithmeticOverflow,
}

struct BoundaryCase {
    name: &'static str,
    op: ArithmeticOp,
    width: IntegerWidth,
    lhs: i64,
    rhs: i64,
    expected: Expected,
}

fn boundary_cases() -> Vec<BoundaryCase> {
    vec![
        BoundaryCase {
            name: "i8 add at max commits",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I8,
            lhs: 100,
            rhs: 27,
            expected: Expected::Committed(127),
        },
        BoundaryCase {
            name: "i8 add past max overflows the width",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I8,
            lhs: 100,
            rhs: 28,
            expected: Expected::WidthOverflow,
        },
        BoundaryCase {
            name: "i8 subtract at min commits",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I8,
            lhs: -100,
            rhs: 28,
            expected: Expected::Committed(-128),
        },
        BoundaryCase {
            name: "i8 subtract past min overflows the width",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I8,
            lhs: -100,
            rhs: 29,
            expected: Expected::WidthOverflow,
        },
        BoundaryCase {
            name: "i16 add at max commits",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I16,
            lhs: 32_000,
            rhs: 767,
            expected: Expected::Committed(32_767),
        },
        BoundaryCase {
            name: "i16 add past max overflows the width",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I16,
            lhs: 32_000,
            rhs: 768,
            expected: Expected::WidthOverflow,
        },
        BoundaryCase {
            name: "i16 subtract at min commits",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I16,
            lhs: -32_000,
            rhs: 768,
            expected: Expected::Committed(-32_768),
        },
        BoundaryCase {
            name: "i16 subtract past min overflows the width",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I16,
            lhs: -32_000,
            rhs: 769,
            expected: Expected::WidthOverflow,
        },
        BoundaryCase {
            name: "i32 add at max commits",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I32,
            lhs: 2_147_483_600,
            rhs: 47,
            expected: Expected::Committed(2_147_483_647),
        },
        BoundaryCase {
            name: "i32 add past max overflows the width",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I32,
            lhs: 2_147_483_600,
            rhs: 48,
            expected: Expected::WidthOverflow,
        },
        BoundaryCase {
            name: "i32 subtract at min commits",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I32,
            lhs: -2_147_483_600,
            rhs: 48,
            expected: Expected::Committed(-2_147_483_648),
        },
        BoundaryCase {
            name: "i32 subtract past min overflows the width",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I32,
            lhs: -2_147_483_600,
            rhs: 49,
            expected: Expected::WidthOverflow,
        },
        // I64's declared width spans all of i64, so nothing before the true i64
        // boundary can overflow it — only true i64 overflow aborts, and it does so
        // arithmetically, never as a width overflow.
        BoundaryCase {
            name: "i64 add at max commits",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I64,
            lhs: i64::MAX - 1,
            rhs: 1,
            expected: Expected::Committed(i64::MAX),
        },
        BoundaryCase {
            name: "i64 add past max is an arithmetic overflow",
            op: ArithmeticOp::Add,
            width: IntegerWidth::I64,
            lhs: i64::MAX,
            rhs: 1,
            expected: Expected::ArithmeticOverflow,
        },
        BoundaryCase {
            name: "i64 subtract at min commits",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I64,
            lhs: i64::MIN + 1,
            rhs: 1,
            expected: Expected::Committed(i64::MIN),
        },
        BoundaryCase {
            name: "i64 subtract past min is an arithmetic overflow",
            op: ArithmeticOp::Subtract,
            width: IntegerWidth::I64,
            lhs: i64::MIN,
            rhs: 1,
            expected: Expected::ArithmeticOverflow,
        },
    ]
}

#[test]
fn width_boundaries_are_enforced_for_add_and_subtract_via_insert_and_update() {
    for case in boundary_cases() {
        for via in [WriteVia::Insert, WriteVia::Update] {
            let declared_column = Column {
                name: column("c"),
                width: case.width,
                domain: None,
            };
            let execution = run_write(
                declared_column,
                arithmetic(case.op, case.lhs, case.rhs),
                via,
            );

            match case.expected {
                Expected::Committed(value) => {
                    let row = execution
                        .store
                        .row(&table("t"), &Value::Integer(1))
                        .unwrap_or_else(|| {
                            panic!("case {} ({via:?}): expected a commit", case.name)
                        });
                    assert_eq!(
                        row.columns.get(&column("c")),
                        Some(&Value::Integer(value)),
                        "case {} ({via:?})",
                        case.name
                    );
                }
                Expected::WidthOverflow => {
                    assert_eq!(
                        execution.outcome,
                        Outcome::Aborted {
                            reason: AbortReason::Overflow(OverflowReason::Width {
                                table: table("t"),
                                column: column("c"),
                                key: Value::Integer(1),
                            }),
                        },
                        "case {} ({via:?})",
                        case.name
                    );
                }
                Expected::ArithmeticOverflow => {
                    assert_eq!(
                        execution.outcome,
                        Outcome::Aborted {
                            reason: AbortReason::Overflow(OverflowReason::Arithmetic),
                        },
                        "case {} ({via:?})",
                        case.name
                    );
                }
            }
        }
    }
}

// --- D3: a clamping domain must lie within its column's own declared width and have
// min <= max; the schema itself is rejected before any call runs instead of panicking
// (docs/concept.md §2.2) ---

fn table_name() -> TableName {
    table("t")
}

fn column_name() -> ColumnName {
    column("c")
}

fn schema_with_domain(width: IntegerWidth, domain: ClampDomain) -> kiln_ir::Schema {
    kiln_ir::Schema {
        tables: vec![Table {
            name: table_name(),
            columns: vec![Column {
                name: column_name(),
                width,
                domain: Some(domain),
            }],
        }],
    }
}

#[test]
fn a_domain_reaching_outside_its_columns_width_is_rejected() {
    let schema = schema_with_domain(IntegerWidth::I8, ClampDomain { min: 0, max: 1000 });

    assert_eq!(
        schema.validate(),
        Err(SchemaError::DomainExceedsWidth {
            table: table_name(),
            column: column_name(),
        })
    );
}

#[test]
fn a_domain_with_reversed_bounds_is_rejected() {
    let schema = schema_with_domain(IntegerWidth::I32, ClampDomain { min: 100, max: 0 });

    assert_eq!(
        schema.validate(),
        Err(SchemaError::ReversedDomain {
            table: table_name(),
            column: column_name(),
            min: 100,
            max: 0,
        })
    );
}

// --- D3: a domain's own bounds and width containment are checked at their exact boundary,
// not only when they are far outside it ---

#[test]
fn a_domain_whose_min_equals_its_max_is_accepted() {
    let schema = schema_with_domain(IntegerWidth::I32, ClampDomain { min: 50, max: 50 });

    assert_eq!(schema.validate(), Ok(()));
}

#[test]
fn a_domain_exactly_at_its_columns_width_bounds_is_accepted() {
    let (width_min, width_max) = IntegerWidth::I8.range();
    let schema = schema_with_domain(
        IntegerWidth::I8,
        ClampDomain {
            min: width_min,
            max: width_max,
        },
    );

    assert_eq!(schema.validate(), Ok(()));
}

#[test]
fn a_domain_one_below_its_columns_width_minimum_is_rejected() {
    let (width_min, _) = IntegerWidth::I8.range();
    let schema = schema_with_domain(
        IntegerWidth::I8,
        ClampDomain {
            min: width_min - 1,
            max: width_min,
        },
    );

    assert_eq!(
        schema.validate(),
        Err(SchemaError::DomainExceedsWidth {
            table: table_name(),
            column: column_name(),
        })
    );
}

#[test]
fn a_domain_one_above_its_columns_width_maximum_is_rejected() {
    let (_, width_max) = IntegerWidth::I8.range();
    let schema = schema_with_domain(
        IntegerWidth::I8,
        ClampDomain {
            min: width_max,
            max: width_max + 1,
        },
    );

    assert_eq!(
        schema.validate(),
        Err(SchemaError::DomainExceedsWidth {
            table: table_name(),
            column: column_name(),
        })
    );
}

#[test]
fn run_rejects_an_invalid_schema_instead_of_panicking() {
    let schema = schema_with_domain(IntegerWidth::I8, ClampDomain { min: 0, max: 1000 });
    let program = kiln_ir::Program {
        schema,
        functions: vec![kiln_ir::Function {
            name: kiln_ir::FunctionName("under_test".to_owned()),
            parameters: vec![],
            body: kiln_ir::Block {
                effects: vec![],
                terminal: Terminal::Return(Expr::Literal(Value::Integer(0))),
            },
        }],
    };

    let result = run(&program, &call_under_test(vec![]), &Store::empty());

    assert_eq!(
        result,
        Err(SchemaError::DomainExceedsWidth {
            table: table_name(),
            column: column_name(),
        })
    );
}
