use kiln_e1::{
    render_units, tpcc_schema, tpcc_units, LoweringRefusal, Profile, Requirement, Subject,
    TpccTable, TpccTransaction,
};
use kiln_ir::{ColumnName, ColumnType, IntegerWidth, TableName};
use TpccTable::{Customer, District, History, Item, NewOrder, Order, OrderLine, Stock, Warehouse};
use TpccTransaction::{Delivery, OrderStatus, Payment, StockLevel};

const CUSTOMER_BY_LAST_NAME: Requirement = Requirement::LookupByAttributes {
    table: Customer,
    columns: &["C_W_ID", "C_D_ID", "C_LAST"],
};
const ORDER_OF_CUSTOMER: Requirement = Requirement::LookupByAttributes {
    table: Order,
    columns: &["O_W_ID", "O_D_ID", "O_C_ID"],
};

fn refusal_of(profile: &Profile) -> LoweringRefusal {
    LoweringRefusal::of(profile).expect("the profile has a step the IR cannot express")
}

fn blocking_requirements(profile: &Profile) -> Vec<Requirement> {
    let mut requirements: Vec<Requirement> = Vec::new();
    for blocker in refusal_of(profile).blockers() {
        if !requirements.contains(&blocker.requirement) {
            requirements.push(blocker.requirement);
        }
    }
    requirements
}

fn tables_touched(profile: &Profile) -> Vec<TpccTable> {
    let mut touched: Vec<TpccTable> = Vec::new();
    for requirement in profile.steps.iter().flat_map(|step| step.requires) {
        let (Requirement::KeyedAccess(table)
        | Requirement::KeyedRange(table)
        | Requirement::KeyedInsert(table)
        | Requirement::LookupByAttributes { table, .. }) = *requirement
        else {
            continue;
        };
        if !touched.contains(&table) {
            touched.push(table);
        }
    }
    touched
}

fn assert_same_requirements(actual: &[Requirement], expected: &[Requirement], context: &str) {
    for requirement in expected {
        assert!(
            actual.contains(requirement),
            "{context}: missing {requirement:?}"
        );
    }
    for requirement in actual {
        assert!(
            expected.contains(requirement),
            "{context}: unexpected {requirement:?}"
        );
    }
}

fn table(name: &str) -> TableName {
    TableName(name.to_owned())
}

fn column(name: &str) -> ColumnName {
    ColumnName(name.to_owned())
}

mod kiln_e1_30 {
    use super::*;

    #[test]
    fn the_schema_is_valid_and_has_the_nine_tables_of_clause_1_3_1() {
        let schema = tpcc_schema();
        schema.validate().unwrap();

        let mut names: Vec<&str> = schema
            .tables
            .iter()
            .map(|table| table.name.0.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "CUSTOMER",
                "DISTRICT",
                "HISTORY",
                "ITEM",
                "NEW-ORDER",
                "ORDER",
                "ORDER-LINE",
                "STOCK",
                "WAREHOUSE"
            ]
        );
    }

    #[test]
    fn only_foreign_keys_of_one_column_are_declared_and_no_index() {
        let schema = tpcc_schema();

        let mut declared: Vec<(&str, &str, &str)> = schema
            .foreign_keys
            .iter()
            .map(|key| {
                (
                    key.table.0.as_str(),
                    key.column.0.as_str(),
                    key.references.0.as_str(),
                )
            })
            .collect();
        declared.sort_unstable();
        assert_eq!(
            declared,
            [
                ("DISTRICT", "D_W_ID", "WAREHOUSE"),
                ("STOCK", "S_I_ID", "ITEM"),
                ("STOCK", "S_W_ID", "WAREHOUSE"),
            ]
        );
        assert!(schema.indexes.is_empty());
    }

    #[test]
    fn a_table_keyed_by_one_column_leaves_that_column_to_the_key() {
        let schema = tpcc_schema();

        assert!(schema
            .column(&table("WAREHOUSE"), &column("W_ID"))
            .is_none());
        assert!(schema.column(&table("ITEM"), &column("I_ID")).is_none());
        assert!(schema
            .column(&table("WAREHOUSE"), &column("W_TAX"))
            .is_some());
        assert!(schema.column(&table("DISTRICT"), &column("D_ID")).is_some());
        assert!(schema
            .column(&table("DISTRICT"), &column("D_W_ID"))
            .is_some());
    }

    #[test]
    fn a_column_is_wide_enough_for_the_values_clause_1_3_1_requires() {
        let integer = |width| ColumnType::Integer {
            width,
            domain: None,
        };
        let cases = [
            ("DISTRICT", "D_ID", integer(IntegerWidth::I8)),
            ("ORDER", "O_OL_CNT", integer(IntegerWidth::I8)),
            ("STOCK", "S_QUANTITY", integer(IntegerWidth::I16)),
            ("CUSTOMER", "C_ID", integer(IntegerWidth::I32)),
            ("ORDER-LINE", "OL_AMOUNT", integer(IntegerWidth::I32)),
            ("STOCK", "S_YTD", integer(IntegerWidth::I32)),
            ("WAREHOUSE", "W_YTD", integer(IntegerWidth::I64)),
            ("ORDER", "O_ENTRY_D", integer(IntegerWidth::I64)),
            ("CUSTOMER", "C_DATA", ColumnType::Text),
        ];
        let schema = tpcc_schema();
        for (table_name, column_name, expected) in cases {
            let declared = schema
                .column(&table(table_name), &column(column_name))
                .unwrap();
            assert_eq!(declared.kind, expected, "{table_name}.{column_name}");
        }
    }
}

mod kiln_e1_31 {
    use super::*;

    /// The tables each profile selects from, updates, deletes from or inserts
    /// into, read off the clause text.
    const TABLES: [(TpccTransaction, &str, &[TpccTable]); 5] = [
        (
            TpccTransaction::NewOrder,
            "2.4.",
            &[
                Warehouse, District, Customer, NewOrder, Order, Item, Stock, OrderLine,
            ],
        ),
        (Payment, "2.5.", &[Warehouse, District, Customer, History]),
        (OrderStatus, "2.6.", &[Customer, Order, OrderLine]),
        (Delivery, "2.7.", &[NewOrder, Order, OrderLine, Customer]),
        (StockLevel, "2.8.", &[District, OrderLine, Stock]),
    ];

    #[test]
    fn every_profile_touches_the_tables_its_clause_names_and_cites_only_its_own_clauses() {
        for (transaction, clause_prefix, expected) in TABLES {
            let profile = Profile::complete(transaction);
            let touched = tables_touched(&profile);
            assert_eq!(touched.len(), expected.len(), "{transaction}");
            for table in &touched {
                assert!(expected.contains(table), "{transaction}: {table:?}");
            }
            for step in &profile.steps {
                assert!(
                    step.clause.starts_with(clause_prefix),
                    "{transaction}: {}",
                    step.clause
                );
            }
        }
    }

    /// What each profile needs that the IR cannot express, read off the clause
    /// text: several key columns, an insert without a key, rows picked by
    /// order, the clock, multiplication, text work, a returned aggregate and a
    /// response of several fields.
    #[test]
    fn every_transaction_is_refused_for_exactly_the_steps_the_ir_cannot_express() {
        use Requirement::*;
        let cases: [(TpccTransaction, Vec<Requirement>); 5] = [
            (
                TpccTransaction::NewOrder,
                vec![
                    KeyedAccess(District),
                    KeyedAccess(Customer),
                    KeyedAccess(Stock),
                    KeyedInsert(NewOrder),
                    KeyedInsert(Order),
                    KeyedInsert(OrderLine),
                    CurrentTime,
                    Multiplication,
                    TextSearch,
                    ReturnedAggregate,
                    MultiFieldResponse,
                ],
            ),
            (
                Payment,
                vec![
                    KeyedAccess(District),
                    KeyedAccess(Customer),
                    CUSTOMER_BY_LAST_NAME,
                    MiddleOfSortedRows,
                    TextConstruction,
                    KeyedInsert(History),
                    CurrentTime,
                    MultiFieldResponse,
                ],
            ),
            (
                OrderStatus,
                vec![
                    KeyedAccess(Customer),
                    CUSTOMER_BY_LAST_NAME,
                    MiddleOfSortedRows,
                    ORDER_OF_CUSTOMER,
                    LargestKey,
                    KeyedRange(OrderLine),
                    MultiFieldResponse,
                ],
            ),
            (
                Delivery,
                vec![
                    KeyedRange(NewOrder),
                    LowestKey,
                    KeyedAccess(NewOrder),
                    KeyedAccess(Order),
                    KeyedRange(OrderLine),
                    CurrentTime,
                    KeyedAccess(Customer),
                    MultiFieldResponse,
                ],
            ),
            (
                StockLevel,
                vec![
                    KeyedAccess(District),
                    KeyedRange(OrderLine),
                    KeyedAccess(Stock),
                    ReturnedAggregate,
                ],
            ),
        ];
        for (transaction, expected) in cases {
            let profile = Profile::complete(transaction);
            assert_same_requirements(
                &blocking_requirements(&profile),
                &expected,
                &transaction.to_string(),
            );
        }
    }

    #[test]
    fn a_table_keyed_by_one_column_blocks_nothing() {
        let profile = Profile::complete(TpccTransaction::NewOrder);
        let blocking = blocking_requirements(&profile);

        assert!(!blocking.contains(&Requirement::KeyedAccess(Warehouse)));
        assert!(!blocking.contains(&Requirement::KeyedAccess(Item)));
    }

    #[test]
    fn a_refusal_names_every_blocking_step_with_its_clause_and_reason() {
        for transaction in TpccTransaction::ALL {
            let refusal = refusal_of(&Profile::complete(transaction));
            let text = refusal.to_string();
            assert!(refusal.blockers().len() >= 2, "{transaction}");
            for blocker in refusal.blockers() {
                assert!(!blocker.why.is_empty());
                assert!(text.contains(blocker.clause), "{text}");
                assert!(text.contains(&blocker.why), "{text}");
            }
        }
    }

    #[test]
    fn a_refused_transaction_is_an_unknown_unit_with_its_refusal_as_reason() {
        let units = tpcc_units().gating;

        assert_eq!(units.len(), 5);
        for (unit, transaction) in units.iter().zip(TpccTransaction::ALL) {
            assert_eq!(unit.unit().subject, Subject::Tpcc(transaction));
            let written: serde_json::Value =
                serde_json::from_str(render_units(std::slice::from_ref(unit)).trim_end()).unwrap();
            let refusal = refusal_of(&Profile::complete(transaction));
            assert_eq!(written["category"], "unknown");
            assert_eq!(written["refusal_reason"], refusal.to_string());
        }
    }
}

mod kiln_e1_32 {
    use super::*;

    fn lookups(profile: &Profile) -> Vec<Requirement> {
        blocking_requirements(profile)
            .into_iter()
            .filter(|requirement| matches!(requirement, Requirement::LookupByAttributes { .. }))
            .collect()
    }

    #[test]
    fn the_by_name_customer_lookup_has_no_declared_index_and_blocks_payment_and_order_status() {
        for transaction in [Payment, OrderStatus] {
            let refusal = refusal_of(&Profile::complete(transaction));
            let blocker = refusal
                .blockers()
                .iter()
                .find(|blocker| blocker.requirement == CUSTOMER_BY_LAST_NAME)
                .expect("the by-name lookup blocks the lowering");
            assert!(
                blocker.why.contains("no primary key and no index"),
                "{}",
                blocker.why
            );
            assert!(blocker.clause.contains("case 2"), "{}", blocker.clause);
        }
        assert!(tpcc_schema().indexes.is_empty());
    }

    #[test]
    fn the_order_of_a_customer_is_looked_up_without_a_declared_index() {
        assert_eq!(
            lookups(&Profile::complete(OrderStatus)),
            [CUSTOMER_BY_LAST_NAME, ORDER_OF_CUSTOMER]
        );
        let by_id = Profile::sensitivity().remove(1);
        assert_eq!(lookups(&by_id), [ORDER_OF_CUSTOMER]);
    }

    #[test]
    fn payment_by_customer_number_looks_nothing_up_by_attributes() {
        let by_id = Profile::sensitivity().remove(0);
        assert!(lookups(&by_id).is_empty());
    }
}

mod kiln_e1_33 {
    use super::*;

    fn why(profile: &Profile, requirement: Requirement) -> String {
        refusal_of(profile)
            .blockers()
            .iter()
            .find(|blocker| blocker.requirement == requirement)
            .unwrap_or_else(|| panic!("{requirement:?} does not block"))
            .why
            .clone()
    }

    #[test]
    fn a_row_keyed_by_several_columns_blocks_the_lowering_naming_them() {
        let payment = Profile::complete(Payment);
        assert!(why(&payment, Requirement::KeyedAccess(District)).contains("(D_W_ID, D_ID)"));
        assert!(
            why(&payment, Requirement::KeyedAccess(Customer)).contains("(C_W_ID, C_D_ID, C_ID)")
        );
        let stock_level = Profile::complete(StockLevel);
        assert!(why(&stock_level, Requirement::KeyedAccess(Stock)).contains("(S_W_ID, S_I_ID)"));
        assert!(why(&stock_level, Requirement::KeyedRange(OrderLine))
            .contains("(OL_W_ID, OL_D_ID, OL_O_ID, OL_NUMBER)"));
    }

    #[test]
    fn a_table_keyed_by_one_column_is_accessed_and_inserted_into_without_a_blocker() {
        for table in [Warehouse, Item] {
            assert_eq!(Requirement::KeyedAccess(table).why_inexpressible(), None);
            assert_eq!(Requirement::KeyedRange(table).why_inexpressible(), None);
            assert_eq!(Requirement::KeyedInsert(table).why_inexpressible(), None);
        }
    }

    #[test]
    fn an_insert_into_history_blocks_the_lowering_because_it_has_no_primary_key() {
        let payment = Profile::complete(Payment);
        assert!(why(&payment, Requirement::KeyedInsert(History)).contains("no primary key"));
    }
}

mod kiln_e1_34 {
    use super::*;

    #[test]
    fn the_by_id_variants_are_reported_apart_from_the_five_gating_units() {
        let units = tpcc_units();

        let gating: Vec<_> = units
            .gating
            .iter()
            .map(|unit| unit.unit().subject.clone())
            .collect();
        assert_eq!(gating, TpccTransaction::ALL.map(Subject::Tpcc));
        let sensitivity: Vec<_> = units
            .sensitivity
            .iter()
            .map(|unit| unit.unit().subject.clone())
            .collect();
        assert_eq!(
            sensitivity,
            [Subject::Tpcc(Payment), Subject::Tpcc(OrderStatus)]
        );
    }

    #[test]
    fn a_by_id_variant_takes_a_customer_number_and_no_last_name() {
        let variants = Profile::sensitivity();
        assert_eq!(variants.len(), 2);
        for variant in &variants {
            assert!(variant.parameters.contains(&"C_ID"));
            assert!(!variant.parameters.contains(&"C_LAST"));
            assert!(Profile::complete(variant.transaction)
                .parameters
                .contains(&"C_LAST"));
            refusal_of(variant);
        }
    }
}
