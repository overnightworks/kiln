use kiln_e1::UnknownCause;
use kiln_ir::{Block, ColumnName, Condition, Effect, Slot, Terminal};

use super::programs::*;

mod kiln_e1_19 {
    use super::*;

    const FIRST_READ: &str = "f/body/effects[0]";
    const SECOND_READ: &str = "f/body/effects[1]";

    fn static_arm() -> Block {
        returns(vec![update("audit", param("id"), &[])])
    }

    fn unknown_arm() -> Block {
        returns(vec![read("customers", column_of(0, "note"), 1)])
    }

    fn writing(table: &str, key: &str) -> Block {
        returns(vec![update(table, param(key), &[("c", literal(1))])])
    }

    fn selected_by(selector: &str) -> UnknownCause {
        UnknownCause::DataSelectedTargets {
            selector: selector.to_owned(),
        }
    }

    fn read_accounts(key: &str, into: u32) -> Effect {
        read("accounts", param(key), into)
    }

    #[test]
    fn a_branch_whose_arms_reach_different_targets_is_unknown_and_names_its_selector() {
        let resolving_arm = returns(vec![read_customer_of_order(1)]);
        let cases = [
            (writing("a", "x"), writing("b", "y")),
            (writing("audit", "x"), writing("audit", "y")),
            (static_arm(), returns(vec![])),
            (resolving_arm, static_arm()),
        ];
        for (present, absent) in cases {
            let body = branch_on_presence(vec![read_order()], 0, present, absent);
            assert_unknown(body.clone(), selected_by(FIRST_READ));
            assert!(evidence_of(body).contains(&FIRST_READ.to_owned()));
        }
    }

    #[test]
    fn a_primary_key_range_presence_selecting_unrelated_writes_is_unknown() {
        let body = branch_on_presence(
            vec![read_range("orders", param("from"), param("to"), 0)],
            0,
            writing("a", "x"),
            writing("b", "y"),
        );
        assert_unknown(body, selected_by(FIRST_READ));
    }

    #[test]
    fn arms_that_reach_the_same_targets_leave_a_static_function_static() {
        let body = branch_on_presence(
            vec![read("accounts", param("id"), 0)],
            0,
            returns(vec![update("audit", param("id"), &[("a", literal(1))])]),
            returns(vec![update("audit", param("id"), &[("b", literal(2))])]),
        );
        assert_eq!(category_of(body), STATIC);
    }

    #[test]
    fn arms_that_resolve_the_same_read_the_same_way_are_one_hop() {
        let resolving = || returns(vec![read_customer_of_order(1)]);
        let body = branch_on_presence(vec![read_order()], 0, resolving(), resolving());
        assert_eq!(category_of(body), ONE_HOP);
    }

    #[test]
    fn independent_selectors_whose_arms_swap_targets_are_unknown() {
        let on_second = |present, absent| branch_on_presence(vec![], 1, present, absent);
        let body = branch_on_presence(
            vec![read_accounts("r", 0), read_accounts("s", 1)],
            0,
            on_second(writing("a", "x"), writing("b", "y")),
            on_second(writing("b", "y"), writing("a", "x")),
        );
        assert_unknown(body, selected_by(SECOND_READ));
    }

    #[test]
    fn a_selector_that_changes_the_targets_only_beside_another_selector_is_named() {
        let on_second = |present, absent| branch_on_presence(vec![], 1, present, absent);
        let body = branch_on_presence(
            vec![read_accounts("r", 0), read_accounts("s", 1)],
            0,
            on_second(writing("a", "x"), writing("a", "x")),
            on_second(writing("b", "y"), writing("b", "y")),
        );
        assert_unknown(body, selected_by(FIRST_READ));
    }

    #[test]
    fn a_selector_that_changes_nothing_next_to_one_that_does_is_not_named() {
        let on_second = |present, absent| branch_on_presence(vec![], 1, present, absent);
        let body = branch_on_presence(
            vec![read_accounts("r", 0), read_accounts("s", 1)],
            0,
            on_second(writing("a", "x"), writing("b", "y")),
            on_second(writing("a", "x"), writing("b", "y")),
        );
        assert_unknown(body, selected_by(SECOND_READ));
    }

    #[test]
    fn a_resolution_before_the_branch_adds_to_one_in_an_arm() {
        let body = branch_on_presence(
            vec![read_order(), read_customer_of_order(1)],
            1,
            returns(vec![read("regions", column_of(1, "region_id"), 2)]),
            static_arm(),
        );
        assert_unknown(body, UnknownCause::ChainedResolutions { count: 2 });
    }

    #[test]
    fn the_most_demanding_path_decides_whichever_side_it_is_on() {
        let rows_of_range = |effect| returns(vec![iterate(0, 1, vec![effect])]);
        let inserting = || rows_of_range(insert("orders", key_of(1)));
        let updating = || rows_of_range(update("orders", key_of(1), &[]));
        let range = || vec![read_range("orders", param("from"), param("to"), 0)];
        let inserting_first = branch_on_presence(range(), 0, inserting(), updating());
        let inserting_last = branch_on_presence(range(), 0, updating(), inserting());
        assert_eq!(category_of(inserting_first), ONE_HOP);
        assert_eq!(category_of(inserting_last), ONE_HOP);
    }

    #[test]
    fn an_unknown_arm_makes_the_function_unknown() {
        let body = branch_on_presence(vec![read_order()], 0, static_arm(), unknown_arm());
        assert_eq!(category_of(body), None);
    }

    #[test]
    fn of_equally_demanding_arms_the_first_names_the_cause() {
        let arm = |column: &str| returns(vec![read("customers", column_of(0, column), 1)]);
        let body = branch_on_presence(vec![read_order()], 0, arm("first"), arm("second"));
        assert_unknown(
            body,
            UnknownCause::NoDeclaredForeignKey {
                from: table("orders"),
                column: ColumnName("first".to_owned()),
                to: table("customers"),
            },
        );
    }

    #[test]
    fn a_continuation_contradicting_an_established_presence_is_not_feasible() {
        let present_then_absent = branch_on_presence(
            vec![read_order()],
            0,
            branch_on_presence(vec![], 0, static_arm(), unknown_arm()),
            static_arm(),
        );
        let absent_then_present = branch_on_presence(
            vec![read_order()],
            0,
            static_arm(),
            branch_on_presence(vec![], 0, unknown_arm(), static_arm()),
        );
        assert_eq!(category_of(present_then_absent), STATIC);
        assert_eq!(category_of(absent_then_present), STATIC);
    }

    #[test]
    fn a_slot_read_again_inside_an_arm_is_tested_afresh() {
        let retested = Block {
            effects: vec![read_order()],
            terminal: Terminal::Branch {
                condition: Condition::RowPresent(Slot(0)),
                when_present: Box::new(branch_on_presence(
                    vec![read("orders", param("other_order_id"), 0)],
                    0,
                    static_arm(),
                    unknown_arm(),
                )),
                when_absent: Box::new(static_arm()),
            },
        };
        assert_eq!(category_of(retested), None);
    }
}
