//! What D8 and D9 reject before a call runs or abort by a key component
//! (`specs/kiln-ir.spec.md`, KILN-IR-79 to KILN-IR-82).
mod support;

use kiln_ir::{
    AbortReason, Block, Call, Effect, Expr, FunctionFault, Key, KeyComponentFault, ProgramError,
    Slot, Store, Terminal, Value,
};
use support::world::*;
use support::{aborted_reason, column, columns_of, integer, table, text};

mod kiln_ir_79 {
    use super::*;

    fn arity_fault(given: usize) -> ProgramError {
        function_fault(FunctionFault::KeyArity {
            table: table("lines"),
            expected: 3,
            given,
        })
    }

    fn keyed_by(components: usize) -> Vec<Expr> {
        (0..components).map(|n| literal(n as i64)).collect()
    }

    fn effects_keyed_by(components: usize) -> Vec<Effect> {
        let key = keyed_by(components);
        vec![
            Effect::Read {
                table: table("lines"),
                key: key.clone(),
                into: Slot(0),
            },
            Effect::Insert {
                table: table("lines"),
                key: key.clone(),
                columns: Default::default(),
            },
            Effect::Update {
                table: table("lines"),
                key: key.clone(),
                columns: Default::default(),
            },
            Effect::Delete {
                table: table("lines"),
                key: key.clone(),
            },
            Effect::ReadRange {
                table: table("lines"),
                start: key.clone(),
                end: line_key_expr(0, 0, 0),
                bound: literal(1),
                into: Slot(0),
            },
            Effect::ReadRange {
                table: table("lines"),
                start: line_key_expr(0, 0, 0),
                end: key,
                bound: literal(1),
                into: Slot(0),
            },
        ]
    }

    #[test]
    fn a_key_with_too_few_or_too_many_components_is_rejected_for_every_effect_kind() {
        for given in [0, 2, 4] {
            for effect in effects_keyed_by(given) {
                let program = world_program(vec![effect.clone()], returning_zero());

                assert_eq!(rejection_of(&program), arity_fault(given), "{effect:?}");
            }
        }
    }

    #[test]
    fn a_faulty_effect_nested_in_an_iteration_or_a_branch_is_rejected() {
        let faulty = read_line_with_two_components();
        let nested_in_iteration = Effect::Iterate {
            source: Slot(0),
            row: Slot(1),
            body: vec![faulty.clone()],
        };
        let branch = Terminal::Branch {
            condition: kiln_ir::Condition::RowPresent(Slot(0)),
            when_present: Box::new(Block {
                effects: vec![faulty],
                terminal: returning_zero(),
            }),
            when_absent: Box::new(Block {
                effects: vec![],
                terminal: returning_zero(),
            }),
        };
        let programs = [
            world_program(vec![nested_in_iteration], returning_zero()),
            world_program(vec![], branch),
        ];

        for program in programs {
            assert_eq!(rejection_of(&program), arity_fault(2));
        }
    }

    fn read_line_with_two_components() -> Effect {
        Effect::Read {
            table: table("lines"),
            key: keyed_by(2),
            into: Slot(2),
        }
    }

    #[test]
    fn the_rejection_is_the_same_whichever_function_the_call_names() {
        let mut program = world_program(effects_keyed_by(2)[..1].to_vec(), returning_zero());
        let valid = kiln_ir::Function {
            name: kiln_ir::FunctionName("fine".to_owned()),
            parameters: vec![],
            body: Block {
                effects: vec![],
                terminal: returning_zero(),
            },
        };
        program.functions.push(valid);
        let call = Call {
            function: kiln_ir::FunctionName("fine".to_owned()),
            args: vec![],
        };

        let result = kiln_ir::run(&program, &call, &lines_store(&[(1, 2, 3, 4)]));

        assert_eq!(result, Err(arity_fault(2)));
    }
}

mod kiln_ir_80 {
    use super::*;
    use support::WriteVia;

    fn writing(via: WriteVia, key_column: &'static str) -> Effect {
        let columns = columns_of([(key_column, literal(1))]);
        let key = line_key_expr(1, 2, 3);
        match via {
            WriteVia::Insert => Effect::Insert {
                table: table("lines"),
                key,
                columns,
            },
            WriteVia::Update => Effect::Update {
                table: table("lines"),
                key,
                columns,
            },
        }
    }

    #[test]
    fn writing_a_key_column_by_update_or_insert_is_rejected() {
        for (via, key_column) in [(WriteVia::Update, "d"), (WriteVia::Insert, "w")] {
            let program = world_program(vec![writing(via, key_column)], returning_zero());

            assert_eq!(
                rejection_of(&program),
                function_fault(FunctionFault::WritesKeyColumn {
                    table: table("lines"),
                    column: column(key_column)
                }),
                "{via:?}"
            );
        }
    }
}

mod kiln_ir_81 {
    use super::*;

    fn effects_on(table_name: &str) -> Vec<Effect> {
        let key = vec![Expr::Literal(text("x"))];
        vec![
            Effect::Read {
                table: table(table_name),
                key: key.clone(),
                into: Slot(0),
            },
            Effect::Insert {
                table: table(table_name),
                key: key.clone(),
                columns: Default::default(),
            },
            Effect::Update {
                table: table(table_name),
                key: key.clone(),
                columns: Default::default(),
            },
            Effect::Delete {
                table: table(table_name),
                key: key.clone(),
            },
            Effect::ReadRange {
                table: table(table_name),
                start: key.clone(),
                end: key,
                bound: literal(1),
                into: Slot(0),
            },
        ]
    }

    #[test]
    fn an_effect_naming_an_undeclared_table_is_rejected() {
        for effect in effects_on("nowhere") {
            let program = world_program(vec![effect.clone()], returning_zero());

            assert_eq!(
                rejection_of(&program),
                function_fault(FunctionFault::UndeclaredTable(table("nowhere"))),
                "{effect:?}"
            );
        }
    }

    #[test]
    fn an_effect_naming_a_keyless_table_is_rejected() {
        for effect in effects_on("events") {
            let program = world_program(vec![effect.clone()], returning_zero());

            assert_eq!(
                rejection_of(&program),
                function_fault(FunctionFault::KeylessTable(table("events"))),
                "{effect:?}"
            );
        }
    }
}

mod kiln_ir_82 {
    use super::*;

    fn key_expr_of(parts: Vec<Value>) -> Vec<Expr> {
        parts.into_iter().map(Expr::Literal).collect()
    }

    fn effects_keyed_by(table_name: &str, key: Vec<Expr>) -> Vec<Effect> {
        vec![
            Effect::Read {
                table: table(table_name),
                key: key.clone(),
                into: Slot(0),
            },
            Effect::Insert {
                table: table(table_name),
                key: key.clone(),
                columns: Default::default(),
            },
            Effect::Update {
                table: table(table_name),
                key: key.clone(),
                columns: Default::default(),
            },
            Effect::Delete {
                table: table(table_name),
                key: key.clone(),
            },
            Effect::ReadRange {
                table: table(table_name),
                start: key.clone(),
                end: key,
                bound: literal(1),
                into: Slot(0),
            },
        ]
    }

    #[test]
    fn a_component_of_the_wrong_kind_or_outside_its_width_aborts_naming_table_and_column() {
        let cases = [
            (
                "lines",
                vec![integer(1), integer(128), integer(0)],
                "d",
                KeyComponentFault::OutsideWidth,
            ),
            (
                "lines",
                vec![integer(i64::from(i32::MIN) - 1), integer(0), integer(0)],
                "w",
                KeyComponentFault::OutsideWidth,
            ),
            (
                "lines",
                vec![integer(1), integer(1), text("o")],
                "o",
                KeyComponentFault::WrongKind,
            ),
            (
                "labels",
                vec![integer(1)],
                "code",
                KeyComponentFault::WrongKind,
            ),
        ];
        let store = lines_store(&[(1, 1, 1, 1)]);

        for (table_name, key, column_name, fault) in cases {
            for effect in effects_keyed_by(table_name, key_expr_of(key.clone())) {
                let execution = run_world(vec![effect.clone()], &store);

                assert_eq!(
                    aborted_reason(&execution),
                    &AbortReason::KeyComponentRejected {
                        table: table(table_name),
                        column: column(column_name),
                        fault
                    },
                    "{effect:?}"
                );
                assert_eq!(execution.store, store);
            }
        }
    }

    #[test]
    fn a_key_column_with_a_clamping_domain_never_clamps_its_component() {
        let insert = |bucket: i64| Effect::Insert {
            table: table("buckets"),
            key: vec![literal(bucket)],
            columns: Default::default(),
        };

        let inside_width = run_world(vec![insert(50)], &Store::empty());
        let outside_width = run_world(vec![insert(i64::from(i32::MAX) + 1)], &Store::empty());

        assert!(inside_width
            .store
            .row(&table("buckets"), &Key(vec![integer(50)]))
            .is_some());
        assert_eq!(
            aborted_reason(&outside_width),
            &AbortReason::KeyComponentRejected {
                table: table("buckets"),
                column: column("bucket"),
                fault: KeyComponentFault::OutsideWidth
            }
        );
    }
}
