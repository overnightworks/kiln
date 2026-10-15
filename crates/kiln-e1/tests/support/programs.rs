//! Small `kiln-ir` programs for the classifier tests. A test target includes
//! this file with `#[path]` and uses all of it.

use std::collections::BTreeMap;

use kiln_e1::{
    classify_function, Classification, FootprintClassification, ProvenCategory, Reason,
    UnknownCause,
};
use kiln_ir::{
    ArithmeticOp, Block, ColumnName, Condition, Effect, Expr, ForeignKey, Function, FunctionName,
    Index, IndexName, ParameterName, Program, Schema, Slot, TableName, Terminal, Value,
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

pub fn key_of(slot: u32) -> Expr {
    Expr::PrimaryKey { slot: Slot(slot) }
}

pub fn read(table_name: &str, key: Expr, into: u32) -> Effect {
    Effect::Read {
        table: table(table_name),
        key,
        into: Slot(into),
    }
}

pub fn insert(table_name: &str, key: Expr) -> Effect {
    Effect::Insert {
        table: table(table_name),
        key,
        columns: BTreeMap::new(),
    }
}

pub fn update(table_name: &str, key: Expr, columns: &[(&str, Expr)]) -> Effect {
    Effect::Update {
        table: table(table_name),
        key,
        columns: columns
            .iter()
            .map(|(name, value)| (ColumnName((*name).to_owned()), value.clone()))
            .collect(),
    }
}

pub fn read_range(table_name: &str, start: Expr, end: Expr, into: u32) -> Effect {
    Effect::ReadRange {
        table: table(table_name),
        start,
        end,
        bound: literal(10),
        into: Slot(into),
    }
}

pub fn read_index(index: &str, start: Vec<Expr>, end: Vec<Expr>, into: u32) -> Effect {
    Effect::ReadIndexRange {
        index: IndexName(index.to_owned()),
        start,
        end,
        bound: literal(10),
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

/// Orders reference customers and products, customers reference regions; the
/// declared indexes are per customer and per tenant.
pub fn schema() -> Schema {
    let foreign_key = |from: &str, column: &str, to: &str| ForeignKey {
        table: table(from),
        column: ColumnName(column.to_owned()),
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
    Schema {
        foreign_keys: vec![
            foreign_key("orders", "customer_id", "customers"),
            foreign_key("orders", "product_id", "products"),
            foreign_key("customers", "region_id", "regions"),
        ],
        indexes: vec![
            index("orders_by_customer", "orders", &["customer_id"]),
            index("orders_by_tenant", "orders", &["tenant_id", "customer_id"]),
            index(
                "customers_by_tenant",
                "customers",
                &["tenant_id", "region_id"],
            ),
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
