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
const DOMAIN_BOUNDED: Option<ProvenCategory> = Some(ProvenCategory::DomainBounded);

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
}

mod kiln_e1_19 {
    use super::*;

    fn static_arm() -> Block {
        returns(vec![update("audit", param("id"), &[])])
    }

    fn resolving_arm() -> Block {
        returns(vec![read_customer_of_order(1)])
    }

    #[test]
    fn a_data_selected_branch_with_static_arms_is_static() {
        let body = branch_on_presence(
            vec![read("accounts", param("id"), 0)],
            0,
            static_arm(),
            returns(vec![insert("accounts", param("id"))]),
        );
        assert_eq!(category_of(body), STATIC);
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
        let body = branch_on_presence(
            vec![read_order()],
            0,
            static_arm(),
            returns(vec![read("customers", column_of(0, "note"), 1)]),
        );
        assert_eq!(category_of(body), None);
    }

    #[test]
    fn a_read_that_only_decides_the_branch_is_no_resolution() {
        let body = branch_on_presence(vec![read_order()], 0, static_arm(), static_arm());
        assert_eq!(category_of(body), STATIC);
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

    fn tenant_customers() -> Effect {
        read_index(
            "customers_by_tenant",
            vec![param("tenant"), literal(0)],
            vec![param("tenant"), literal(9)],
            0,
        )
    }

    fn orders_of_customer_in(partition: &str) -> Effect {
        read_index(
            "orders_by_tenant",
            vec![param(partition), key_of(1)],
            vec![param(partition), key_of(1)],
            2,
        )
    }

    fn chain_within(partition: &str) -> Block {
        returns(vec![
            tenant_customers(),
            iterate(
                0,
                1,
                vec![
                    orders_of_customer_in(partition),
                    iterate(2, 3, vec![update("orders", key_of(3), &[])]),
                ],
            ),
        ])
    }

    #[test]
    fn a_chain_confined_to_one_argument_selected_partition_is_domain_bounded() {
        let body = chain_within("tenant");
        assert_eq!(category_of(body.clone()), DOMAIN_BOUNDED);
        assert_eq!(
            evidence_of(body),
            [
                "f/body/effects[0]",
                "f/body/effects[1]/body/effects[0]",
                "f/body/effects[1]/body/effects[1]/body/effects[0]"
            ]
        );
    }

    #[test]
    fn a_chain_whose_second_range_leaves_the_partition_is_unknown() {
        assert_unknown(
            chain_within("other_tenant"),
            UnknownCause::ChainedResolutions { count: 2 },
        );
    }

    #[test]
    fn a_chain_leaving_the_partition_through_foreign_keys_is_unknown() {
        let body = returns(vec![
            read_index(
                "orders_by_tenant",
                vec![param("tenant"), literal(0)],
                vec![param("tenant"), literal(9)],
                0,
            ),
            iterate(
                0,
                1,
                vec![
                    read("customers", column_of(1, "customer_id"), 2),
                    read("regions", column_of(2, "region_id"), 3),
                ],
            ),
        ]);
        assert_unknown(body, UnknownCause::ChainedResolutions { count: 2 });
    }

    #[test]
    fn an_access_outside_the_partition_makes_the_chain_unknown() {
        let mut effects = vec![read("items", param("item_id"), 9)];
        effects.extend(chain_within("tenant").effects);
        assert!(category_of(returns(effects)).is_none());
    }

    #[test]
    fn a_partition_pinned_by_a_literal_is_not_argument_selected() {
        let body = returns(vec![
            read_index(
                "customers_by_tenant",
                vec![literal(7), literal(0)],
                vec![literal(7), literal(9)],
                0,
            ),
            iterate(
                0,
                1,
                vec![
                    read_index(
                        "orders_by_tenant",
                        vec![literal(7), key_of(1)],
                        vec![literal(7), key_of(1)],
                        2,
                    ),
                    iterate(2, 3, vec![update("orders", key_of(3), &[])]),
                ],
            ),
        ]);
        assert!(category_of(body).is_none());
    }

    #[test]
    fn a_partition_root_keyed_by_the_partition_argument_stays_confined() {
        let mut effects = vec![read("tenants", param("tenant"), 9)];
        effects.extend(chain_within("tenant").effects);
        assert_eq!(category_of(returns(effects)), DOMAIN_BOUNDED);
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
