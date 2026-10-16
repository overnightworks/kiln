//! Primary-key prefix ranges (`docs/concept.md` §2.2 D8) driven through
//! `kiln_ir::run` (`specs/kiln-ir.spec.md`, KILN-IR-91 to KILN-IR-93): which
//! rows a prefix and an interval match, how the access log records them, and
//! how their components are checked.
mod support;

use kiln_ir::{
    AbortReason, AccessKind, AccessTarget, Effect, Expr, FunctionFault, IntegerWidth, Key,
    KeyComponentFault, KeyInterval, Slot, Store, Value,
};
use support::world::*;
use support::{
    aborted_reason, call_under_test, column, integer, program_with_schema, run_ok, table,
    typed_parameter,
};

/// The `lines` rows the range reads, as the keys a call iterating them copies
/// into `seen`.
fn keys_read_by(prefix: &[i64], interval: Option<(i64, i64)>, store: &Store) -> Vec<Key> {
    let execution = run_world(
        vec![
            read_lines_range(prefix, interval, Slot(0)),
            copy_keys_into_seen(Slot(0)),
        ],
        store,
    );
    keys_in_seen(&execution)
}

fn lines_at(keys: &[(i64, i64, i64)]) -> Store {
    lines_store(
        &keys
            .iter()
            .map(|&(w, d, o)| (w, d, o, 0))
            .collect::<Vec<_>>(),
    )
}

fn keys_of(keys: &[(i64, i64, i64)]) -> Vec<Key> {
    keys.iter().map(|&(w, d, o)| line_key(w, d, o)).collect()
}

mod kiln_ir_91 {
    use super::*;

    #[test]
    fn a_range_matches_the_keys_holding_the_prefix_whose_next_component_is_in_the_interval() {
        let store = lines_at(&[
            (1, 1, 9),
            (1, 2, 0),
            (1, 2, 7),
            (1, 3, 5),
            (1, 4, 0),
            (0, 2, 5),
            (2, 2, 5),
        ]);

        let read = keys_read_by(&[1], Some((2, 3)), &store);

        assert_eq!(read, keys_of(&[(1, 2, 0), (1, 2, 7), (1, 3, 5)]));
    }

    #[test]
    fn an_omitted_interval_matches_every_value_of_the_next_component_including_the_extremes() {
        let store = lines_at(&[
            (1, 2, i64::MIN),
            (1, 2, 0),
            (1, 2, i64::MAX),
            (1, 3, 0),
            (2, 2, 0),
        ]);

        let read = keys_read_by(&[1, 2], None, &store);

        assert_eq!(
            read,
            keys_of(&[(1, 2, i64::MIN), (1, 2, 0), (1, 2, i64::MAX)])
        );
    }

    #[test]
    fn an_empty_prefix_without_interval_reads_the_whole_table_in_key_order() {
        let all = [(2, 0, 0), (i32::MIN as i64, -128, i64::MIN), (1, 5, 5)];
        let store = lines_at(&all);

        let read = keys_read_by(&[], None, &store);

        assert_eq!(
            read,
            keys_of(&[(i32::MIN as i64, -128, i64::MIN), (1, 5, 5), (2, 0, 0)])
        );
    }

    #[test]
    fn a_reversed_interval_reads_as_an_empty_sequence() {
        let store = lines_at(&[(1, 2, 0), (1, 3, 0)]);

        let read = keys_read_by(&[1], Some((3, 2)), &store);

        assert_eq!(read, Vec::<Key>::new());
    }

    #[test]
    fn a_prefix_as_long_as_the_key_or_longer_is_rejected_before_any_call_runs() {
        for prefix_len in [3, 4] {
            let prefix = vec![1; prefix_len];
            let program = world_program(
                vec![Effect::ReadRange {
                    table: table("lines"),
                    prefix: prefix.iter().copied().map(literal).collect(),
                    interval: None,
                    bound: literal(1),
                    into: Slot(0),
                }],
                returning_zero(),
            );

            assert_eq!(
                support::world::rejection_of(&program),
                function_fault(FunctionFault::KeyArity {
                    table: table("lines"),
                    expected: 3,
                    given: prefix_len,
                })
            );
        }
    }
}

mod kiln_ir_92 {
    use super::*;

    fn logged_range(prefix: &[i64], interval: Option<(i64, i64)>) -> AccessTarget {
        let execution = run_world(
            vec![read_lines_range(prefix, interval, Slot(0))],
            &Store::empty(),
        );
        let [entry] = &execution.access_log[..] else {
            panic!("one range read logs one entry: {:?}", execution.access_log);
        };
        assert_eq!(entry.kind, AccessKind::RangeRead);
        entry.target.clone()
    }

    #[test]
    fn a_range_logs_its_prefix_and_interval_as_written_even_when_it_matches_nothing() {
        assert_eq!(
            logged_range(&[1, 2], Some((3, 9))),
            AccessTarget::Range {
                prefix: vec![integer(1), integer(2)],
                interval: Some((integer(3), integer(9))),
            }
        );
    }

    #[test]
    fn a_reversed_interval_is_logged_as_written() {
        assert_eq!(
            logged_range(&[1], Some((9, 3))),
            AccessTarget::Range {
                prefix: vec![integer(1)],
                interval: Some((integer(9), integer(3))),
            }
        );
    }

    #[test]
    fn an_omitted_interval_is_logged_as_omitted_and_not_as_an_unbounded_one() {
        let omitted = logged_range(&[1], None);

        assert_eq!(
            omitted,
            AccessTarget::Range {
                prefix: vec![integer(1)],
                interval: None,
            }
        );
        assert_ne!(omitted, logged_range(&[1], Some((-128, 127))));
    }
}

mod kiln_ir_93 {
    use super::*;

    fn range_by_parameter(prefix: Vec<Expr>, interval: Option<KeyInterval>) -> Effect {
        Effect::ReadRange {
            table: table("lines"),
            prefix,
            interval,
            bound: literal(1),
            into: Slot(0),
        }
    }

    fn parameter_expr() -> Expr {
        Expr::Parameter(support::parameter("n"))
    }

    fn abort_of(effect: Effect, argument: i64) -> (AbortReason, usize) {
        let program = program_with_schema(
            world_schema(),
            vec![typed_parameter("n", IntegerWidth::I64)],
            vec![effect],
            returning_zero(),
        );
        let execution = run_ok(
            &program,
            &call_under_test(vec![Value::Integer(argument)]),
            &lines_at(&[(1, 2, 3)]),
        );
        (
            aborted_reason(&execution).clone(),
            execution.access_log.len(),
        )
    }

    fn rejected(column_name: &str, fault: KeyComponentFault) -> AbortReason {
        AbortReason::KeyComponentRejected {
            table: table("lines"),
            column: column(column_name),
            fault,
        }
    }

    #[test]
    fn a_prefix_component_outside_its_key_column_aborts_naming_that_column() {
        let effect = range_by_parameter(vec![parameter_expr()], None);

        let (reason, logged) = abort_of(effect, i64::from(i32::MAX) + 1);

        assert_eq!(reason, rejected("w", KeyComponentFault::OutsideWidth));
        assert_eq!(logged, 0);
    }

    #[test]
    fn an_interval_bound_outside_the_column_after_the_prefix_aborts_naming_that_column() {
        for (from, to) in [
            (parameter_expr(), literal(1)),
            (literal(1), parameter_expr()),
        ] {
            let effect = range_by_parameter(vec![literal(1)], Some(KeyInterval { from, to }));

            let (reason, logged) = abort_of(effect, 128);

            assert_eq!(reason, rejected("d", KeyComponentFault::OutsideWidth));
            assert_eq!(logged, 0);
        }
    }
}
