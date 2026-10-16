use kiln_e1::{Classification, ProvenCategory, Reason, UnknownCause};
use kiln_ir::{Block, Effect, Expr, Slot};

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

    fn label_of(body: Block) -> Result<ProvenCategory, Reason> {
        match classify(body).classification {
            Classification::Proven { category, .. } => Ok(category),
            Classification::Unknown { reason, .. } => Err(reason),
            Classification::ExcludedNonDatabase(_) => {
                unreachable!("the classifier excludes nothing")
            }
        }
    }

    fn unknown_because(cause: UnknownCause) -> Result<ProvenCategory, Reason> {
        Err(Reason::new(cause.to_string()).unwrap())
    }

    fn is_refused_at(body: Block, slot: u32, node: &str) -> bool {
        let cause = UnknownCause::SlotCarriedAcrossIterations { slot: Slot(slot) };
        label_of(body.clone()) == unknown_because(cause) && evidence_of(body) == [node]
    }

    fn assert_refused_at(body: Block, slot: u32, node: &str) {
        assert!(is_refused_at(body.clone(), slot, node), "{body:?}");
    }

    /// What a path keeps of the place where the evaluator aborts: an insert or
    /// update keeps its access, which the evaluator logs before it resolves
    /// the written columns.
    #[derive(Clone, Copy)]
    enum Keeps {
        Nothing,
        ItsAccess,
    }

    /// Every place an effect reads a slot through `projection`, named, with
    /// what a path aborting there keeps of it.
    fn effects_reading(projection: &Expr) -> Vec<(&'static str, Effect, Keeps)> {
        let p = || projection.clone();
        let ten = || literal(10);
        let range =
            |start, end, bound| read_range_bounded("a", vec![], Some((start, end)), bound, 5);
        let index = |start, end, bound| {
            read_index_bounded("orders_by_customer", vec![start], vec![end], bound, 5)
        };
        let column = [("c", p())];
        let read_before_logging = [
            ("point-read key", read("a", p(), 5)),
            ("insert key", insert("a", p())),
            ("update key", update("a", p(), &[])),
            ("delete key", delete("a", p())),
            ("range prefix", read_prefix_range(LINES, vec![p()], None, 5)),
            ("interval start", range(p(), param("high"), ten())),
            ("interval end", range(param("low"), p(), ten())),
            ("range bound", range(param("low"), param("high"), p())),
            ("index start", index(p(), param("high"), ten())),
            ("index end", index(param("low"), p(), ten())),
            ("index bound", index(param("low"), param("high"), p())),
        ];
        let read_after_logging = [
            ("insert's column", insert_writing("a", param("x"), &column)),
            ("update's column", update("a", param("x"), &column)),
        ];
        let tagged = |keeps| move |(place, site)| (place, site, keeps);
        read_before_logging
            .map(tagged(Keeps::Nothing))
            .into_iter()
            .chain(read_after_logging.map(tagged(Keeps::ItsAccess)))
            .collect()
    }

    /// Slot 1 is the row of an iteration that has ended, so it holds no row.
    fn released() -> Expr {
        key_of(1)
    }

    const PRESENT_ARM: &str = "f/body/terminal/when_present";

    fn write_b() -> Effect {
        update("b", param("y"), &[])
    }

    /// Takes `present` or an arm that writes `b` on the presence of a read
    /// made after slot 1 was released.
    fn after_a_released_row(present: Block) -> Block {
        branch_on_presence(
            vec![
                low_to_high("orders", 0),
                iterate(0, 1, vec![]),
                read("accounts", param("s"), 2),
            ],
            2,
            present,
            returns(vec![write_b()]),
        )
    }

    /// Whether the present arm ends at `site` with nothing after it in the
    /// evidence, and the function is what that truncated arm makes it: when
    /// the arm keeps the aborting access, the read selects which table is
    /// written; when it keeps nothing, only whether `b` is.
    fn ends_at(present: Block, site: &str, keeps: Keeps) -> bool {
        let body = after_a_released_row(present);
        let in_present_arm: Vec<String> = evidence_of(body.clone())
            .into_iter()
            .filter(|node| node.starts_with(PRESENT_ARM))
            .collect();
        let truncated = match keeps {
            Keeps::ItsAccess => unknown_because(UnknownCause::DataSelectedTargets {
                selector: "f/body/effects[2]".to_owned(),
            }),
            Keeps::Nothing => Ok(ProvenCategory::Static),
        };
        in_present_arm == [site] && label_of(body) == truncated
    }

    #[test]
    fn every_place_the_evaluator_reads_a_released_slot_ends_the_path_in_its_logging_order() {
        let effect = format!("{PRESENT_ARM}/effects[0]");
        let terminal = format!("{PRESENT_ARM}/terminal");
        let comparing = |left, right| {
            let arm = returns(vec![write_b()]);
            branch_on_equality(vec![], (left, right), arm.clone(), arm)
        };
        let terminals = [
            ("comparison's left", comparing(released(), param("v"))),
            ("comparison's right", comparing(param("v"), released())),
            ("returned value", returning(vec![], released())),
        ];
        let effects = effects_reading(&released())
            .into_iter()
            .map(|(place, site, keeps)| (place, returns(vec![site, write_b()]), &effect, keeps));
        let rows = effects
            .chain(terminals.map(|(place, present)| (place, present, &terminal, Keeps::Nothing)));
        let failing: Vec<&str> = rows
            .filter(|(_, present, site, keeps)| !ends_at(present.clone(), site, *keeps))
            .map(|(place, ..)| place)
            .collect();
        assert!(failing.is_empty(), "{failing:?}");
    }

    #[test]
    fn a_projection_finds_no_row_in_a_range_slot_or_in_a_point_read_found_absent() {
        // Where the projection of slot 2 finds no row, the evaluator logs the
        // write of `audit` and aborts, so the read selects which table is
        // written; where it finds one, the arm writes `b` too.
        let writing_audit_from_2 = || {
            returns(vec![
                update("audit", param("x"), &[("c", key_of(2))]),
                write_b(),
            ])
        };
        let writing_b = || returns(vec![write_b()]);
        let account = || vec![read("accounts", param("s"), 2)];
        let range = vec![low_to_high("orders", 2)];
        let selected = unknown_because(UnknownCause::DataSelectedTargets {
            selector: "f/body/effects[0]".to_owned(),
        });
        let rows = [
            (
                "a range read's slot",
                branch_on_presence(range, 2, writing_audit_from_2(), writing_b()),
                selected.clone(),
            ),
            (
                "a point read found absent",
                branch_on_presence(account(), 2, writing_b(), writing_audit_from_2()),
                selected,
            ),
            (
                "a point read found present",
                branch_on_presence(account(), 2, writing_audit_from_2(), writing_b()),
                Ok(ProvenCategory::Static),
            ),
        ];
        let failing: Vec<&str> = rows
            .into_iter()
            .filter(|(_, body, truncated)| label_of(body.clone()) != *truncated)
            .map(|(source, ..)| source)
            .collect();
        assert!(failing.is_empty(), "{failing:?}");
    }

    #[test]
    fn a_write_aborting_at_its_written_column_in_an_iteration_body_forks_on_the_range() {
        // On the first iteration the evaluator logs the write of `audit`, then
        // aborts at its column: `b` is written only where the range is empty.
        let after_the_loop = |table: &str| {
            returns(vec![
                low_to_high("orders", 0),
                iterate(0, 1, vec![]),
                low_to_high("accounts", 3),
                iterate(
                    3,
                    4,
                    vec![update("audit", param("y"), &[("c", released())]), write_b()],
                ),
                update(table, param("y"), &[]),
            ])
        };
        let selected = UnknownCause::DataSelectedTargets {
            selector: "f/body/effects[2]".to_owned(),
        };
        assert_eq!(label_of(after_the_loop("b")), unknown_because(selected));
        assert_eq!(
            label_of(after_the_loop("audit")),
            Ok(ProvenCategory::Static)
        );
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
    fn every_place_a_body_reads_a_slot_it_binds_later_is_a_use_that_is_refused() {
        // A body holds effects only, so these are all the places it reads a
        // slot; each body binds slot 0 again after reading it.
        let point = read("accounts", param("k"), 0);
        let range = low_to_high("log", 0);
        let projections = effects_reading(&key_of(0))
            .into_iter()
            .map(|(place, site, _)| (place, &point, site));
        let source = ("inner iteration's source", &range, iterate(0, 3, vec![]));
        let failing: Vec<&str> = projections
            .chain([source])
            .filter(|(_, binding, site)| {
                let binding = (*binding).clone();
                let body = over_ledger(vec![binding.clone()], vec![site.clone(), binding]);
                !is_refused_at(body, 0, "f/body/effects[2]/body/effects[0]")
            })
            .map(|(place, ..)| place)
            .collect();
        assert!(failing.is_empty(), "{failing:?}");
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
