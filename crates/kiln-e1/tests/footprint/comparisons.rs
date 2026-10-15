use kiln_e1::UnknownCause;
use kiln_ir::Block;

use super::programs::*;

const BRANCH: &str = "f/body/terminal";

fn by_customer_id() -> Block {
    returns(vec![read("customers", param("customer_id"), 1)])
}

fn by_order_id() -> Block {
    returns(vec![read("orders", param("order_id"), 1)])
}

fn by_argument(present: Block, absent: Block) -> Block {
    branch_on_equality(vec![], (param("kind"), literal(0)), present, absent)
}

fn selected_by_branch() -> UnknownCause {
    UnknownCause::DataSelectedTargets {
        selector: BRANCH.to_owned(),
    }
}

mod kiln_e1_28 {
    use super::*;

    #[test]
    fn an_argument_selected_branch_between_two_argument_keyed_reads_is_static() {
        assert_eq!(
            category_of(by_argument(by_customer_id(), by_order_id())),
            STATIC
        );
    }

    #[test]
    fn a_comparison_of_literals_and_of_a_point_reads_own_key_is_argument_selected() {
        let by_key_of_read = branch_on_equality(
            vec![read("orders", param("order_id"), 0)],
            (key_of(0), literal(0)),
            by_customer_id(),
            by_order_id(),
        );
        let by_literals = branch_on_equality(
            vec![],
            (literal(1), literal(2)),
            by_customer_id(),
            by_order_id(),
        );
        assert_eq!(category_of(by_key_of_read), STATIC);
        assert_eq!(category_of(by_literals), STATIC);
    }

    #[test]
    fn a_read_before_an_argument_selected_branch_does_not_make_it_data_selected() {
        let body = branch_on_equality(
            vec![read_order()],
            (param("kind"), literal(0)),
            returns(vec![update("audit", param("a"), &[])]),
            returns(vec![update("log", param("b"), &[])]),
        );
        assert_eq!(category_of(body), STATIC);
    }

    #[test]
    fn an_argument_selected_branch_composes_by_its_arms() {
        let resolving = returns(vec![read_order(), read_customer_of_order(1)]);
        let body = by_argument(by_customer_id(), resolving);
        assert_eq!(category_of(body), ONE_HOP);
        let unknown_arm = returns(vec![
            read_order(),
            read("customers", column_of(0, "note"), 1),
        ]);
        assert_eq!(
            category_of(by_argument(by_customer_id(), unknown_arm)),
            None
        );
    }

    #[test]
    fn presence_tests_inside_argument_selected_arms_are_not_compared_across_arms() {
        let presence_in = |table_name: &str, key: &str| {
            branch_on_presence(
                vec![],
                0,
                returns(vec![read(table_name, param(key), 2)]),
                returns(vec![read(table_name, param(key), 2)]),
            )
        };
        let body = branch_on_equality(
            vec![read("orders", param("id"), 0)],
            (param("kind"), literal(0)),
            presence_in("customers", "a"),
            presence_in("orders", "b"),
        );
        assert_eq!(category_of(body), STATIC);
    }

    #[test]
    fn a_presence_test_within_one_argument_arm_still_selects_targets() {
        let presence_selecting = branch_on_presence(vec![], 0, by_customer_id(), by_order_id());
        let body = branch_on_equality(
            vec![read("orders", param("id"), 0)],
            (param("kind"), literal(0)),
            presence_selecting,
            by_order_id(),
        );
        assert_unknown(
            body,
            UnknownCause::DataSelectedTargets {
                selector: "f/body/effects[0]".to_owned(),
            },
        );
    }

    #[test]
    fn nested_argument_selected_branches_classify_each_path() {
        let inner = by_argument(by_customer_id(), by_order_id());
        let body = branch_on_equality(
            vec![],
            (param("tenant"), literal(1)),
            inner,
            returns(vec![]),
        );
        assert_eq!(category_of(body), STATIC);
    }
}

mod kiln_e1_29 {
    use super::*;

    fn on_status(present: Block, absent: Block) -> Block {
        branch_on_equality(
            vec![read_order()],
            (column_of(0, "status"), literal(1)),
            present,
            absent,
        )
    }

    #[test]
    fn a_branch_on_a_projected_read_value_with_differing_targets_is_unknown() {
        assert_unknown(
            on_status(by_customer_id(), by_order_id()),
            selected_by_branch(),
        );
    }

    #[test]
    fn the_selector_is_the_branch_node_and_is_evidence() {
        let body = on_status(by_customer_id(), by_order_id());
        assert!(evidence_of(body).contains(&BRANCH.to_owned()));
    }

    #[test]
    fn a_comparison_mixing_an_argument_and_a_read_projection_is_data_selected() {
        let body = branch_on_equality(
            vec![read_order()],
            (column_of(0, "status"), param("wanted")),
            by_customer_id(),
            by_order_id(),
        );
        assert_unknown(body, selected_by_branch());
    }

    #[test]
    fn arithmetic_over_a_projection_is_data_selected() {
        let sum = plus(column_of(0, "status"), literal(1));
        let body = branch_on_equality(
            vec![read_order()],
            (sum, literal(2)),
            by_customer_id(),
            by_order_id(),
        );
        assert_unknown(body, selected_by_branch());
    }

    #[test]
    fn arms_reaching_the_same_targets_leave_a_data_selected_function_static() {
        let body = on_status(by_customer_id(), by_customer_id());
        assert_eq!(category_of(body), STATIC);
    }
}
