//! What D8 and D9 reject before a call runs or abort by a key component
//! (`specs/kiln-ir.spec.md`, KILN-IR-79 to KILN-IR-82).
mod support;

use kiln_ir::{
    AbortReason, Block, Call, Effect, Expr, FunctionFault, Key, KeyComponentFault, KeyInterval,
    ProgramError, Slot, Store, Terminal,
};
use support::world::*;
use support::{
    aborted_reason, call_under_test, column, columns_of, columns_of_values, committed_response,
    integer, parameter, program_with_schema, run_ok, table, text, text_parameter, typed_parameter,
};

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
                prefix: vec![],
                interval: Some(KeyInterval {
                    from: key[0].clone(),
                    to: key[0].clone(),
                }),
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
    use kiln_ir::{
        ArithmeticOp, Column, ColumnName, ColumnType, FunctionName, IntegerWidth, Parameter,
        Program, Row, Schema, Table,
    };

    /// The effects that carry a key, each with `bad` as the component at
    /// `position` of the key it names and valid components elsewhere.
    fn effects_with_component(table_name: &str, position: usize, bad: Expr) -> Vec<Effect> {
        let mut key = match table_name {
            "lines" => line_key_expr(1, 1, 1),
            _ => vec![Expr::Literal(text("x"))],
        };
        key[position] = bad.clone();
        let valid = if table_name == "lines" {
            line_key_expr(1, 1, 1)
        } else {
            vec![Expr::Literal(text("x"))]
        };
        let into = Slot(0);
        let bound = literal(1);
        vec![
            Effect::Read {
                table: table(table_name),
                key: key.clone(),
                into,
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
                prefix: valid[..position].to_vec(),
                interval: Some(KeyInterval {
                    from: bad.clone(),
                    to: valid[position].clone(),
                }),
                bound: bound.clone(),
                into,
            },
            Effect::ReadRange {
                table: table(table_name),
                prefix: valid[..position].to_vec(),
                interval: Some(KeyInterval {
                    from: valid[position].clone(),
                    to: bad,
                }),
                bound,
                into,
            },
        ]
    }

    fn param(name: &str) -> Expr {
        Expr::Parameter(parameter(name))
    }

    fn sum() -> Expr {
        Expr::Arithmetic(
            ArithmeticOp::Add,
            Box::new(literal(1)),
            Box::new(literal(1)),
        )
    }

    /// Reads a `lines` row and a `labels` row into slots 8 and 9, so that
    /// their columns can be projected into a key.
    fn rows_to_project_from() -> Vec<Effect> {
        vec![
            read_line(1, 1, 1, Slot(8)),
            Effect::Read {
                table: table("labels"),
                key: vec![Expr::Literal(text("x"))],
                into: Slot(9),
            },
        ]
    }

    fn parameters() -> Vec<Parameter> {
        vec![
            typed_parameter("count", IntegerWidth::I64),
            text_parameter("name"),
        ]
    }

    type Rejection = Option<(ColumnName, KeyComponentFault)>;

    /// What validating a program with just that effect says about its key
    /// column, for each effect that carries a key.
    fn rejections_by_effect(
        table_name: &str,
        position: usize,
        component: Expr,
    ) -> Vec<(Effect, Rejection)> {
        effects_with_component(table_name, position, component)
            .into_iter()
            .map(|effect| {
                let mut effects = rows_to_project_from();
                effects.push(effect.clone());
                let program =
                    program_with_schema(world_schema(), parameters(), effects, returning_zero());
                let rejection = match program.validate() {
                    Err(ProgramError::Function {
                        fault: FunctionFault::KeyComponent { column, fault, .. },
                        ..
                    }) => Some((column, fault)),
                    Ok(()) => None,
                    other => panic!("{other:?}"),
                };
                (effect, rejection)
            })
            .collect()
    }

    /// Every (component, effect) cell whose rejection is not `expected`.
    fn cells_not_rejected_as(cases: Vec<(&str, usize, Expr, Rejection)>) -> Vec<String> {
        cases
            .into_iter()
            .flat_map(|(table_name, position, component, expected)| {
                rejections_by_effect(table_name, position, component.clone())
                    .into_iter()
                    .filter(|(_, rejection)| *rejection != expected)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(move |(effect, rejection)| {
                        format!("{component:?} in {effect:?}: {rejection:?}, not {expected:?}")
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn a_component_that_can_never_fit_its_column_is_rejected_before_any_call_runs() {
        use KeyComponentFault::{OutsideWidth, WrongKind};
        let rejected = |column_name, fault| Some((column(column_name), fault));
        let cases = vec![
            (
                "lines",
                1,
                Expr::Literal(text("d")),
                rejected("d", WrongKind),
            ),
            ("lines", 1, literal(128), rejected("d", OutsideWidth)),
            (
                "lines",
                0,
                literal(i64::from(i32::MIN) - 1),
                rejected("w", OutsideWidth),
            ),
            ("labels", 0, literal(1), rejected("code", WrongKind)),
            ("lines", 2, param("name"), rejected("o", WrongKind)),
            ("labels", 0, param("count"), rejected("code", WrongKind)),
            ("labels", 0, sum(), rejected("code", WrongKind)),
            (
                "lines",
                1,
                project(Slot(9), "note"),
                rejected("d", WrongKind),
            ),
            (
                "labels",
                0,
                project(Slot(8), "qty"),
                rejected("code", WrongKind),
            ),
        ];

        assert_eq!(cells_not_rejected_as(cases), Vec::<String>::new());
    }

    #[test]
    fn a_component_that_may_fit_its_column_is_accepted() {
        let cases = vec![
            ("lines", 1, literal(127), None),
            ("labels", 0, Expr::Literal(text("y")), None),
            ("lines", 1, param("count"), None),
            ("lines", 1, sum(), None),
            ("lines", 1, project(Slot(8), "qty"), None),
            ("labels", 0, project(Slot(9), "note"), None),
            ("labels", 0, param("name"), None),
        ];

        assert_eq!(cells_not_rejected_as(cases), Vec::<String>::new());
    }

    #[test]
    fn a_projection_from_a_slot_two_tables_bind_is_left_to_the_run() {
        let ambiguous_slot = vec![
            Effect::Read {
                table: table("labels"),
                key: vec![Expr::Literal(text("x"))],
                into: Slot(0),
            },
            read_line(1, 1, 1, Slot(0)),
            Effect::Delete {
                table: table("labels"),
                key: vec![project(Slot(0), "d")],
            },
        ];
        let program = world_program(ambiguous_slot, returning_zero());
        let store = lines_store(&[(1, 1, 1, 1)]);

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(
            aborted_reason(&execution),
            &AbortReason::KeyComponentRejected {
                table: table("labels"),
                column: column("code"),
                fault: KeyComponentFault::WrongKind
            }
        );
    }

    #[test]
    fn a_component_outside_its_width_known_only_at_run_time_aborts_naming_table_and_column() {
        let cases = [
            ("lines", 1, 128, "d"),
            ("lines", 0, i64::from(i32::MIN) - 1, "w"),
        ];
        let store = lines_store(&[(1, 1, 1, 1)]);

        for (table_name, position, argument, column_name) in cases {
            for effect in effects_with_component(table_name, position, param("count")) {
                let program = program_with_schema(
                    world_schema(),
                    parameters(),
                    vec![effect.clone()],
                    returning_zero(),
                );
                let call = call_under_test(vec![integer(argument), text("n")]);

                let execution = run_ok(&program, &call, &store);

                assert_eq!(
                    aborted_reason(&execution),
                    &AbortReason::KeyComponentRejected {
                        table: table(table_name),
                        column: column(column_name),
                        fault: KeyComponentFault::OutsideWidth
                    },
                    "{effect:?}"
                );
                assert_eq!(execution.store, store);
            }
        }
    }

    #[test]
    fn a_key_column_with_a_clamping_domain_never_clamps_its_component() {
        let insert = Effect::Insert {
            table: table("buckets"),
            key: vec![param("count")],
            columns: Default::default(),
        };
        let program =
            program_with_schema(world_schema(), parameters(), vec![insert], returning_zero());
        let run_with = |bucket: i64| {
            run_ok(
                &program,
                &call_under_test(vec![integer(bucket), text("n")]),
                &Store::empty(),
            )
        };

        let inside_width = run_with(50);
        let outside_width = run_with(i64::from(i32::MAX) + 1);

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

    fn key_table(name: &str, kind: ColumnType) -> Table {
        Table {
            name: table(name),
            columns: vec![Column {
                name: column("c"),
                kind,
            }],
            key: key_of(&["c"]),
        }
    }

    fn range_over(table_name: &str, into: u32, start: Expr, end: Expr) -> Effect {
        Effect::ReadRange {
            table: table(table_name),
            prefix: vec![],
            interval: Some(KeyInterval {
                from: start,
                to: end,
            }),
            bound: literal(5),
            into: Slot(into),
        }
    }

    fn iterate(source: u32, row: u32, body: Vec<Effect>) -> Effect {
        Effect::Iterate {
            source: Slot(source),
            row: Slot(row),
            body,
        }
    }

    /// `text_rows` is empty, `int_rows` and `outer` hold rows; `u` is
    /// integer-keyed. Both `text_rows` and `int_rows` have a column `c`.
    fn iteration_row_program(row_sources: Vec<Effect>) -> Program {
        let schema = Schema {
            tables: vec![
                key_table("text_rows", ColumnType::Text),
                key_table(
                    "int_rows",
                    ColumnType::Integer {
                        width: IntegerWidth::I64,
                        domain: None,
                    },
                ),
                key_table(
                    "u",
                    ColumnType::Integer {
                        width: IntegerWidth::I64,
                        domain: None,
                    },
                ),
                key_table(
                    "outer",
                    ColumnType::Integer {
                        width: IntegerWidth::I64,
                        domain: None,
                    },
                ),
            ],
            ..Schema::default()
        };
        program_with_schema(schema, vec![], row_sources, returning_zero())
    }

    fn read_u_keyed_by_row_c() -> Effect {
        Effect::Read {
            table: table("u"),
            key: vec![project(Slot(2), "c")],
            into: Slot(3),
        }
    }

    #[test]
    fn a_projection_from_the_row_of_an_iteration_over_one_table_is_rejected() {
        let program = iteration_row_program(vec![
            range_over(
                "text_rows",
                0,
                Expr::Literal(text("a")),
                Expr::Literal(text("z")),
            ),
            iterate(0, 2, vec![read_u_keyed_by_row_c()]),
        ]);

        assert_eq!(
            program.validate(),
            Err(ProgramError::Function {
                function: FunctionName("under_test".to_owned()),
                fault: FunctionFault::KeyComponent {
                    table: table("u"),
                    column: column("c"),
                    fault: KeyComponentFault::WrongKind
                }
            })
        );
    }

    #[test]
    fn a_projection_from_the_row_of_an_iteration_whose_source_a_later_read_rebinds_is_left_to_the_run(
    ) {
        let program = iteration_row_program(vec![
            range_over(
                "text_rows",
                0,
                Expr::Literal(text("a")),
                Expr::Literal(text("z")),
            ),
            range_over("outer", 1, literal(0), literal(9)),
            iterate(
                1,
                4,
                vec![
                    iterate(0, 2, vec![read_u_keyed_by_row_c()]),
                    range_over("int_rows", 0, literal(0), literal(9)),
                ],
            ),
        ]);
        let store = Store::containing([("outer", 1), ("outer", 2), ("int_rows", 1)].map(
            |(table_name, c)| {
                let row = Row {
                    columns: columns_of_values([("c", integer(c))]),
                };
                (table(table_name), Key(vec![integer(c)]), row)
            },
        ));

        let execution = run_ok(&program, &call_under_test(vec![]), &store);

        assert_eq!(program.validate(), Ok(()));
        assert_eq!(committed_response(&execution), &integer(0));
    }
}
