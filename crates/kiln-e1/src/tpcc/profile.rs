//! The five TPC-C transaction profiles as ordered steps, each citing the
//! clause of TPC-C Revision 5.11 that states it and listing what the step
//! needs from the IR. Whether the IR can express a need is decided against
//! the schema of Clause 1.3.1 (`tables.rs`) and the IR's vocabulary.

use super::tables::TpccTable;
use super::TpccTransaction;

/// What a step needs from the IR to be lowered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    /// Select, update or delete the row with this table's primary key.
    KeyedAccess(TpccTable),
    /// Select the rows whose primary key starts with given columns.
    KeyedRange(TpccTable),
    KeyedInsert(TpccTable),
    /// Select rows by columns that are not a primary key.
    LookupByAttributes {
        table: TpccTable,
        columns: &'static [&'static str],
    },
    LowestKey,
    LargestKey,
    /// The row at position n/2 rounded up among n rows sorted by a column.
    MiddleOfSortedRows,
    CurrentTime,
    Multiplication,
    TextConstruction,
    TextSearch,
    /// A sum or a count of distinct items that the function returns.
    ReturnedAggregate,
    /// A response of several fields; the IR returns one value.
    MultiFieldResponse,
}

impl Requirement {
    /// Why the IR cannot express this need, or `None` when it can.
    pub fn why_inexpressible(&self) -> Option<String> {
        match *self {
            Self::KeyedAccess(table) | Self::KeyedRange(table) => keyed_by_several(table),
            Self::KeyedInsert(table) => match table.primary_key() {
                [] => Some(format!(
                    "{} has no primary key (Clause 1.3.1) and the IR inserts a row only at a key",
                    table.name()
                )),
                _ => keyed_by_several(table),
            },
            Self::LookupByAttributes { table, columns } => Some(format!(
                "{} is looked up by ({}), which is no primary key and no index Clause 1.3.1 declares",
                table.name(),
                columns.join(", ")
            )),
            Self::LowestKey | Self::LargestKey | Self::MiddleOfSortedRows => Some(
                "the IR iterates every row of a range and cannot select the first, last or \
                 middle one"
                    .to_owned(),
            ),
            Self::CurrentTime => Some("the IR has no clock".to_owned()),
            Self::Multiplication => {
                Some("the IR's arithmetic is addition and subtraction only".to_owned())
            }
            Self::TextConstruction => {
                Some("the IR has no text concatenation, shifting or truncation".to_owned())
            }
            Self::TextSearch => {
                Some("the IR cannot test whether text contains a string".to_owned())
            }
            Self::ReturnedAggregate => {
                Some("the IR has no accumulator whose value a response can return".to_owned())
            }
            Self::MultiFieldResponse => {
                Some("the IR's response is one value, not several fields".to_owned())
            }
        }
    }
}

fn keyed_by_several(table: TpccTable) -> Option<String> {
    (table.primary_key().len() > 1).then(|| {
        format!(
            "{} is keyed by ({}) and the IR keys a row by one value",
            table.name(),
            table.primary_key().join(", ")
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub clause: &'static str,
    pub summary: &'static str,
    pub requires: &'static [Requirement],
}

const fn step(
    clause: &'static str,
    summary: &'static str,
    requires: &'static [Requirement],
) -> Step {
    Step {
        clause,
        summary,
        requires,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CustomerSelection {
    NumberOrLastName,
    NumberOnly,
}

impl CustomerSelection {
    /// The steps of a profile with the by-last-name case present or not.
    fn steps(self, before: &[Step], by_last_name_case: &[Step], after: &[Step]) -> Vec<Step> {
        let case = match self {
            Self::NumberOrLastName => by_last_name_case,
            Self::NumberOnly => &[],
        };
        [before, case, after].concat()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub transaction: TpccTransaction,
    /// The clause holding the transaction profile.
    pub clause: &'static str,
    /// The specification's printed page of that clause.
    pub page: u32,
    pub parameters: Vec<&'static str>,
    pub steps: Vec<Step>,
}

impl Profile {
    pub fn complete(transaction: TpccTransaction) -> Self {
        match transaction {
            TpccTransaction::NewOrder => new_order(),
            TpccTransaction::Payment => payment(CustomerSelection::NumberOrLastName),
            TpccTransaction::OrderStatus => order_status(CustomerSelection::NumberOrLastName),
            TpccTransaction::Delivery => delivery(),
            TpccTransaction::StockLevel => stock_level(),
        }
    }

    /// The by-id-only variants of Payment and Order-Status.
    pub fn sensitivity() -> Vec<Self> {
        vec![
            payment(CustomerSelection::NumberOnly),
            order_status(CustomerSelection::NumberOnly),
        ]
    }
}

use Requirement::{
    CurrentTime, KeyedAccess, KeyedInsert, KeyedRange, LargestKey, LowestKey, MiddleOfSortedRows,
    MultiFieldResponse, Multiplication, ReturnedAggregate, TextConstruction, TextSearch,
};
use TpccTable::{Customer, District, History, Item, NewOrder, Order, OrderLine, Stock, Warehouse};

const CUSTOMER_BY_LAST_NAME: Requirement = Requirement::LookupByAttributes {
    table: Customer,
    columns: &["C_W_ID", "C_D_ID", "C_LAST"],
};

const ORDER_OF_CUSTOMER: Requirement = Requirement::LookupByAttributes {
    table: Order,
    columns: &["O_W_ID", "O_D_ID", "O_C_ID"],
};

/// Clause 2.4.2.2, with the order-entry date of Clause 2.4.1.6. The 5 to 15
/// items of Clause 2.4.1.3 are fifteen blocks, each guarded by a comparison of
/// arguments, which the IR expresses, so they block nothing.
const NEW_ORDER_STEPS: [Step; 12] = [
    step("2.4.2.2", "select WAREHOUSE by W_ID, retrieve W_TAX", &[KeyedAccess(Warehouse)]),
    step(
        "2.4.2.2",
        "select DISTRICT by (D_W_ID, D_ID), retrieve D_TAX, retrieve and increment D_NEXT_O_ID",
        &[KeyedAccess(District)],
    ),
    step(
        "2.4.2.2",
        "select CUSTOMER by (C_W_ID, C_D_ID, C_ID), retrieve C_DISCOUNT, C_LAST, C_CREDIT",
        &[KeyedAccess(Customer)],
    ),
    step("2.4.2.2", "insert the NEW-ORDER row", &[KeyedInsert(NewOrder)]),
    step(
        "2.4.2.2, 2.4.1.6",
        "insert the ORDER row, O_ENTRY_D the current time, O_ALL_LOCAL from the order-lines",
        &[KeyedInsert(Order), CurrentTime],
    ),
    step(
        "2.4.2.2, 2.4.2.3",
        "select ITEM by I_ID per item, roll back when the item is unused",
        &[KeyedAccess(Item)],
    ),
    step(
        "2.4.2.2",
        "select STOCK by (S_W_ID, S_I_ID) per item, retrieve S_DIST_xx of the district, update S_QUANTITY, S_YTD, S_ORDER_CNT, S_REMOTE_CNT",
        &[KeyedAccess(Stock)],
    ),
    step("2.4.2.2", "OL_AMOUNT is OL_QUANTITY * I_PRICE", &[Multiplication]),
    step(
        "2.4.2.2",
        "brand-generic is B when I_DATA and S_DATA both include \"ORIGINAL\"",
        &[TextSearch],
    ),
    step("2.4.2.2", "insert the ORDER-LINE row per item", &[KeyedInsert(OrderLine)]),
    step(
        "2.4.2.2",
        "total-amount is sum(OL_AMOUNT) * (1 - C_DISCOUNT) * (1 + W_TAX + D_TAX)",
        &[ReturnedAggregate, Multiplication],
    ),
    step(
        "2.4.2.2, 2.4.3.3",
        "communicate the output data to the terminal",
        &[MultiFieldResponse],
    ),
];

fn new_order() -> Profile {
    Profile {
        transaction: TpccTransaction::NewOrder,
        clause: "2.4.2.2",
        page: 29,
        parameters: vec![
            "W_ID",
            "D_ID",
            "C_ID",
            "OL_I_ID",
            "OL_SUPPLY_W_ID",
            "OL_QUANTITY",
        ],
        steps: NEW_ORDER_STEPS.to_vec(),
    }
}

/// Clause 2.5.2.2, case 1 (customer number) and case 2 (last name) as an
/// argument-selected branch (2.5.2.1).
const PAYMENT_BEFORE_CUSTOMER: [Step; 3] = [
    step(
        "2.5.2.2",
        "select WAREHOUSE by W_ID, retrieve its name and address, increase W_YTD",
        &[KeyedAccess(Warehouse)],
    ),
    step(
        "2.5.2.2",
        "select DISTRICT by (D_W_ID, D_ID), retrieve its name and address, increase D_YTD",
        &[KeyedAccess(District)],
    ),
    step(
        "2.5.2.2, case 1",
        "select CUSTOMER by (C_W_ID, C_D_ID, C_ID), update C_BALANCE, C_YTD_PAYMENT, C_PAYMENT_CNT",
        &[KeyedAccess(Customer)],
    ),
];

const PAYMENT_BY_LAST_NAME: [Step; 1] = [step(
    "2.5.2.2, case 2",
    "select the CUSTOMER rows with (C_W_ID, C_D_ID, C_LAST) sorted by C_FIRST, take position n/2 rounded up",
    &[CUSTOMER_BY_LAST_NAME, MiddleOfSortedRows],
)];

const PAYMENT_AFTER_CUSTOMER: [Step; 4] = [
    step(
        "2.5.2.2",
        "when C_CREDIT is \"BC\" shift the history information into C_DATA, at most 500 characters",
        &[TextConstruction],
    ),
    step(
        "2.5.2.2",
        "H_DATA is W_NAME and D_NAME separated by 4 spaces",
        &[TextConstruction],
    ),
    step(
        "2.5.2.2, 2.5.1.4",
        "insert the HISTORY row, H_DATE the current time",
        &[KeyedInsert(History), CurrentTime],
    ),
    step(
        "2.5.2.2, 2.5.3.3",
        "communicate the output data to the terminal",
        &[MultiFieldResponse],
    ),
];

fn payment(selection: CustomerSelection) -> Profile {
    let mut parameters = vec!["W_ID", "D_ID", "C_W_ID", "C_D_ID", "C_ID", "H_AMOUNT"];
    if selection == CustomerSelection::NumberOrLastName {
        parameters.push("C_LAST");
    }
    Profile {
        transaction: TpccTransaction::Payment,
        clause: "2.5.2.2",
        page: 34,
        parameters,
        steps: selection.steps(
            &PAYMENT_BEFORE_CUSTOMER,
            &PAYMENT_BY_LAST_NAME,
            &PAYMENT_AFTER_CUSTOMER,
        ),
    }
}

/// Clause 2.6.2.2, case 1 (customer number) and case 2 (last name) as an
/// argument-selected branch (2.6.2.1).
const ORDER_STATUS_BEFORE_ORDER: [Step; 1] = [step(
    "2.6.2.2, case 1",
    "select CUSTOMER by (C_W_ID, C_D_ID, C_ID), retrieve C_BALANCE, C_FIRST, C_MIDDLE, C_LAST",
    &[KeyedAccess(Customer)],
)];

const ORDER_STATUS_BY_LAST_NAME: [Step; 1] = [step(
    "2.6.2.2, case 2",
    "select the CUSTOMER rows with (C_W_ID, C_D_ID, C_LAST) sorted by C_FIRST, take position n/2 rounded up",
    &[CUSTOMER_BY_LAST_NAME, MiddleOfSortedRows],
)];

const ORDER_STATUS_AFTER_CUSTOMER: [Step; 3] = [
    step(
        "2.6.2.2",
        "select the ORDER of the customer with the largest O_ID",
        &[ORDER_OF_CUSTOMER, LargestKey],
    ),
    step(
        "2.6.2.2",
        "select the ORDER-LINE rows of (OL_W_ID, OL_D_ID, OL_O_ID) of that order",
        &[KeyedRange(OrderLine)],
    ),
    step(
        "2.6.2.2, 2.6.3.3",
        "communicate the output data to the terminal",
        &[MultiFieldResponse],
    ),
];

fn order_status(selection: CustomerSelection) -> Profile {
    let mut parameters = vec!["C_W_ID", "C_D_ID", "C_ID"];
    if selection == CustomerSelection::NumberOrLastName {
        parameters.push("C_LAST");
    }
    Profile {
        transaction: TpccTransaction::OrderStatus,
        clause: "2.6.2.2",
        page: 37,
        parameters,
        steps: selection.steps(
            &ORDER_STATUS_BEFORE_ORDER,
            &ORDER_STATUS_BY_LAST_NAME,
            &ORDER_STATUS_AFTER_CUSTOMER,
        ),
    }
}

/// Clause 2.7.4.2, per district of the warehouse; skipping a district with no
/// outstanding order is a branch on the first read.
const DELIVERY_STEPS: [Step; 6] = [
    step(
        "2.7.4.2",
        "select the NEW-ORDER row of (NO_W_ID, NO_D_ID) with the lowest NO_O_ID, skip the district if none",
        &[KeyedRange(NewOrder), LowestKey],
    ),
    step("2.7.4.2", "delete the selected NEW-ORDER row", &[KeyedAccess(NewOrder)]),
    step(
        "2.7.4.2",
        "select the ORDER by (O_W_ID, O_D_ID, O_ID), retrieve O_C_ID, update O_CARRIER_ID",
        &[KeyedAccess(Order)],
    ),
    step(
        "2.7.4.2, 2.7.1.3",
        "select the ORDER-LINE rows of the order, update OL_DELIVERY_D to the current time",
        &[KeyedRange(OrderLine), CurrentTime],
    ),
    step(
        "2.7.4.2",
        "select CUSTOMER by (C_W_ID, C_D_ID, C_ID), add the OL_AMOUNT sum to C_BALANCE, increment C_DELIVERY_CNT",
        &[KeyedAccess(Customer)],
    ),
    step(
        "2.7.4.2, 2.7.2.2",
        "record the delivered orders and times in the result file",
        &[MultiFieldResponse, CurrentTime],
    ),
];

fn delivery() -> Profile {
    Profile {
        transaction: TpccTransaction::Delivery,
        clause: "2.7.4.2",
        page: 42,
        parameters: vec!["W_ID", "O_CARRIER_ID"],
        steps: DELIVERY_STEPS.to_vec(),
    }
}

/// Clause 2.8.2.2; its output (2.8.3.3) is the input data and one computed
/// field, `low_stock`, so the response itself blocks nothing.
const STOCK_LEVEL_STEPS: [Step; 3] = [
    step(
        "2.8.2.2",
        "select DISTRICT by (D_W_ID, D_ID), retrieve D_NEXT_O_ID",
        &[KeyedAccess(District)],
    ),
    step(
        "2.8.2.2",
        "select the ORDER-LINE rows of (OL_W_ID, OL_D_ID) with OL_O_ID from D_NEXT_O_ID - 20 below D_NEXT_O_ID",
        &[KeyedRange(OrderLine)],
    ),
    step(
        "2.8.2.2",
        "select STOCK by (S_W_ID, S_I_ID) per distinct OL_I_ID, count those with S_QUANTITY below threshold",
        &[KeyedAccess(Stock), ReturnedAggregate],
    ),
];

fn stock_level() -> Profile {
    Profile {
        transaction: TpccTransaction::StockLevel,
        clause: "2.8.2.2",
        page: 44,
        parameters: vec!["W_ID", "D_ID", "threshold"],
        steps: STOCK_LEVEL_STEPS.to_vec(),
    }
}
