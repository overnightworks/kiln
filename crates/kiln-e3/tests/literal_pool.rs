use std::collections::BTreeMap;

use kiln_e3::{Family, Signature};
use kiln_ir::{
    Block, ClampDomain, Column, ColumnName, ColumnType, Effect, Expr, Function, FunctionName,
    IntegerWidth, KeyDeclaration, Program, Schema, Slot, Table, TableName, Terminal, Value,
};

fn name(text: &str) -> String {
    text.to_owned()
}

fn literal(number: i64) -> Expr {
    Expr::Literal(Value::Integer(number))
}

fn column(label: &str, kind: ColumnType) -> Column {
    Column {
        name: ColumnName(name(label)),
        kind,
    }
}

fn integer(width: IntegerWidth, domain: Option<ClampDomain>) -> ColumnType {
    ColumnType::Integer { width, domain }
}

fn program_touching_every_column_kind() -> Program {
    let table = TableName(name("t"));
    let effects = vec![
        Effect::Update {
            table: table.clone(),
            key: vec![literal(3)],
            columns: BTreeMap::from([(ColumnName(name("count")), literal(7))]),
        },
        Effect::ReadRange {
            table: table.clone(),
            start: vec![literal(4)],
            end: vec![literal(5)],
            bound: literal(6),
            into: Slot(0),
        },
        Effect::Iterate {
            source: Slot(0),
            row: Slot(1),
            body: vec![Effect::Delete {
                table: table.clone(),
                key: vec![literal(8)],
            }],
        },
    ];
    Program {
        schema: Schema {
            tables: vec![Table {
                name: table,
                columns: vec![
                    column("id", integer(IntegerWidth::I64, None)),
                    column("label", ColumnType::Text),
                    column(
                        "level",
                        integer(IntegerWidth::I8, Some(ClampDomain { min: 0, max: 9 })),
                    ),
                    column("count", integer(IntegerWidth::I16, None)),
                ],
                key: KeyDeclaration::Columns(vec![ColumnName(name("id"))]),
            }],
            ..Default::default()
        },
        functions: vec![Function {
            name: FunctionName(name("f")),
            parameters: Vec::new(),
            body: Block {
                effects,
                terminal: Terminal::Return(literal(2)),
            },
        }],
    }
}

fn family() -> Family {
    let signature = Signature {
        components: Vec::new(),
        categories: Vec::new(),
    };
    Family::new(program_touching_every_column_kind(), signature, Vec::new())
}

mod kiln_e3_01 {
    use super::*;

    #[test]
    fn literal_pool_holds_literals_of_every_effect_kind() {
        let pool = family().literal_pool().clone();

        for number in [2, 3, 4, 5, 6, 7, 8, -128, 127, 0, 1] {
            assert!(pool.contains(&number), "missing {number}");
        }
    }

    #[test]
    fn literal_pool_leaves_out_the_width_bounds_of_key_columns() {
        let pool = family().literal_pool().clone();

        assert!(!pool.contains(&i64::MIN));
        assert!(!pool.contains(&i64::MAX));
    }

    #[test]
    fn literal_pool_holds_both_bounds_of_a_clamp_domain() {
        let pool = family().literal_pool().clone();

        assert!(pool.contains(&0) && pool.contains(&9));
    }
}
