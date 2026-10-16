use kiln_e1::UnknownCause;
use kiln_ir::{ArithmeticOp, Block, Condition, Effect, Expr, Relation, Slot};

use super::programs::*;

fn times(left: Expr, right: Expr) -> Expr {
    Expr::Arithmetic(ArithmeticOp::Multiply, Box::new(left), Box::new(right))
}

fn when(condition: Condition, when_true: Expr, when_false: Expr) -> Expr {
    Expr::Conditional {
        condition: Box::new(condition),
        when_true: Box::new(when_true),
        when_false: Box::new(when_false),
    }
}

fn equal(left: Expr, right: Expr) -> Condition {
    Condition::Compare {
        left,
        relation: Relation::Equal,
        right,
    }
}

mod kiln_e1_38 {
    use super::*;

    fn after_an_order(key: Expr) -> Block {
        returns(vec![read_order(), read("accounts", key, 1)])
    }

    #[test]
    fn a_key_computed_from_arguments_by_multiply_or_condition_is_static() {
        let keys = [
            times(param("quantity"), literal(100)),
            when(equal(param("a"), literal(1)), param("b"), literal(2)),
            when(
                equal(param("a"), literal(1)),
                times(param("b"), param("b")),
                param("c"),
            ),
        ];
        for key in keys {
            assert_eq!(category_of(returns(vec![read("accounts", key, 0)])), STATIC);
        }
    }

    #[test]
    fn a_key_multiplying_a_read_result_is_unknown_as_computed_from_a_read() {
        assert_unknown(
            after_an_order(times(column_of(0, "customer_id"), literal(2))),
            computed_from_read("accounts"),
        );
    }

    #[test]
    fn a_key_conditional_on_or_choosing_a_read_result_is_unknown_as_computed_from_a_read() {
        let keys = [
            when(
                equal(column_of(0, "customer_id"), literal(1)),
                param("a"),
                param("b"),
            ),
            when(
                equal(param("a"), literal(1)),
                column_of(0, "customer_id"),
                param("b"),
            ),
            when(Condition::RowPresent(Slot(0)), param("a"), param("b")),
        ];
        for key in keys {
            assert_unknown(after_an_order(key), computed_from_read("accounts"));
        }
    }

    fn computed_from_read(name: &str) -> UnknownCause {
        UnknownCause::KeyComputedFromRead { table: table(name) }
    }
}

mod kiln_e1_39 {
    use super::*;

    fn low_to_high(table: &str, into: u32) -> Effect {
        read_range(table, param("low"), param("high"), into)
    }

    /// Slot 1 is the row of an iteration that has ended, so it holds no row.
    fn released() -> Expr {
        key_of(1)
    }

    /// Reads `accounts` by `key` after slot 1 was released, then accesses `b`.
    fn reading_by(key: Expr) -> Block {
        returns(vec![
            low_to_high("orders", 0),
            iterate(0, 1, vec![]),
            read("accounts", key, 2),
            update("b", param("y"), &[]),
        ])
    }

    fn reaches_the_access_after(body: Block) -> bool {
        evidence_of(body)
            .iter()
            .any(|node| node == "f/body/effects[3]")
    }

    #[test]
    fn a_projection_in_an_arm_the_condition_does_not_decide_never_ends_the_path() {
        let key = when(equal(param("a"), literal(1)), param("x"), released());

        let body = reading_by(key);

        assert_eq!(category_of(body.clone()), STATIC);
        assert!(reaches_the_access_after(body));
    }

    #[test]
    fn a_projection_in_the_arm_a_decided_condition_chooses_ends_the_path_like_the_evaluator() {
        let chosen = when(equal(literal(1), literal(2)), param("x"), released());
        let unchosen = when(equal(literal(1), literal(1)), param("x"), released());
        let released_slot_is_never_present =
            when(Condition::RowPresent(Slot(1)), released(), param("x"));

        assert!(!reaches_the_access_after(reading_by(chosen)));
        assert!(reaches_the_access_after(reading_by(unchosen)));
        assert!(reaches_the_access_after(reading_by(
            released_slot_is_never_present
        )));
    }

    #[test]
    fn a_presence_test_the_path_already_settled_chooses_its_arm() {
        let key = when(Condition::RowPresent(Slot(0)), param("x"), released());
        let absent = returns(vec![
            low_to_high("orders", 1),
            iterate(1, 2, vec![]),
            read("accounts", key, 3),
            update("b", param("y"), &[]),
        ]);
        let body = branch_on_presence(
            vec![read("accounts", param("k"), 0)],
            0,
            returns(vec![]),
            absent,
        );

        let nodes = evidence_of(body);

        assert!(nodes.contains(&"f/body/terminal/when_absent/effects[2]".to_owned()));
        assert!(!nodes.contains(&"f/body/terminal/when_absent/effects[3]".to_owned()));
    }

    #[test]
    fn a_slot_a_conditional_arm_reads_before_the_body_binds_it_is_carried() {
        let key = when(equal(param("a"), literal(1)), param("x"), key_of(3));
        let body = returns(vec![
            read("accounts", param("s"), 3),
            low_to_high("ledger", 1),
            iterate(
                1,
                2,
                vec![read("a", key, 4), read("accounts", param("k"), 3)],
            ),
        ]);

        assert_unknown(
            body,
            UnknownCause::SlotCarriedAcrossIterations { slot: Slot(3) },
        );
    }
}
