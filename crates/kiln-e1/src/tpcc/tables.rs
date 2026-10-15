//! The nine tables of TPC-C Clause 1.3.1 and the `kiln-ir` schema they lower
//! to. The specification declares primary keys and foreign keys, and no
//! secondary index, so the schema declares none (`inputs.md` §5, Q2.1).

use kiln_ir::{Column, ColumnName, ColumnType, ForeignKey, IntegerWidth, Schema, Table, TableName};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TpccTable {
    Warehouse,
    District,
    Customer,
    History,
    NewOrder,
    Order,
    OrderLine,
    Item,
    Stock,
}

/// How a column's values are bounded by Clause 1.3.1.
#[derive(Clone, Copy)]
enum Domain {
    /// "N unique IDs": the column holds any one of N ids.
    Ids(u64),
    /// "numeric(m[,n])" or "signed numeric(m[,n])": an integer of m digits
    /// once the n decimals are scaled away.
    Digits(u32),
    /// "date and time": whole seconds, since the IR has no date type.
    Timestamp,
    Text,
}

#[derive(Clone, Copy)]
struct ColumnSpec {
    name: &'static str,
    domain: Domain,
}

const fn ids(name: &'static str, count: u64) -> ColumnSpec {
    ColumnSpec {
        name,
        domain: Domain::Ids(count),
    }
}

const fn warehouse_ids(name: &'static str) -> ColumnSpec {
    ids(name, WAREHOUSE_ID_COUNT)
}

const fn digits(name: &'static str, count: u32) -> ColumnSpec {
    ColumnSpec {
        name,
        domain: Domain::Digits(count),
    }
}

const fn timestamp(name: &'static str) -> ColumnSpec {
    ColumnSpec {
        name,
        domain: Domain::Timestamp,
    }
}

const fn text(name: &'static str) -> ColumnSpec {
    ColumnSpec {
        name,
        domain: Domain::Text,
    }
}

/// "2*W unique IDs": Clause 1.3.1 leaves the warehouse count W open ("W
/// Warehouses are populated"); 32-bit ids hold any W a system under test can
/// run.
const WAREHOUSE_ID_COUNT: u64 = i32::MAX as u64;
const DISTRICT_ID_COUNT: u64 = 20;
const CUSTOMER_ID_COUNT: u64 = 96_000;
const ORDER_ID_COUNT: u64 = 10_000_000;
const ITEM_ID_COUNT: u64 = 200_000;

/// A foreign key as Clause 1.3.1 states it.
struct ForeignKeySpec {
    columns: &'static [&'static str],
    references: TpccTable,
}

struct TableSpec {
    name: &'static str,
    columns: &'static [ColumnSpec],
    primary_key: &'static [&'static str],
    foreign_keys: &'static [ForeignKeySpec],
}

const WAREHOUSE: TableSpec = TableSpec {
    name: "WAREHOUSE",
    columns: &[
        warehouse_ids("W_ID"),
        text("W_NAME"),
        text("W_STREET_1"),
        text("W_STREET_2"),
        text("W_CITY"),
        text("W_STATE"),
        text("W_ZIP"),
        digits("W_TAX", 4),
        digits("W_YTD", 12),
    ],
    primary_key: &["W_ID"],
    foreign_keys: &[],
};

const DISTRICT: TableSpec = TableSpec {
    name: "DISTRICT",
    columns: &[
        ids("D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("D_W_ID"),
        text("D_NAME"),
        text("D_STREET_1"),
        text("D_STREET_2"),
        text("D_CITY"),
        text("D_STATE"),
        text("D_ZIP"),
        digits("D_TAX", 4),
        digits("D_YTD", 12),
        ids("D_NEXT_O_ID", ORDER_ID_COUNT),
    ],
    primary_key: &["D_W_ID", "D_ID"],
    foreign_keys: &[ForeignKeySpec {
        columns: &["D_W_ID"],
        references: TpccTable::Warehouse,
    }],
};

const CUSTOMER: TableSpec = TableSpec {
    name: "CUSTOMER",
    columns: &[
        ids("C_ID", CUSTOMER_ID_COUNT),
        ids("C_D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("C_W_ID"),
        text("C_FIRST"),
        text("C_MIDDLE"),
        text("C_LAST"),
        text("C_STREET_1"),
        text("C_STREET_2"),
        text("C_CITY"),
        text("C_STATE"),
        text("C_ZIP"),
        text("C_PHONE"),
        timestamp("C_SINCE"),
        text("C_CREDIT"),
        digits("C_CREDIT_LIM", 12),
        digits("C_DISCOUNT", 4),
        digits("C_BALANCE", 12),
        digits("C_YTD_PAYMENT", 12),
        digits("C_PAYMENT_CNT", 4),
        digits("C_DELIVERY_CNT", 4),
        text("C_DATA"),
    ],
    primary_key: &["C_W_ID", "C_D_ID", "C_ID"],
    foreign_keys: &[ForeignKeySpec {
        columns: &["C_W_ID", "C_D_ID"],
        references: TpccTable::District,
    }],
};

const HISTORY: TableSpec = TableSpec {
    name: "HISTORY",
    columns: &[
        ids("H_C_ID", CUSTOMER_ID_COUNT),
        ids("H_C_D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("H_C_W_ID"),
        ids("H_D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("H_W_ID"),
        timestamp("H_DATE"),
        digits("H_AMOUNT", 6),
        text("H_DATA"),
    ],
    primary_key: &[],
    foreign_keys: &[
        ForeignKeySpec {
            columns: &["H_C_W_ID", "H_C_D_ID", "H_C_ID"],
            references: TpccTable::Customer,
        },
        ForeignKeySpec {
            columns: &["H_W_ID", "H_D_ID"],
            references: TpccTable::District,
        },
    ],
};

const NEW_ORDER: TableSpec = TableSpec {
    name: "NEW-ORDER",
    columns: &[
        ids("NO_O_ID", ORDER_ID_COUNT),
        ids("NO_D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("NO_W_ID"),
    ],
    primary_key: &["NO_W_ID", "NO_D_ID", "NO_O_ID"],
    foreign_keys: &[ForeignKeySpec {
        columns: &["NO_W_ID", "NO_D_ID", "NO_O_ID"],
        references: TpccTable::Order,
    }],
};

const ORDER: TableSpec = TableSpec {
    name: "ORDER",
    columns: &[
        ids("O_ID", ORDER_ID_COUNT),
        ids("O_D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("O_W_ID"),
        ids("O_C_ID", CUSTOMER_ID_COUNT),
        timestamp("O_ENTRY_D"),
        ids("O_CARRIER_ID", 10),
        digits("O_OL_CNT", 2),
        digits("O_ALL_LOCAL", 1),
    ],
    primary_key: &["O_W_ID", "O_D_ID", "O_ID"],
    foreign_keys: &[ForeignKeySpec {
        columns: &["O_W_ID", "O_D_ID", "O_C_ID"],
        references: TpccTable::Customer,
    }],
};

const ORDER_LINE: TableSpec = TableSpec {
    name: "ORDER-LINE",
    columns: &[
        ids("OL_O_ID", ORDER_ID_COUNT),
        ids("OL_D_ID", DISTRICT_ID_COUNT),
        warehouse_ids("OL_W_ID"),
        ids("OL_NUMBER", 15),
        ids("OL_I_ID", ITEM_ID_COUNT),
        warehouse_ids("OL_SUPPLY_W_ID"),
        timestamp("OL_DELIVERY_D"),
        digits("OL_QUANTITY", 2),
        digits("OL_AMOUNT", 6),
        text("OL_DIST_INFO"),
    ],
    primary_key: &["OL_W_ID", "OL_D_ID", "OL_O_ID", "OL_NUMBER"],
    foreign_keys: &[
        ForeignKeySpec {
            columns: &["OL_W_ID", "OL_D_ID", "OL_O_ID"],
            references: TpccTable::Order,
        },
        ForeignKeySpec {
            columns: &["OL_SUPPLY_W_ID", "OL_I_ID"],
            references: TpccTable::Stock,
        },
    ],
};

const ITEM: TableSpec = TableSpec {
    name: "ITEM",
    columns: &[
        ids("I_ID", ITEM_ID_COUNT),
        ids("I_IM_ID", ITEM_ID_COUNT),
        text("I_NAME"),
        digits("I_PRICE", 5),
        text("I_DATA"),
    ],
    primary_key: &["I_ID"],
    foreign_keys: &[],
};

const STOCK: TableSpec = TableSpec {
    name: "STOCK",
    columns: &[
        ids("S_I_ID", ITEM_ID_COUNT),
        warehouse_ids("S_W_ID"),
        digits("S_QUANTITY", 4),
        text("S_DIST_01"),
        text("S_DIST_02"),
        text("S_DIST_03"),
        text("S_DIST_04"),
        text("S_DIST_05"),
        text("S_DIST_06"),
        text("S_DIST_07"),
        text("S_DIST_08"),
        text("S_DIST_09"),
        text("S_DIST_10"),
        digits("S_YTD", 8),
        digits("S_ORDER_CNT", 4),
        digits("S_REMOTE_CNT", 4),
        text("S_DATA"),
    ],
    primary_key: &["S_W_ID", "S_I_ID"],
    foreign_keys: &[
        ForeignKeySpec {
            columns: &["S_W_ID"],
            references: TpccTable::Warehouse,
        },
        ForeignKeySpec {
            columns: &["S_I_ID"],
            references: TpccTable::Item,
        },
    ],
};

impl TpccTable {
    pub const ALL: [Self; 9] = [
        Self::Warehouse,
        Self::District,
        Self::Customer,
        Self::History,
        Self::NewOrder,
        Self::Order,
        Self::OrderLine,
        Self::Item,
        Self::Stock,
    ];

    fn spec(self) -> &'static TableSpec {
        match self {
            Self::Warehouse => &WAREHOUSE,
            Self::District => &DISTRICT,
            Self::Customer => &CUSTOMER,
            Self::History => &HISTORY,
            Self::NewOrder => &NEW_ORDER,
            Self::Order => &ORDER,
            Self::OrderLine => &ORDER_LINE,
            Self::Item => &ITEM,
            Self::Stock => &STOCK,
        }
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    /// The columns of the primary key (Clause 1.3.1); none for HISTORY.
    pub fn primary_key(self) -> &'static [&'static str] {
        self.spec().primary_key
    }

    fn ir_name(self) -> TableName {
        TableName(self.name().to_owned())
    }
}

/// The schema of Clause 1.3.1 as far as `kiln-ir` states it.
///
/// The IR keys a row by one value outside its columns, so a table keyed by one
/// column leaves that column out, and a composite primary key or foreign key
/// has no declaration: nothing here encodes several columns into one key.
pub fn tpcc_schema() -> Schema {
    Schema {
        tables: TpccTable::ALL.map(table_of).to_vec(),
        foreign_keys: TpccTable::ALL
            .into_iter()
            .flat_map(foreign_keys_of)
            .collect(),
        ..Schema::default()
    }
}

fn table_of(table: TpccTable) -> Table {
    let spec = table.spec();
    Table {
        name: table.ir_name(),
        columns: spec
            .columns
            .iter()
            .filter(|column| spec.primary_key != [column.name])
            .map(|column| Column {
                name: ColumnName(column.name.to_owned()),
                kind: column_type(column.domain),
            })
            .collect(),
    }
}

fn foreign_keys_of(table: TpccTable) -> Vec<ForeignKey> {
    table
        .spec()
        .foreign_keys
        .iter()
        .filter_map(|foreign_key| match foreign_key.columns {
            [column] => Some(ForeignKey {
                table: table.ir_name(),
                column: ColumnName((*column).to_owned()),
                references: foreign_key.references.ir_name(),
            }),
            _ => None,
        })
        .collect()
}

fn column_type(domain: Domain) -> ColumnType {
    let integer = |largest: u64| ColumnType::Integer {
        width: width_holding(largest),
        domain: None,
    };
    match domain {
        Domain::Ids(count) => integer(count),
        Domain::Digits(count) => integer(10_u64.pow(count) - 1),
        Domain::Timestamp => ColumnType::Integer {
            width: IntegerWidth::I64,
            domain: None,
        },
        Domain::Text => ColumnType::Text,
    }
}

fn width_holding(largest: u64) -> IntegerWidth {
    [IntegerWidth::I8, IntegerWidth::I16, IntegerWidth::I32]
        .into_iter()
        .find(|width| i64::try_from(largest).is_ok_and(|largest| largest <= width.range().1))
        .unwrap_or(IntegerWidth::I64)
}
