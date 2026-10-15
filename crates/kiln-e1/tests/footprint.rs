use kiln_e1::{
    classify_function, Classification, ClassifyError, FootprintClassification, IndexRead,
    IndexReadKind, ProvenCategory, Reason, UnknownCause,
};
use kiln_ir::{Block, FunctionName, IndexName, Slot};

#[path = "support/programs.rs"]
mod programs;
use programs::*;

fn classify(body: Block) -> FootprintClassification {
    classify_function(&program(body), &FunctionName(FUNCTION.to_owned())).unwrap()
}

fn category_of(body: Block) -> Option<ProvenCategory> {
    match classify(body).classification {
        Classification::Proven { category, .. } => Some(category),
        _ => None,
    }
}

fn evidence_of(body: Block) -> Vec<String> {
    match classify(body).classification {
        Classification::Proven { evidence, .. } => evidence.paths().to_vec(),
        Classification::Unknown { evidence, .. } => evidence.unwrap().paths().to_vec(),
        Classification::ExcludedNonDatabase(_) => unreachable!("the classifier excludes nothing"),
    }
}

fn assert_unknown(body: Block, cause: UnknownCause) {
    let Classification::Unknown { reason, .. } = classify(body).classification else {
        panic!("expected unknown because of {cause}");
    };
    assert_eq!(reason, Reason::new(cause.to_string()).unwrap());
}

const STATIC: Option<ProvenCategory> = Some(ProvenCategory::Static);
const ONE_HOP: Option<ProvenCategory> = Some(ProvenCategory::OneHop);

/// Reads an order by `order_id` into slot 0.
fn read_order() -> Effect {
    read("orders", param("order_id"), 0)
}

/// Reads the customer an order in slot 0 names through its foreign key.
fn read_customer_of_order(into: u32) -> Effect {
    read("customers", column_of(0, "customer_id"), into)
}

use kiln_ir::Effect;

mod kiln_e1_18 {
    use super::*;

    #[test]
    fn keys_computed_from_arguments_and_literals_are_static() {
        let bodies = [
            returns(vec![read("accounts", param("from_id"), 0)]),
            returns(vec![update("accounts", plus(param("id"), literal(1)), &[])]),
            returns(vec![insert("audit", param("id"))]),
            returns(vec![read_range("accounts", param("low"), param("high"), 0)]),
            returns(vec![]),
        ];
        for body in bodies {
            assert_eq!(category_of(body.clone()), STATIC, "{body:?}");
        }
    }

    #[test]
    fn evidence_names_every_access_and_the_terminal() {
        let body = returns(vec![
            read("accounts", param("from_id"), 0),
            update("accounts", param("to_id"), &[]),
        ]);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]", "f/body/terminal"]
        );
    }

    #[test]
    fn evidence_names_every_access_and_terminal_of_every_branch() {
        let arm =
            |column: &str| returns(vec![update("audit", param("id"), &[(column, literal(1))])]);
        let body = branch_on_presence(
            vec![read("accounts", param("id"), 0)],
            0,
            arm("first"),
            arm("second"),
        );
        assert_eq!(
            evidence_of(body),
            [
                "f/body/effects[0]",
                "f/body/terminal/when_present/effects[0]",
                "f/body/terminal/when_absent/effects[0]",
                "f/body/terminal/when_present/terminal",
                "f/body/terminal/when_absent/terminal",
            ]
        );
    }

    #[test]
    fn a_function_touching_nothing_is_static_with_its_terminal_as_evidence() {
        assert_eq!(evidence_of(returns(vec![])), ["f/body/terminal"]);
    }

    #[test]
    fn a_key_projected_from_a_read_is_not_static() {
        assert_eq!(
            category_of(returns(vec![read_order(), read_customer_of_order(1)])),
            ONE_HOP
        );
    }

    #[test]
    fn the_primary_key_of_a_point_read_row_is_the_key_it_was_read_by() {
        let bodies = [
            returns(vec![read_order(), update("customers", key_of(0), &[])]),
            returns(vec![
                read_order(),
                update("customers", plus(key_of(0), literal(1)), &[]),
            ]),
        ];
        for body in bodies {
            assert_eq!(category_of(body.clone()), STATIC, "{body:?}");
        }
    }
}

mod kiln_e1_19 {
    use super::*;

    fn static_arm() -> Block {
        returns(vec![update("audit", param("id"), &[])])
    }

    fn resolving_arm() -> Block {
        returns(vec![read_customer_of_order(1)])
    }

    fn unknown_arm() -> Block {
        returns(vec![read("customers", column_of(0, "note"), 1)])
    }

    #[test]
    fn a_data_selected_branch_counts_its_selector_only_when_the_arms_reach_different_targets() {
        let touching = |table: &str, key: &str, column: &str| {
            returns(vec![update(table, param(key), &[(column, literal(1))])])
        };
        let cases = [
            (
                touching("audit", "id", "a"),
                touching("audit", "id", "b"),
                STATIC,
            ),
            (touching("a", "x", "c"), touching("b", "y", "c"), ONE_HOP),
            (
                touching("audit", "x", "c"),
                touching("audit", "y", "c"),
                ONE_HOP,
            ),
            (static_arm(), returns(vec![]), ONE_HOP),
        ];
        for (present, absent, expected) in cases {
            let body =
                branch_on_presence(vec![read("accounts", param("id"), 0)], 0, present, absent);
            assert_eq!(category_of(body.clone()), expected, "{body:?}");
        }
    }

    #[test]
    fn the_selector_read_and_a_resolving_read_in_an_arm_are_two_resolutions() {
        let inner = branch_on_presence(
            vec![read("orders", param("other_order_id"), 1)],
            1,
            returns(vec![update("a", param("id"), &[])]),
            returns(vec![update("b", param("id"), &[])]),
        );
        let body = branch_on_presence(
            vec![read_order()],
            0,
            inner,
            returns(vec![update("c", param("id"), &[])]),
        );
        assert_unknown(body, UnknownCause::IndependentResolutions { count: 2 });
    }

    #[test]
    fn the_most_demanding_arm_decides_whichever_side_it_is_on() {
        let present = branch_on_presence(vec![read_order()], 0, resolving_arm(), static_arm());
        let absent = branch_on_presence(vec![read_order()], 0, static_arm(), resolving_arm());
        assert_eq!(category_of(present), ONE_HOP);
        assert_eq!(category_of(absent), ONE_HOP);
    }

    #[test]
    fn resolutions_on_different_arms_do_not_add_up() {
        let body = branch_on_presence(
            vec![read_order()],
            0,
            returns(vec![read_customer_of_order(1)]),
            returns(vec![read("products", column_of(0, "product_id"), 1)]),
        );
        assert_eq!(category_of(body), ONE_HOP);
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
                column: kiln_ir::ColumnName("first".to_owned()),
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
            terminal: kiln_ir::Terminal::Branch {
                condition: kiln_ir::Condition::RowPresent(Slot(0)),
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

mod kiln_e1_20 {
    use super::*;

    #[test]
    fn a_read_feeding_only_the_response_or_a_written_column_is_no_hop() {
        let bodies = [
            returning(vec![read_order()], column_of(0, "customer_id")),
            returns(vec![
                read_order(),
                update("audit", param("id"), &[("seen", column_of(0, "total"))]),
            ]),
        ];
        for body in bodies {
            assert_eq!(category_of(body.clone()), STATIC, "{body:?}");
        }
    }
}

mod kiln_e1_21 {
    use super::*;

    #[test]
    fn one_read_resolving_through_a_foreign_key_is_one_hop_with_both_nodes_as_evidence() {
        let body = returns(vec![read_order(), read_customer_of_order(1)]);
        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]"]
        );
    }

    #[test]
    fn evidence_also_names_the_accesses_of_the_other_branches() {
        let body = branch_on_presence(
            vec![read_order()],
            0,
            returns(vec![read_customer_of_order(1)]),
            returns(vec![update("audit", param("id"), &[])]),
        );
        assert_eq!(
            evidence_of(body),
            [
                "f/body/effects[0]",
                "f/body/terminal/when_present/effects[0]",
                "f/body/terminal/when_absent/effects[0]",
            ]
        );
    }

    #[test]
    fn one_result_may_determine_several_accesses() {
        let body = returns(vec![
            read_order(),
            read_customer_of_order(1),
            read("products", column_of(0, "product_id"), 2),
            update("customers", column_of(0, "customer_id"), &[]),
        ]);
        assert_eq!(category_of(body), ONE_HOP);
    }

    #[test]
    fn a_loop_over_the_rows_of_one_range_is_one_hop() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(
                0,
                1,
                vec![read("customers", column_of(1, "customer_id"), 2)],
            ),
        ]);
        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            ["f/body/effects[0]", "f/body/effects[1]/body/effects[0]"]
        );
    }
}

mod kiln_e1_22 {
    use super::*;

    #[test]
    fn a_result_reaching_an_access_with_no_declared_foreign_key_is_unknown() {
        let missing_key = returns(vec![
            read_order(),
            read("customers", column_of(0, "note"), 1),
        ]);
        assert_unknown(
            missing_key,
            UnknownCause::NoDeclaredForeignKey {
                from: table("orders"),
                column: kiln_ir::ColumnName("note".to_owned()),
                to: table("customers"),
            },
        );
    }

    #[test]
    fn a_foreign_key_into_another_table_does_not_identify_the_access() {
        let wrong_target = returns(vec![
            read_order(),
            read("products", column_of(0, "customer_id"), 1),
        ]);
        assert!(category_of(wrong_target).is_none());
    }

    #[test]
    fn a_primary_key_of_one_table_used_in_another_is_unknown() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(0, 1, vec![read("customers", key_of(1), 2)]),
        ]);
        assert_unknown(
            body,
            UnknownCause::KeyOfAnotherTable {
                from: table("orders"),
                to: table("customers"),
            },
        );
    }

    #[test]
    fn a_primary_key_range_bounded_by_a_read_is_unknown() {
        let body = returns(vec![
            read_order(),
            read_range("orders", column_of(0, "customer_id"), param("high"), 1),
        ]);
        assert_unknown(body, UnknownCause::PrimaryRangeBoundedByRead);
    }

    #[test]
    fn a_key_computed_from_a_read_keeps_neither_foreign_key_identity_nor_range_containment() {
        let computed_foreign_key = returns(vec![
            read_order(),
            read(
                "customers",
                plus(column_of(0, "customer_id"), literal(1)),
                1,
            ),
        ]);
        let computed_range_key = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(0, 1, vec![read("orders", plus(key_of(1), literal(1)), 2)]),
        ]);
        let cases = [
            (computed_foreign_key, "customers"),
            (computed_range_key, "orders"),
        ];
        for (body, accessed) in cases {
            assert_unknown(
                body,
                UnknownCause::KeyComputedFromRead {
                    table: table(accessed),
                },
            );
        }
    }

    #[test]
    fn unknown_evidence_names_the_offending_node() {
        let body = returns(vec![
            read_order(),
            read("customers", column_of(0, "note"), 1),
        ]);
        assert_eq!(evidence_of(body), ["f/body/effects[1]"]);
    }
}

mod kiln_e1_23 {
    use super::*;

    #[test]
    fn an_insert_key_derived_from_a_read_is_the_one_resolution_whatever_the_column() {
        let body = returns(vec![
            read("districts", param("district_id"), 0),
            insert("orders", column_of(0, "next_order_id")),
            insert(
                "order_lines",
                plus(column_of(0, "next_order_id"), literal(1)),
            ),
        ]);
        assert_eq!(category_of(body.clone()), ONE_HOP);
        assert_eq!(
            evidence_of(body),
            [
                "f/body/effects[0]",
                "f/body/effects[1]",
                "f/body/effects[2]"
            ]
        );
    }

    #[test]
    fn a_second_resolving_read_still_exceeds_one() {
        let body = returns(vec![
            read_order(),
            insert("audit", column_of(0, "total")),
            read_customer_of_order(1),
            update("customers", column_of(1, "region_id"), &[]),
        ]);
        assert!(category_of(body).is_none());
    }
}

mod kiln_e1_24 {
    use super::*;

    fn customer_orders() -> Effect {
        read_index(
            "orders_by_customer",
            vec![param("customer")],
            vec![param("customer")],
            0,
        )
    }

    #[test]
    fn an_argument_bounded_range_over_a_declared_index_is_static_and_index_only() {
        let result = classify(returns(vec![customer_orders()]));
        assert!(matches!(
            result.classification,
            Classification::Proven {
                category: ProvenCategory::Static,
                ..
            }
        ));
        assert_eq!(
            result.index_reads,
            [IndexRead {
                node: "f/body/effects[0]".to_owned(),
                kind: IndexReadKind::IndexOnly
            }]
        );
    }

    #[test]
    fn resolving_its_rows_into_base_rows_is_one_hop_followed_by_row_accesses() {
        let result = classify(returns(vec![
            customer_orders(),
            iterate(0, 1, vec![update("orders", key_of(1), &[])]),
        ]));
        assert!(matches!(
            result.classification,
            Classification::Proven {
                category: ProvenCategory::OneHop,
                ..
            }
        ));
        assert_eq!(
            result.index_reads,
            [IndexRead {
                node: "f/body/effects[0]".to_owned(),
                kind: IndexReadKind::FollowedByRowAccesses
            }]
        );
    }

    #[test]
    fn resolving_its_rows_through_a_foreign_key_is_one_hop() {
        let body = returns(vec![
            customer_orders(),
            iterate(0, 1, vec![read("products", column_of(1, "product_id"), 2)]),
        ]);
        assert_eq!(category_of(body), ONE_HOP);
    }

    #[test]
    fn a_range_over_an_undeclared_index_is_unknown() {
        let body = returns(vec![read_index(
            "orders_by_note",
            vec![param("note")],
            vec![param("note")],
            0,
        )]);
        assert_unknown(
            body,
            UnknownCause::UndeclaredIndex(IndexName("orders_by_note".to_owned())),
        );
    }

    #[test]
    fn a_primary_key_range_is_no_index_and_its_own_rows_cost_no_hop() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(0, 1, vec![update("orders", key_of(1), &[])]),
        ]);
        assert_eq!(category_of(body), STATIC);
    }

    #[test]
    fn a_range_feeding_another_ranges_bounds_is_not_followed_by_row_accesses() {
        let result = classify(returns(vec![
            read_index(
                "customers_by_tenant",
                vec![param("tenant"), literal(0)],
                vec![param("tenant"), literal(9)],
                0,
            ),
            iterate(
                0,
                1,
                vec![read_index(
                    "orders_by_customer",
                    vec![key_of(1)],
                    vec![plus(key_of(1), literal(1))],
                    2,
                )],
            ),
        ]));
        assert_eq!(
            result.index_reads,
            [
                IndexRead {
                    node: "f/body/effects[0]".to_owned(),
                    kind: IndexReadKind::FeedsIndexBounds
                },
                IndexRead {
                    node: "f/body/effects[1]/body/effects[0]".to_owned(),
                    kind: IndexReadKind::IndexOnly
                }
            ]
        );
    }

    #[test]
    fn index_reads_cover_every_branch_and_a_selecting_range_stays_index_only() {
        let rows_of_range = returns(vec![iterate(0, 1, vec![update("orders", key_of(1), &[])])]);
        let other_range = read_index(
            "orders_by_customer",
            vec![param("other")],
            vec![param("other")],
            2,
        );
        let followed = branch_on_presence(
            vec![customer_orders()],
            0,
            rows_of_range,
            returns(vec![other_range.clone()]),
        );
        let selecting = branch_on_presence(
            vec![customer_orders()],
            0,
            returns(vec![update("audit", param("id"), &[])]),
            returns(vec![]),
        );
        let reads = |body| classify(body).index_reads;
        assert_eq!(
            reads(followed),
            [
                IndexRead {
                    node: "f/body/effects[0]".to_owned(),
                    kind: IndexReadKind::FollowedByRowAccesses
                },
                IndexRead {
                    node: "f/body/terminal/when_absent/effects[0]".to_owned(),
                    kind: IndexReadKind::IndexOnly
                }
            ]
        );
        assert_eq!(
            reads(selecting),
            [IndexRead {
                node: "f/body/effects[0]".to_owned(),
                kind: IndexReadKind::IndexOnly
            }]
        );
    }

    #[test]
    fn a_range_bound_by_a_read_result_is_a_resolution() {
        let body = returns(vec![
            read_order(),
            read_index(
                "orders_by_customer",
                vec![column_of(0, "customer_id")],
                vec![column_of(0, "customer_id")],
                1,
            ),
        ]);
        assert_eq!(category_of(body), ONE_HOP);
    }
}

mod kiln_e1_25 {
    use super::*;

    #[test]
    fn independent_resolving_reads_exceed_one_hop() {
        let body = returns(vec![
            read_order(),
            read("orders", param("other_order_id"), 1),
            read("customers", column_of(0, "customer_id"), 2),
            read("products", column_of(1, "product_id"), 3),
        ]);
        assert_unknown(body, UnknownCause::IndependentResolutions { count: 2 });
    }

    #[test]
    fn a_read_depending_only_on_a_covered_range_is_no_chain() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(0, 1, vec![read("orders", key_of(1), 2)]),
            read("customers", column_of(2, "customer_id"), 3),
            read("orders", param("other_order_id"), 4),
            read("products", column_of(4, "product_id"), 5),
        ]);
        assert_unknown(body, UnknownCause::IndependentResolutions { count: 2 });
    }

    #[test]
    fn a_chain_of_resolving_reads_is_named_a_chain() {
        let body = returns(vec![
            read_order(),
            read_customer_of_order(1),
            read("regions", column_of(1, "region_id"), 2),
        ]);
        assert_unknown(body, UnknownCause::ChainedResolutions { count: 2 });
    }

    #[test]
    fn chain_evidence_names_each_resolving_read_and_the_access_it_resolves() {
        let body = returns(vec![
            read_order(),
            read_customer_of_order(1),
            read("regions", column_of(1, "region_id"), 2),
        ]);
        let result = classify(body);
        let Classification::Unknown { evidence, .. } = result.classification else {
            panic!("a two-step chain is not one-hop");
        };
        assert_eq!(
            evidence.unwrap().paths(),
            [
                "f/body/effects[0]",
                "f/body/effects[1]",
                "f/body/effects[2]"
            ]
        );
    }
}

mod kiln_e1_26 {
    use super::*;

    fn within_one_tenant() -> Block {
        returns(vec![
            read_index(
                "customers_by_tenant",
                vec![param("tenant"), literal(0)],
                vec![param("tenant"), literal(9)],
                0,
            ),
            iterate(
                0,
                1,
                vec![
                    read_index(
                        "orders_by_tenant",
                        vec![param("tenant"), key_of(1)],
                        vec![param("tenant"), key_of(1)],
                        2,
                    ),
                    iterate(2, 3, vec![update("orders", key_of(3), &[])]),
                ],
            ),
        ])
    }

    fn through_foreign_keys() -> Block {
        returns(vec![
            read_order(),
            read_customer_of_order(1),
            read("regions", column_of(1, "region_id"), 2),
        ])
    }

    #[test]
    fn a_chain_is_unknown_for_want_of_a_declared_partition_however_its_arguments_look() {
        for body in [within_one_tenant(), through_foreign_keys()] {
            let Classification::Unknown { reason, .. } = classify(body.clone()).classification
            else {
                panic!("a chain is never labelled without a declared partition: {body:?}");
            };
            let cause = UnknownCause::ChainedResolutions { count: 2 };
            assert_eq!(reason, Reason::new(cause.to_string()).unwrap());
            assert!(cause.to_string().contains("no declared partition"));
        }
    }
}

mod kiln_e1_27 {
    use super::*;

    #[test]
    fn a_function_the_program_does_not_declare_is_reported() {
        let missing = FunctionName("g".to_owned());
        assert_eq!(
            classify_function(&program(returns(vec![])), &missing),
            Err(ClassifyError::UnknownFunction(missing))
        );
    }

    #[test]
    fn a_read_of_a_slot_no_earlier_read_bound_is_reported() {
        let body = returns(vec![read("customers", column_of(4, "customer_id"), 0)]);
        assert_eq!(
            classify_function(&program(body), &FunctionName(FUNCTION.to_owned())),
            Err(ClassifyError::UnboundSlot {
                slot: Slot(4),
                at: "f/body/effects[0]".to_owned()
            })
        );
    }

    #[test]
    fn the_report_of_an_error_names_what_is_wrong() {
        let message = ClassifyError::UnknownFunction(FunctionName("g".to_owned())).to_string();
        assert!(message.contains('g'));
        let unbound = ClassifyError::UnboundSlot {
            slot: Slot(4),
            at: "f/body/effects[0]".to_owned(),
        };
        assert!(unbound.to_string().contains("f/body/effects[0]"));
    }

    #[test]
    fn every_label_carries_evidence_and_every_unknown_its_reason() {
        let bodies = [
            returns(vec![]),
            returns(vec![read_order(), read_customer_of_order(1)]),
            returns(vec![
                read_order(),
                read("customers", column_of(0, "note"), 1),
            ]),
        ];
        for body in bodies {
            match classify(body).classification {
                Classification::Proven { evidence, .. } => assert!(!evidence.paths().is_empty()),
                Classification::Unknown { evidence, .. } => assert!(evidence.is_some()),
                Classification::ExcludedNonDatabase(_) => unreachable!(),
            }
        }
    }
}
