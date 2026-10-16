use std::collections::BTreeSet;

use kiln_e1::{classify_function, Classification, ClassifyError, UnknownCause};
use kiln_ir::{
    run, Block, Call, ColumnName, Condition, Effect, Expr, Function, FunctionFault, FunctionName,
    IntegerWidth, Key, Parameter, ParameterName, ParameterType, Program, ProgramError, Row, Slot,
    Store, TableName, Terminal, TextFormFault, Value,
};

use super::programs::*;

fn text(value: &str) -> Expr {
    Expr::Literal(Value::Text(value.to_owned()))
}

fn concat(left: Expr, right: Expr) -> Expr {
    Expr::Concat(Box::new(left), Box::new(right))
}

fn truncate(text: Expr, length: Expr) -> Expr {
    Expr::Truncate {
        text: Box::new(text),
        length: Box::new(length),
    }
}

fn contains(text: Expr, part: Expr) -> Condition {
    Condition::Contains { text, part }
}

fn when(condition: Condition, when_true: Expr, when_false: Expr) -> Expr {
    Expr::Conditional {
        condition: Box::new(condition),
        when_true: Box::new(when_true),
        when_false: Box::new(when_false),
    }
}

fn branch_on(effects: Vec<Effect>, condition: Condition, present: Block, absent: Block) -> Block {
    Block {
        effects,
        terminal: Terminal::Branch {
            condition,
            when_present: Box::new(present),
            when_absent: Box::new(absent),
        },
    }
}

fn read_label(key: Expr, into: u32) -> Effect {
    read(LABELS, key, into)
}

fn the_name_of_label_0() -> Expr {
    column_of(0, "name")
}

fn the_size_of_label_0() -> Expr {
    column_of(0, "size")
}

fn text_forms_of_a_read_result() -> Vec<Expr> {
    vec![
        concat(the_name_of_label_0(), param("suffix")),
        concat(param("prefix"), the_name_of_label_0()),
        truncate(the_name_of_label_0(), literal(5)),
        truncate(param("s"), the_size_of_label_0()),
        when(
            contains(the_name_of_label_0(), text("ORIGINAL")),
            param("a"),
            param("b"),
        ),
    ]
}

mod kiln_e1_44 {
    use super::*;

    fn after_a_label(key: Expr) -> Block {
        returns(vec![read_label(param("label"), 0), read_label(key, 1)])
    }

    #[test]
    fn a_key_built_from_arguments_and_literals_by_a_text_form_is_static() {
        let keys = [
            concat(param("prefix"), text("-x")),
            truncate(param("s"), literal(3)),
            truncate(concat(param("a"), param("b")), param("n")),
            when(contains(param("s"), text("A")), param("a"), param("b")),
        ];
        for key in keys {
            assert_eq!(category_of(returns(vec![read_label(key, 0)])), STATIC);
        }
    }

    #[test]
    fn a_key_built_from_a_read_result_by_a_text_form_is_unknown_as_computed_from_a_read() {
        for key in text_forms_of_a_read_result() {
            assert_unknown(
                after_a_label(key),
                UnknownCause::KeyComputedFromRead {
                    table: table(LABELS),
                },
            );
        }
    }

    #[test]
    fn a_containment_branch_over_arguments_alone_is_argument_selected() {
        let body = branch_on(
            vec![],
            contains(param("note"), text("ORIGINAL")),
            returns(vec![read("customers", param("c"), 1)]),
            returns(vec![read("orders", param("o"), 1)]),
        );

        assert_eq!(category_of(body), STATIC);
    }

    #[test]
    fn a_containment_branch_over_a_read_result_is_data_selected() {
        let body = branch_on(
            vec![read_label(param("label"), 0)],
            contains(the_name_of_label_0(), text("ORIGINAL")),
            returns(vec![read("customers", param("c"), 1)]),
            returns(vec![read("orders", param("o"), 1)]),
        );

        assert_unknown(
            body,
            UnknownCause::DataSelectedTargets {
                selector: "f/body/terminal".to_owned(),
            },
        );
    }

    /// Slot 1 is the row of an iteration that has ended, so it holds no row.
    fn released_name() -> Expr {
        column_of(1, "name")
    }

    /// Reads `labels` by `key` after slot 1 was released, then accesses `b`.
    fn reading_by(key: Expr) -> Block {
        returns(vec![
            read_range("orders", param("low"), param("high"), 0),
            iterate(0, 1, vec![]),
            read_label(key, 2),
            update("b", param("y"), &[]),
        ])
    }

    fn reaches_the_access_after(body: Block) -> bool {
        evidence_of(body)
            .iter()
            .any(|node| node == "f/body/effects[3]")
    }

    #[test]
    fn a_projection_in_a_text_form_ends_the_path_like_the_evaluator() {
        let keys = [
            concat(released_name(), text("x")),
            concat(text("x"), released_name()),
            truncate(released_name(), literal(2)),
            truncate(text("abc"), key_of(1)),
            when(contains(released_name(), text("x")), param("a"), param("b")),
            when(contains(text("x"), released_name()), param("a"), param("b")),
        ];
        for key in keys {
            assert!(
                !reaches_the_access_after(reading_by(key.clone())),
                "{key:?}"
            );
        }
    }

    #[test]
    fn a_text_form_aborting_on_one_arm_of_an_undecided_conditional_is_unknown_naming_the_access() {
        let key = when(
            contains(param("s"), text("A")),
            concat(released_name(), text("x")),
            param("b"),
        );

        let body = reading_by(key);

        assert_unknown(body.clone(), UnknownCause::ConditionalAbortsOnOneArm);
        assert_eq!(evidence_of(body), ["f/body/effects[2]"]);
    }

    #[test]
    fn a_containment_of_two_literals_chooses_its_arm_like_the_evaluator() {
        let aborting = || concat(released_name(), text("x"));
        let holds = when(contains(text("abc"), text("b")), param("a"), aborting());
        let fails = when(contains(text("abc"), text("z")), param("a"), aborting());

        assert!(reaches_the_access_after(reading_by(holds)));
        assert!(!reaches_the_access_after(reading_by(fails)));
    }

    /// Slot 0 holds a label (its `name` is text) on the present arm and a tag
    /// (its `name` is an integer) on the absent arm, so no program text
    /// decides what projecting `name` yields. Each arm then writes `value`
    /// into `a` and accesses `b`.
    fn label_or_tag_then(value: Expr) -> Block {
        let arm = |table: &str| {
            returns(vec![
                read(table, param("p"), 0),
                update("a", param("y"), &[("c", value.clone())]),
                update("b", param("y"), &[]),
            ])
        };
        branch_on(
            vec![],
            contains(param("s"), text("A")),
            arm(LABELS),
            arm(TAGS),
        )
    }

    /// Whether the present and the absent arm reach their access to `b`.
    fn arms_reaching_b(body: Block) -> [bool; 2] {
        let evidence = evidence_of(body);
        ["when_present", "when_absent"].map(|arm| {
            evidence
                .iter()
                .any(|node| node.contains(arm) && node.ends_with("effects[2]"))
        })
    }

    fn name_of_slot_0() -> Expr {
        column_of(0, "name")
    }

    #[test]
    fn a_projection_of_a_column_that_cannot_be_text_ends_only_the_paths_that_read_it() {
        let values = [
            concat(name_of_slot_0(), text("x")),
            concat(text("x"), name_of_slot_0()),
            truncate(name_of_slot_0(), literal(2)),
            when(
                contains(name_of_slot_0(), text("x")),
                param("a"),
                param("b"),
            ),
            when(
                contains(text("x"), name_of_slot_0()),
                param("a"),
                param("b"),
            ),
        ];
        for value in values {
            assert_eq!(
                arms_reaching_b(label_or_tag_then(value.clone())),
                [true, false]
            );
        }
    }

    #[test]
    fn a_projection_of_a_column_that_cannot_be_an_integer_length_ends_only_the_paths_that_read_it()
    {
        let value = truncate(text("abc"), name_of_slot_0());

        assert_eq!(arms_reaching_b(label_or_tag_then(value)), [false, true]);
    }

    #[test]
    fn a_projection_of_the_wrong_kind_on_one_arm_of_an_undecided_conditional_is_unknown() {
        let value = concat(
            when(
                contains(param("s2"), text("A")),
                name_of_slot_0(),
                text("z"),
            ),
            text("x"),
        );

        assert_unknown(
            label_or_tag_then(value),
            UnknownCause::ConditionalAbortsOnOneArm,
        );
    }

    fn text_parameter(name: &str) -> Parameter {
        Parameter {
            name: ParameterName(name.to_owned()),
            kind: ParameterType::Text,
        }
    }

    fn call_of(arguments: Vec<Value>) -> Call {
        Call {
            function: FunctionName(FUNCTION.to_owned()),
            args: arguments,
        }
    }

    fn row_named(name: Value) -> Row {
        Row {
            columns: [(ColumnName("name".to_owned()), name)].into(),
        }
    }

    fn tables_accessed(program: &Program, call: &Call, store: &Store) -> BTreeSet<TableName> {
        let execution = run(program, call, store).unwrap();
        execution
            .access_log
            .into_iter()
            .map(|entry| entry.table)
            .collect()
    }

    /// The selector's label is read into slot 1, which both arms rebind to
    /// a tag, whose integer `name` is no text to truncate. Both arms insert
    /// into `a` and `audit` around the insert into `b` that fails, in
    /// opposite orders: only the inserts before the failure are made.
    fn truncating_a_tag_name_into_a_write() -> Program {
        let arm = |before: &str, after: &str| {
            returns(vec![
                read(TAGS, param("p"), 1),
                insert(before, param("n")),
                insert_writing(
                    "b",
                    param("n"),
                    &[("c", truncate(column_of(1, "name"), literal(1)))],
                ),
                insert(after, param("n")),
            ])
        };
        let body = branch_on(
            vec![read_label(param("label"), 1)],
            contains(column_of(1, "name"), text("x")),
            arm("a", "audit"),
            arm("audit", "a"),
        );
        let parameters = vec![
            text_parameter("label"),
            text_parameter("p"),
            Parameter {
                name: ParameterName("n".to_owned()),
                kind: ParameterType::Integer(IntegerWidth::I64),
            },
        ];
        Program {
            schema: schema(),
            functions: vec![Function {
                name: FunctionName(FUNCTION.to_owned()),
                parameters,
                body,
            }],
        }
    }

    #[test]
    fn a_function_whose_text_form_aborts_on_a_path_the_data_selects_is_not_static() {
        let program = truncating_a_tag_name_into_a_write();
        let store = Store::containing([
            (
                table(LABELS),
                Key(vec![Value::Text("L".to_owned())]),
                row_named(Value::Text("xy".to_owned())),
            ),
            (
                table(LABELS),
                Key(vec![Value::Text("M".to_owned())]),
                row_named(Value::Text("plain".to_owned())),
            ),
            (
                table(TAGS),
                Key(vec![Value::Text("T".to_owned())]),
                row_named(Value::Integer(5)),
            ),
        ]);
        let text_argument = |value: &str| Value::Text(value.to_owned());
        let call_selecting = |label: &str| {
            call_of(vec![
                text_argument(label),
                text_argument("T"),
                Value::Integer(1),
            ])
        };

        let present_reaches = tables_accessed(&program, &call_selecting("L"), &store);
        let absent_reaches = tables_accessed(&program, &call_selecting("M"), &store);
        let classification =
            classify_function(&program, &FunctionName(FUNCTION.to_owned())).unwrap();

        assert!(
            !present_reaches.is_subset(&absent_reaches)
                && !absent_reaches.is_subset(&present_reaches)
        );
        assert!(
            matches!(
                classification.classification,
                Classification::Unknown { .. }
            ),
            "{:?}",
            classification.classification
        );
    }

    fn rejected_as(body: Block, fault: TextFormFault) {
        let function = FunctionName(FUNCTION.to_owned());
        assert_eq!(
            classify_function(&program(body), &function),
            Err(ClassifyError::InvalidProgram(ProgramError::Function {
                function,
                fault: FunctionFault::TextForm(fault),
            }))
        );
    }

    #[test]
    fn a_text_form_that_certainly_aborts_is_reported_not_classified() {
        let after_label = |key: Expr| {
            branch_on(
                vec![read_label(param("label"), 0)],
                contains(name_of_slot_0(), text("x")),
                returns(vec![
                    update("a", param("x"), &[]),
                    read_label(key.clone(), 1),
                    update("b", param("x"), &[]),
                ]),
                returns(vec![
                    update("b", param("x"), &[]),
                    read_label(key, 1),
                    update("a", param("x"), &[]),
                ]),
            )
        };

        rejected_as(
            after_label(truncate(text("x"), literal(-1))),
            TextFormFault::NegativeLength,
        );
        rejected_as(
            after_label(concat(literal(1), text("x"))),
            TextFormFault::OperandNotText,
        );
    }
}

mod kiln_e1_45 {
    use super::*;

    /// Slot 3 is read before the loop and again at the end of the loop body,
    /// so the body's key carries it from one iteration to the next.
    fn loop_reading_slot_3_by(key: Expr) -> Block {
        returns(vec![
            read_label(param("s"), 3),
            read_range("ledger", param("low"), param("high"), 1),
            iterate(1, 2, vec![read_label(key, 4), read_label(param("k"), 3)]),
        ])
    }

    fn name_of_slot_3() -> Expr {
        column_of(3, "name")
    }

    #[test]
    fn a_slot_a_text_form_reads_before_the_body_binds_it_is_carried() {
        let keys = [
            concat(name_of_slot_3(), param("x")),
            concat(param("x"), name_of_slot_3()),
            truncate(name_of_slot_3(), literal(2)),
            truncate(param("x"), column_of(3, "size")),
            when(
                contains(name_of_slot_3(), text("A")),
                param("a"),
                param("b"),
            ),
            when(
                contains(param("x"), name_of_slot_3()),
                param("a"),
                param("b"),
            ),
        ];
        for key in keys {
            assert_unknown(
                loop_reading_slot_3_by(key),
                UnknownCause::SlotCarriedAcrossIterations { slot: Slot(3) },
            );
        }
    }
}
