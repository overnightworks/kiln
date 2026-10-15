//! D3's checked arithmetic at each declared width's own exact boundary, and
//! a clamping domain's own validity against that width — split out of
//! `tests/arithmetic.rs` so neither file approaches the repository's size
//! ratchet. `tests/arithmetic.rs` covers D3's ordinary overflow, width, and
//! clamp behaviour instead.

mod support;

use kiln_ir::{
    run, AbortReason, ArithmeticOp, ClampDomain, Column, ColumnName, Expr, IntegerWidth, Outcome,
    OverflowReason, SchemaError, Store, TableName, Terminal, Value,
};
use support::{call_under_test, column, run_write, table, WriteVia};

fn arithmetic(op: ArithmeticOp, lhs: i64, rhs: i64) -> Expr {
    Expr::Arithmetic(
        op,
        Box::new(Expr::Literal(Value::Integer(lhs))),
        Box::new(Expr::Literal(Value::Integer(rhs))),
    )
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

mod kiln_ir_23 {
    use super::*;

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
        tables: vec![kiln_ir::Table {
            name: table_name(),
            columns: vec![Column {
                name: column_name(),
                width,
                domain: Some(domain),
            }],
        }],
    }
}

mod kiln_ir_02 {
    use super::*;

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
}

mod kiln_ir_01 {
    use super::*;

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

    #[test]
    fn a_domain_whose_min_equals_its_max_is_accepted() {
        let schema = schema_with_domain(IntegerWidth::I32, ClampDomain { min: 50, max: 50 });

        assert_eq!(schema.validate(), Ok(()));
    }
}

// --- D3: a domain's own bounds and width containment are checked at their exact boundary,
// not only when they are far outside it ---

mod kiln_ir_03 {
    use super::*;

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
}
