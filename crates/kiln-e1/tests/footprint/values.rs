use kiln_e1::{Classification, Evidence, ProvenCategory, Reason, UnknownCause};
use kiln_ir::{
    ArithmeticOp, Block, ColumnType, Condition, Effect, Expr, Program, Relation, Slot, Value,
};

use super::programs::*;
use crate::keys::kiln_e1_27::{
    classification_of, point_read_of_u, slot_bound_by_either_arm, table_keyed_by,
};

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

fn computed_from_read(name: &str) -> UnknownCause {
    UnknownCause::KeyComputedFromRead { table: table(name) }
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

    #[test]
    fn a_conditional_over_a_read_is_computed_even_when_its_condition_is_decided_and_every_arm_is_the_same_column(
    ) {
        let same_column = || column_of(0, "customer_id");
        let decided_by_literals = when(equal(literal(1), literal(1)), same_column(), same_column());
        let settled_presence = when(Condition::RowPresent(Slot(0)), same_column(), same_column());

        assert_unknown(
            returns(vec![
                read_order(),
                read("customers", decided_by_literals, 1),
            ]),
            computed_from_read("customers"),
        );
        assert_unknown(
            branch_on_presence(
                vec![read_order()],
                0,
                returns(vec![read("customers", settled_presence, 1)]),
                returns(vec![]),
            ),
            computed_from_read("customers"),
        );
    }

    #[test]
    fn a_decided_conditional_over_arguments_alone_stays_static() {
        let key = when(equal(literal(1), literal(1)), param("a"), param("a"));

        assert_eq!(category_of(returns(vec![read("accounts", key, 0)])), STATIC);
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
    fn a_projection_in_an_arm_the_condition_does_not_decide_makes_the_function_unknown_naming_the_access(
    ) {
        let key = when(equal(param("a"), literal(1)), param("x"), released());

        let body = reading_by(key);

        assert_unknown(body.clone(), UnknownCause::ConditionalAbortsOnOneArm);
        assert_eq!(evidence_of(body), ["f/body/effects[2]"]);
    }

    #[test]
    fn an_undecided_conditional_whose_arms_both_abort_ends_the_path_like_the_evaluator() {
        let key = when(equal(param("a"), literal(1)), released(), released());

        assert!(!reaches_the_access_after(reading_by(key)));
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
        let reading = || {
            let key = when(Condition::RowPresent(Slot(0)), param("x"), released());
            returns(vec![
                low_to_high("orders", 1),
                iterate(1, 2, vec![]),
                read("accounts", key, 3),
                update("b", param("y"), &[]),
            ])
        };
        let settled = |present, absent| {
            branch_on_presence(vec![read("accounts", param("k"), 0)], 0, present, absent)
        };

        let nodes = evidence_of(settled(returns(vec![]), reading()));

        assert!(nodes.contains(&"f/body/terminal/when_absent/effects[2]".to_owned()));
        assert!(!nodes.contains(&"f/body/terminal/when_absent/effects[3]".to_owned()));
        assert_unknown(
            settled(reading(), returns(vec![])),
            computed_from_read("accounts"),
        );
    }

    /// After common setup releases slot 1, a read of `accounts` decides
    /// whether the present arm updates `a` with a value whose conditional arm
    /// reads the released slot, then reads `b`; the absent arm only reads `b`.
    fn update_whose_value_conditional_aborts_on_one_arm(condition: Condition) -> Block {
        let value = when(condition, released(), literal(0));
        branch_on_presence(
            vec![
                low_to_high("orders", 0),
                iterate(0, 1, vec![]),
                read("accounts", param("k"), 2),
            ],
            2,
            returns(vec![
                update("a", param("k"), &[("v", value)]),
                read("b", param("k"), 3),
            ]),
            returns(vec![read("b", param("k"), 3)]),
        )
    }

    #[test]
    fn an_undecided_conditional_aborting_on_one_arm_is_unknown_naming_its_node() {
        let body =
            update_whose_value_conditional_aborts_on_one_arm(equal(param("flag"), literal(1)));

        assert_unknown(body.clone(), UnknownCause::ConditionalAbortsOnOneArm);
        assert_eq!(
            evidence_of(body),
            ["f/body/terminal/when_present/effects[0]"]
        );
    }

    #[test]
    fn a_decided_conditional_keeps_the_evaluators_order_whichever_arm_aborts() {
        let chosen =
            update_whose_value_conditional_aborts_on_one_arm(equal(literal(1), literal(1)));
        let unchosen =
            update_whose_value_conditional_aborts_on_one_arm(equal(literal(1), literal(2)));

        assert!(
            !evidence_of(chosen).contains(&"f/body/terminal/when_present/effects[1]".to_owned())
        );
        assert!(
            evidence_of(unchosen).contains(&"f/body/terminal/when_present/effects[1]".to_owned())
        );
    }

    /// The key of `u` is an integer; slot 2 holds a row of `t1`, whose text
    /// key `c` the key column cannot take, on the path that reads it.
    fn reading_u_by(condition: Condition) -> Classification {
        let key = when(condition, column_of(2, "c"), literal(1));
        classification_of(&slot_bound_by_either_arm("t1", key, point_read_of_u))
    }

    fn evidence_paths(classification: &Classification) -> Vec<String> {
        match classification {
            Classification::Proven { evidence, .. } => evidence.paths().to_vec(),
            Classification::Unknown { evidence, .. } => evidence.as_ref().unwrap().paths().to_vec(),
            Classification::ExcludedNonDatabase(_) => unreachable!(),
        }
    }

    const ACCESS_TO_U: &str = "f/body/terminal/when_present/effects[3]";

    #[test]
    fn a_misfitting_key_in_the_unchosen_arm_keeps_the_access() {
        let classification = reading_u_by(equal(literal(1), literal(2)));

        assert!(matches!(classification, Classification::Proven { .. }));
        assert!(evidence_paths(&classification).contains(&ACCESS_TO_U.to_owned()));
    }

    #[test]
    fn a_misfitting_key_in_the_chosen_arm_ends_the_path_before_its_access() {
        let classification = reading_u_by(equal(literal(1), literal(1)));

        assert!(matches!(classification, Classification::Unknown { .. }));
        assert!(!evidence_paths(&classification).contains(&ACCESS_TO_U.to_owned()));
    }

    #[test]
    fn a_misfitting_key_in_an_undecided_arm_is_unknown_naming_the_access() {
        let classification = reading_u_by(equal(param("a"), literal(1)));

        assert_eq!(
            classification,
            Classification::Unknown {
                reason: Reason::new(UnknownCause::ConditionalAbortsOnOneArm.to_string()).unwrap(),
                evidence: Some(Evidence::new(vec![ACCESS_TO_U.to_owned()]).unwrap()),
            }
        );
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

    #[test]
    fn a_decided_conditional_is_computed_from_the_reads_of_its_unchosen_arm_even_where_that_arm_aborts(
    ) {
        let order_plus_released = plus(column_of(3, "customer_id"), released());
        let key = when(
            equal(literal(1), literal(1)),
            param("k"),
            order_plus_released,
        );
        let body = returns(vec![
            low_to_high("orders", 0),
            iterate(0, 1, vec![]),
            read("orders", param("order_id"), 3),
            read("accounts", key, 2),
        ]);

        assert_unknown(body, computed_from_read("accounts"));
    }

    #[test]
    fn a_conditional_inside_the_arm_a_decided_condition_leaves_never_refuses() {
        let unreached = when(equal(param("flag"), literal(1)), released(), param("k"));
        let key = when(equal(literal(1), literal(1)), param("k"), unreached);

        let body = reading_by(key);

        assert_eq!(category_of(body.clone()), STATIC);
        assert!(reaches_the_access_after(body));
    }

    /// Slot 2 holds a row of `t1`, keyed by the text `c`; slot 1 is released.
    /// `accesses` come after both, and an `accounts` key, an integer, picks
    /// the released column on one arm and the text column on the other.
    fn picking_a_released_or_a_text_column(accesses: impl FnOnce(Expr) -> Vec<Effect>) -> Program {
        let key = when(
            equal(param("flag"), literal(1)),
            released(),
            column_of(2, "c"),
        );
        let mut effects = vec![
            read("t1", Expr::Literal(Value::Text("k".to_owned())), 2),
            low_to_high("a", 0),
            iterate(0, 1, vec![]),
        ];
        effects.extend(accesses(key));
        let mut program = program(returns(effects));
        program
            .schema
            .tables
            .push(table_keyed_by("t1", "c", ColumnType::Text));
        program
    }

    #[test]
    fn an_undecided_conditional_whose_arms_abort_for_different_causes_aborts_like_the_evaluator() {
        let straight = picking_a_released_or_a_text_column(|key| {
            vec![read("accounts", key, 3), update("b", param("y"), &[])]
        });
        let in_a_loop = picking_a_released_or_a_text_column(|key| {
            vec![
                low_to_high("a", 4),
                iterate(4, 5, vec![read("accounts", key, 3)]),
                update("b", param("y"), &[]),
            ]
        });

        let ends_at_the_read = classification_of(&straight);
        let forks_on_the_loop = classification_of(&in_a_loop);

        assert_eq!(
            ends_at_the_read,
            Classification::Proven {
                category: ProvenCategory::Static,
                evidence: Evidence::new(
                    [
                        "f/body/effects[0]",
                        "f/body/effects[1]",
                        "f/body/effects[3]"
                    ]
                    .map(str::to_owned)
                    .to_vec()
                )
                .unwrap(),
            }
        );
        let paths = evidence_paths(&forks_on_the_loop);
        assert!(matches!(forks_on_the_loop, Classification::Proven { .. }));
        assert!(paths.contains(&"f/body/effects[4]/body/effects[0]".to_owned()));
        assert!(paths.contains(&"f/body/effects[5]".to_owned()));
    }

    #[test]
    fn an_iteration_the_path_knows_is_empty_never_refuses_for_its_body() {
        let refusing = |rows, row| {
            let key = when(equal(param("flag"), literal(1)), param("k"), released());
            iterate(rows, row, vec![read("accounts", key, 6)])
        };
        let common = || {
            vec![
                low_to_high("a", 0),
                iterate(0, 1, vec![]),
                low_to_high("b", 2),
            ]
        };
        let tested_empty =
            branch_on_presence(common(), 2, returns(vec![]), returns(vec![refusing(2, 3)]));
        let mut replayed_empty = common();
        replayed_empty.extend([
            iterate(2, 3, vec![read("accounts", released(), 4)]),
            refusing(2, 5),
        ]);

        assert_eq!(category_of(tested_empty), STATIC);
        assert_eq!(category_of(returns(replayed_empty)), STATIC);
    }
}
