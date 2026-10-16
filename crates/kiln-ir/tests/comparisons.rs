//! Comparison conditions, driven through `kiln_ir::run`
//! (`specs/kiln-ir.spec.md`, KILN-IR-73 to KILN-IR-75).

mod support;

use kiln_ir::{
    AbortReason, ArithmeticOp, Block, Condition, Effect, Expr, Outcome, Relation, Slot, Store,
    Terminal, Value,
};
use support::{
    aborted_reason, call_under_test, column, committed_response, integer, literal_key, run_ok,
    single_function_program, store_of_rows, table, text, typed_parameter,
};

const HOLDS: i64 = 1;
const FAILS: i64 = 0;

fn literal(value: Value) -> Expr {
    Expr::Literal(value)
}

fn returning(value: i64) -> Box<Block> {
    Box::new(Block {
        effects: vec![],
        terminal: Terminal::Return(literal(integer(value))),
    })
}

fn compare(left: Expr, relation: Relation, right: Expr) -> Terminal {
    Terminal::Branch {
        condition: Condition::Compare {
            left,
            relation,
            right,
        },
        when_present: returning(HOLDS),
        when_absent: returning(FAILS),
    }
}

fn response_of(terminal: Terminal, effects: Vec<Effect>, store: &Store) -> Value {
    let program = single_function_program(vec![], effects, terminal);
    committed_response(&run_ok(&program, &call_under_test(vec![]), store)).clone()
}

fn holds(left: Value, relation: Relation, right: Value) -> bool {
    let terminal = compare(literal(left), relation, literal(right));
    response_of(terminal, vec![], &Store::empty()) == integer(HOLDS)
}

mod kiln_ir_73 {
    use super::*;

    #[test]
    fn a_comparison_takes_the_first_arm_exactly_when_it_holds() {
        use Relation::*;
        let cases = [
            (integer(1), Equal, integer(1), true),
            (integer(1), Equal, integer(2), false),
            (integer(1), NotEqual, integer(2), true),
            (integer(1), NotEqual, integer(1), false),
            (integer(1), Less, integer(2), true),
            (integer(2), Less, integer(2), false),
            (integer(3), Less, integer(2), false),
            (integer(1), LessOrEqual, integer(2), true),
            (integer(2), LessOrEqual, integer(2), true),
            (integer(3), LessOrEqual, integer(2), false),
            (integer(3), Greater, integer(2), true),
            (integer(2), Greater, integer(2), false),
            (integer(1), Greater, integer(2), false),
            (integer(3), GreaterOrEqual, integer(2), true),
            (integer(2), GreaterOrEqual, integer(2), true),
            (integer(1), GreaterOrEqual, integer(2), false),
            (integer(-5), Less, integer(3), true),
            (text("a"), Equal, text("a"), true),
            (text("a"), Equal, text("b"), false),
            (text("a"), Less, text("b"), true),
            (text("b"), Less, text("a"), false),
            (text("B"), Less, text("a"), true),
            (text("ab"), Greater, text("a"), true),
            (text(""), LessOrEqual, text("a"), true),
        ];
        for (left, relation, right, expected) in cases {
            assert_eq!(
                holds(left.clone(), relation, right.clone()),
                expected,
                "{left:?} {relation:?} {right:?}"
            );
        }
    }

    #[test]
    fn an_argument_decides_the_arm() {
        let program = single_function_program(
            vec![typed_parameter("kind", kiln_ir::IntegerWidth::I8)],
            vec![],
            compare(
                Expr::Parameter(support::parameter("kind")),
                Relation::Equal,
                literal(integer(0)),
            ),
        );
        for (kind, expected) in [(0, HOLDS), (1, FAILS)] {
            let execution = run_ok(
                &program,
                &call_under_test(vec![integer(kind)]),
                &Store::empty(),
            );
            assert_eq!(committed_response(&execution), &integer(expected));
        }
    }

    #[test]
    fn a_projected_column_decides_the_arm() {
        let store = store_of_rows(&[("t", 1, vec![("status", integer(7))])]);
        let read = Effect::Read {
            table: table("t"),
            key: literal_key(1),
            into: Slot(0),
        };
        let projected = Expr::Projection {
            slot: Slot(0),
            column: column("status"),
        };
        let terminal = compare(projected, Relation::Greater, literal(integer(5)));
        assert_eq!(response_of(terminal, vec![read], &store), integer(HOLDS));
    }

    #[test]
    fn arithmetic_and_key_columns_are_comparable_operands() {
        let store = store_of_rows(&[("t", 4, vec![])]);
        let read = Effect::Read {
            table: table("t"),
            key: literal_key(4),
            into: Slot(0),
        };
        let key_plus_one = Expr::Arithmetic(
            ArithmeticOp::Add,
            Box::new(Expr::Projection {
                slot: Slot(0),
                column: column("id"),
            }),
            Box::new(literal(integer(1))),
        );
        let terminal = compare(key_plus_one, Relation::Equal, literal(integer(5)));
        assert_eq!(response_of(terminal, vec![read], &store), integer(HOLDS));
    }
}

mod kiln_ir_74 {
    use super::*;

    #[test]
    fn comparing_an_integer_with_text_aborts_with_a_typed_reason_for_every_relation() {
        use Relation::*;
        for relation in [Equal, NotEqual, Less, LessOrEqual, Greater, GreaterOrEqual] {
            for (left, right) in [(integer(1), text("1")), (text("1"), integer(1))] {
                let terminal = compare(literal(left), relation, literal(right));
                let program = single_function_program(vec![], vec![], terminal);
                let store = Store::empty();

                let execution = run_ok(&program, &call_under_test(vec![]), &store);

                assert_eq!(
                    aborted_reason(&execution),
                    &AbortReason::ComparisonTypeMismatch
                );
                assert_eq!(execution.store, store);
            }
        }
    }
}

mod kiln_ir_75 {
    use super::*;

    #[test]
    fn an_unresolvable_operand_aborts_with_its_own_reason_and_logs_no_access() {
        let absent = Expr::Projection {
            slot: Slot(0),
            column: column("status"),
        };
        let overflowing = Expr::Arithmetic(
            ArithmeticOp::Add,
            Box::new(literal(integer(i64::MAX))),
            Box::new(literal(integer(1))),
        );
        let cases = [
            (
                absent,
                AbortReason::ProjectionSourceAbsent { slot: Slot(0) },
            ),
            (
                overflowing,
                AbortReason::Overflow(kiln_ir::OverflowReason::Arithmetic),
            ),
        ];
        for (operand, reason) in cases {
            let read = Effect::Read {
                table: table("t"),
                key: literal_key(9),
                into: Slot(0),
            };
            let terminal = compare(operand, Relation::Equal, literal(integer(0)));
            let program = single_function_program(vec![], vec![read], terminal);

            let execution = run_ok(&program, &call_under_test(vec![]), &Store::empty());

            assert_eq!(execution.outcome, Outcome::Aborted { reason });
            assert_eq!(execution.access_log.len(), 1, "only the read is logged");
        }
    }
}
