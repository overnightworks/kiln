use kiln_e1::UnknownCause;
use kiln_ir::{Block, Effect, Slot};

use super::programs::*;

mod kiln_e1_27 {
    use super::*;

    fn low_to_high(table: &str, into: u32) -> Effect {
        read_range(table, param("low"), param("high"), into)
    }

    fn over_ledger(effects: Vec<Effect>, loop_body: Vec<Effect>) -> Block {
        let mut body = effects;
        body.push(low_to_high("ledger", 1));
        body.push(iterate(1, 2, loop_body));
        returns(body)
    }

    fn assert_refused_at(body: Block, slot: u32, node: &str) {
        let cause = UnknownCause::SlotCarriedAcrossIterations { slot: Slot(slot) };
        assert_unknown(body.clone(), cause);
        assert_eq!(evidence_of(body), [node]);
    }

    #[test]
    fn an_inner_iteration_releasing_a_slot_its_outer_body_projected_before_is_refused() {
        // A second outer iteration aborts at the read of `a`, because the inner
        // iteration took slot 0 as its row and released it; `b` is then never
        // reached, so the present arm's targets do not include the absent arm's.
        let present = returns(vec![
            iterate(
                1,
                3,
                vec![
                    read("a", key_of(0), 4),
                    low_to_high("log", 5),
                    iterate(5, 0, vec![]),
                ],
            ),
            read("b", param("k"), 6),
            read("shared", key_of(0), 7),
        ]);
        let body = branch_on_presence(
            vec![
                read("accounts", param("k"), 0),
                low_to_high("ledger", 1),
                read("districts", param("s"), 2),
            ],
            2,
            present,
            returns(vec![read("b", param("k"), 6)]),
        );
        assert_refused_at(
            body,
            0,
            "f/body/terminal/when_present/effects[0]/body/effects[0]",
        );
    }

    #[test]
    fn a_read_rebinding_a_slot_its_body_projected_before_is_refused() {
        // A second iteration reads the customer of the order `p` names, a
        // second resolving read that one walk of the body does not see.
        let body = over_ledger(
            vec![read("orders", param("o"), 0)],
            vec![read_customer_of_order(3), read("orders", param("p"), 0)],
        );
        assert_refused_at(body, 0, "f/body/effects[2]/body/effects[0]");
    }

    #[test]
    fn a_written_column_or_an_inner_iteration_using_a_slot_the_body_binds_later_is_refused() {
        let written_column = over_ledger(
            vec![read("accounts", param("k"), 0)],
            vec![
                update("audit", param("id"), &[("c", key_of(0))]),
                read("accounts", param("m"), 0),
            ],
        );
        let inner_iteration = over_ledger(
            vec![low_to_high("log", 0)],
            vec![
                iterate(0, 3, vec![update("log", key_of(3), &[])]),
                read_range("log", param("from"), param("to"), 0),
            ],
        );
        for body in [written_column, inner_iteration] {
            assert_refused_at(body, 0, "f/body/effects[2]/body/effects[0]");
        }
    }

    #[test]
    fn a_slot_every_iteration_binds_or_releases_before_using_it_is_not_refused() {
        let read_before_projecting = over_ledger(
            vec![read("orders", param("o"), 0)],
            vec![read("orders", param("p"), 0), read_customer_of_order(3)],
        );
        let projected_after_an_inner_iteration_released_it = over_ledger(
            vec![],
            vec![
                low_to_high("log", 3),
                iterate(3, 4, vec![]),
                read("a", key_of(4), 5),
            ],
        );
        let own_row_read_again = returns(vec![
            low_to_high("a", 0),
            iterate(
                0,
                1,
                vec![update("a", key_of(1), &[]), read("b", param("k"), 1)],
            ),
        ]);
        let cases = [
            (read_before_projecting, ONE_HOP),
            (projected_after_an_inner_iteration_released_it, STATIC),
            (own_row_read_again, STATIC),
        ];
        for (body, category) in cases {
            assert_eq!(category_of(body.clone()), category, "{body:?}");
        }
    }
}
