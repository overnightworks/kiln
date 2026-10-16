use kiln_e1::{
    classify_function, Classification, ClassifyError, IndexRead, IndexReadKind, ProvenCategory,
    Reason, UnknownCause,
};
use kiln_ir::{Block, Effect, FunctionName, IndexName, Slot};

#[path = "support/programs.rs"]
mod programs;
use programs::*;

#[path = "footprint/branches.rs"]
mod branches;

#[path = "footprint/comparisons.rs"]
mod comparisons;

#[path = "footprint/keys.rs"]
mod keys;

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
    fn accesses_through_the_rows_of_an_undeclared_index_leave_the_undeclared_index_as_the_reason() {
        let accesses = [
            read("orders", column_of(1, "customer_id"), 2),
            read("orders", key_of(1), 2),
            insert("audit", column_of(1, "customer_id")),
            update("orders", key_of(1), &[]),
        ];
        for access in accesses {
            let body = returns(vec![
                read_index(
                    "orders_by_note",
                    vec![param("note")],
                    vec![param("note")],
                    0,
                ),
                iterate(0, 1, vec![access]),
            ]);
            assert_unknown(
                body,
                UnknownCause::UndeclaredIndex(IndexName("orders_by_note".to_owned())),
            );
        }
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
            iterate(
                0,
                1,
                vec![
                    read("orders", key_of(1), 2),
                    read("customers", column_of(2, "customer_id"), 3),
                ],
            ),
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
    fn a_key_projecting_a_slot_an_ended_iteration_released_ends_the_path_before_its_access() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(0, 1, vec![read("orders", key_of(1), 2)]),
            read("customers", column_of(2, "customer_id"), 3),
            update("audit", param("id"), &[]),
        ]);
        assert_eq!(category_of(body.clone()), STATIC);
        assert_eq!(
            evidence_of(body),
            [
                "f/body/effects[0]",
                "f/body/effects[1]/body/effects[0]",
                "f/body/effects[2]"
            ]
        );
    }

    #[test]
    fn an_iteration_over_a_slot_an_ended_iteration_released_is_reported() {
        let body = returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(
                0,
                1,
                vec![read_range("accounts", param("low"), param("high"), 2)],
            ),
            iterate(2, 3, vec![]),
        ]);
        assert_eq!(
            classify_function(&program(body), &FunctionName(FUNCTION.to_owned())),
            Err(ClassifyError::UnboundSlot {
                slot: Slot(2),
                at: "f/body/effects[2]".to_owned()
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
