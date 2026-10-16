//! Small `kiln-ir` programs for the classifier tests. A test target includes
//! this file with `#[path]` and uses all of it.

use std::collections::BTreeMap;

use kiln_e1::{
    classify_function, Classification, FootprintClassification, ProvenCategory, Reason,
    UnknownCause,
};
use kiln_ir::{
    ArithmeticOp, Block, Column, ColumnName, ColumnType, Condition, Effect, Expr, ForeignKey,
    Function, FunctionName, Index, IndexName, IntegerWidth, KeyDeclaration, KeyInterval,
    ParameterName, Program, Relation, Schema, Slot, Table, TableName, Terminal, Value,
};

pub const FUNCTION: &str = "f";

pub fn table(name: &str) -> TableName {
    TableName(name.to_owned())
}

pub fn param(name: &str) -> Expr {
    Expr::Parameter(ParameterName(name.to_owned()))
}

pub fn literal(value: i64) -> Expr {
    Expr::Literal(Value::Integer(value))
}

pub fn plus(left: Expr, right: Expr) -> Expr {
    Expr::Arithmetic(ArithmeticOp::Add, Box::new(left), Box::new(right))
}

pub fn column_of(slot: u32, column: &str) -> Expr {
    Expr::Projection {
        slot: Slot(slot),
        column: ColumnName(column.to_owned()),
    }
}

/// The key column of every table of the shared schema except [`LINES`].
pub const KEY_COLUMN: &str = "id";

/// The shared schema's table keyed by three columns.
pub const LINES: &str = "lines";

/// A table keyed by the same column names as [`LINES`].
pub const ARCHIVE: &str = "lines_archive";

/// A table keyed by [`KEY_COLUMN`] whose other columns carry the names of
/// [`LINE_KEY`]: a row of it names a line without being one.
pub const LINE_REFS: &str = "line_refs";

/// A table keyed by [`KEY_COLUMN`] whose rows name a line of [`LINES`] by a
/// declared foreign key over the columns of [`LINE_KEY`].
pub const SHIPMENTS: &str = "shipments";

/// A table keyed by `(w_id, item_id)`, which a line names by its
/// `(customer_id, item_id)` through a declared foreign key.
pub const INVENTORY: &str = "inventory";

/// The key of [`LINES`] in declared order.
pub const LINE_KEY: [&str; 3] = ["w_id", "d_id", "o_id"];

pub fn key_of(slot: u32) -> Expr {
    column_of(slot, KEY_COLUMN)
}

/// The components of a key of [`LINES`] projected from the row in `slot`, in
/// declared order.
pub fn line_key_of(slot: u32) -> Vec<Expr> {
    LINE_KEY.map(|column| column_of(slot, column)).to_vec()
}

pub fn read(table_name: &str, key: Expr, into: u32) -> Effect {
    read_at(table_name, vec![key], into)
}

pub fn read_at(table_name: &str, key: Vec<Expr>, into: u32) -> Effect {
    Effect::Read {
        table: table(table_name),
        key,
        into: Slot(into),
    }
}

pub fn insert(table_name: &str, key: Expr) -> Effect {
    insert_at(table_name, vec![key])
}

pub fn insert_at(table_name: &str, key: Vec<Expr>) -> Effect {
    Effect::Insert {
        table: table(table_name),
        key,
        columns: BTreeMap::new(),
    }
}

pub fn insert_writing(table_name: &str, key: Expr, columns: &[(&str, Expr)]) -> Effect {
    Effect::Insert {
        table: table(table_name),
        key: vec![key],
        columns: written(columns),
    }
}

pub fn update(table_name: &str, key: Expr, columns: &[(&str, Expr)]) -> Effect {
    update_at(table_name, vec![key], columns)
}

pub fn update_at(table_name: &str, key: Vec<Expr>, columns: &[(&str, Expr)]) -> Effect {
    Effect::Update {
        table: table(table_name),
        key,
        columns: written(columns),
    }
}

fn written(columns: &[(&str, Expr)]) -> BTreeMap<ColumnName, Expr> {
    columns
        .iter()
        .map(|(name, value)| (ColumnName((*name).to_owned()), value.clone()))
        .collect()
}

pub fn delete(table_name: &str, key: Expr) -> Effect {
    Effect::Delete {
        table: table(table_name),
        key: vec![key],
    }
}

/// A range over a table keyed by one column, between `from` and `to`.
pub fn read_range(table_name: &str, from: Expr, to: Expr, into: u32) -> Effect {
    read_prefix_range(table_name, vec![], Some((from, to)), into)
}

pub fn read_prefix_range(
    table_name: &str,
    prefix: Vec<Expr>,
    interval: Option<(Expr, Expr)>,
    into: u32,
) -> Effect {
    read_range_bounded(table_name, prefix, interval, literal(10), into)
}

pub fn read_range_bounded(
    table_name: &str,
    prefix: Vec<Expr>,
    interval: Option<(Expr, Expr)>,
    bound: Expr,
    into: u32,
) -> Effect {
    Effect::ReadRange {
        table: table(table_name),
        prefix,
        interval: interval.map(|(from, to)| KeyInterval { from, to }),
        bound,
        into: Slot(into),
    }
}

pub fn read_index(index: &str, start: Vec<Expr>, end: Vec<Expr>, into: u32) -> Effect {
    read_index_bounded(index, start, end, literal(10), into)
}

pub fn read_index_bounded(
    index: &str,
    start: Vec<Expr>,
    end: Vec<Expr>,
    bound: Expr,
    into: u32,
) -> Effect {
    Effect::ReadIndexRange {
        index: IndexName(index.to_owned()),
        start,
        end,
        bound,
        into: Slot(into),
    }
}

pub fn iterate(source: u32, row: u32, body: Vec<Effect>) -> Effect {
    Effect::Iterate {
        source: Slot(source),
        row: Slot(row),
        body,
    }
}

pub fn returns(effects: Vec<Effect>) -> Block {
    returning(effects, literal(0))
}

pub fn returning(effects: Vec<Effect>, response: Expr) -> Block {
    Block {
        effects,
        terminal: Terminal::Return(response),
    }
}

pub fn branch_on_presence(effects: Vec<Effect>, slot: u32, present: Block, absent: Block) -> Block {
    Block {
        effects,
        terminal: Terminal::Branch {
            condition: Condition::RowPresent(Slot(slot)),
            when_present: Box::new(present),
            when_absent: Box::new(absent),
        },
    }
}

/// Takes `present` when `left` equals `right`, `absent` otherwise.
pub fn branch_on_equality(
    effects: Vec<Effect>,
    (left, right): (Expr, Expr),
    present: Block,
    absent: Block,
) -> Block {
    Block {
        effects,
        terminal: Terminal::Branch {
            condition: Condition::Compare {
                left,
                relation: Relation::Equal,
                right,
            },
            when_present: Box::new(present),
            when_absent: Box::new(absent),
        },
    }
}

fn integer_column(name: &str) -> Column {
    Column {
        name: ColumnName(name.to_owned()),
        kind: ColumnType::Integer {
            width: IntegerWidth::I64,
            domain: None,
        },
    }
}

fn keyed_table(name: &str, key: &[&str], other_columns: &[&str]) -> Table {
    Table {
        name: table(name),
        columns: key
            .iter()
            .chain(other_columns)
            .map(|column| integer_column(column))
            .collect(),
        key: KeyDeclaration::Columns(
            key.iter()
                .map(|column| ColumnName((*column).to_owned()))
                .collect(),
        ),
    }
}

/// Orders reference customers and products, customers reference regions; the
/// declared indexes are per customer and per tenant. Every table is keyed by
/// [`KEY_COLUMN`] but [`LINES`] and [`ARCHIVE`], which have the three-column
/// [`LINE_KEY`] and are indexed by item.
pub fn schema() -> Schema {
    let foreign_key = |from: &str, columns: &[&str], to: &str| ForeignKey {
        table: table(from),
        columns: columns
            .iter()
            .map(|column| ColumnName((*column).to_owned()))
            .collect(),
        references: table(to),
    };
    let index = |name: &str, on: &str, columns: &[&str]| Index {
        name: IndexName(name.to_owned()),
        table: table(on),
        columns: columns
            .iter()
            .map(|c| ColumnName((*c).to_owned()))
            .collect(),
        unique: false,
    };
    let single_key =
        |name: &str, other_columns: &[&str]| keyed_table(name, &[KEY_COLUMN], other_columns);
    Schema {
        tables: vec![
            single_key("orders", &["customer_id", "product_id", "tenant_id"]),
            single_key("customers", &["region_id", "tenant_id"]),
            single_key("products", &[]),
            single_key("regions", &[]),
            single_key("accounts", &[]),
            single_key("audit", &[]),
            single_key("ledger", &[]),
            single_key("log", &[]),
            single_key("districts", &[]),
            single_key("order_lines", &[]),
            single_key("a", &[]),
            single_key("b", &[]),
            single_key("shared", &[]),
            single_key(LINE_REFS, &LINE_KEY),
            keyed_table(LINES, &LINE_KEY, &["customer_id", "item_id"]),
            keyed_table(ARCHIVE, &LINE_KEY, &["item_id"]),
            keyed_table(SHIPMENTS, &[KEY_COLUMN], &LINE_KEY),
            keyed_table(INVENTORY, &["w_id", "item_id"], &[]),
        ],
        foreign_keys: vec![
            foreign_key("orders", &["customer_id"], "customers"),
            foreign_key("orders", &["product_id"], "products"),
            foreign_key("customers", &["region_id"], "regions"),
            foreign_key(LINES, &["customer_id"], "customers"),
            foreign_key(LINES, &["w_id", "d_id", "customer_id"], ARCHIVE),
            foreign_key(LINES, &["customer_id", "item_id"], INVENTORY),
            foreign_key(SHIPMENTS, &LINE_KEY, LINES),
        ],
        indexes: vec![
            index("orders_by_customer", "orders", &["customer_id"]),
            index("orders_by_tenant", "orders", &["tenant_id", "customer_id"]),
            index(
                "customers_by_tenant",
                "customers",
                &["tenant_id", "region_id"],
            ),
            index("lines_by_item", LINES, &["item_id"]),
            index("archive_by_item", ARCHIVE, &["item_id"]),
        ],
        ..Schema::default()
    }
}

/// A program of the shared schema whose only function, named [`FUNCTION`],
/// has the given body.
pub fn program(body: Block) -> Program {
    Program {
        schema: schema(),
        functions: vec![Function {
            name: FunctionName(FUNCTION.to_owned()),
            parameters: Vec::new(),
            body,
        }],
    }
}

pub fn classify(body: Block) -> FootprintClassification {
    classify_function(&program(body), &FunctionName(FUNCTION.to_owned())).unwrap()
}

pub fn category_of(body: Block) -> Option<ProvenCategory> {
    match classify(body).classification {
        Classification::Proven { category, .. } => Some(category),
        _ => None,
    }
}

pub fn evidence_of(body: Block) -> Vec<String> {
    match classify(body).classification {
        Classification::Proven { evidence, .. } => evidence.paths().to_vec(),
        Classification::Unknown { evidence, .. } => evidence.unwrap().paths().to_vec(),
        Classification::ExcludedNonDatabase(_) => unreachable!("the classifier excludes nothing"),
    }
}

pub fn assert_unknown(body: Block, cause: UnknownCause) {
    let Classification::Unknown { reason, .. } = classify(body).classification else {
        panic!("expected unknown because of {cause}");
    };
    assert_eq!(reason, Reason::new(cause.to_string()).unwrap());
}

pub const STATIC: Option<ProvenCategory> = Some(ProvenCategory::Static);
pub const ONE_HOP: Option<ProvenCategory> = Some(ProvenCategory::OneHop);

/// Reads an order by `order_id` into slot 0.
pub fn read_order() -> Effect {
    read("orders", param("order_id"), 0)
}

/// Reads the customer an order in slot 0 names through its foreign key.
pub fn read_customer_of_order(into: u32) -> Effect {
    read("customers", column_of(0, "customer_id"), into)
}
