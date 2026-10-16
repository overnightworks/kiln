use kiln_e1::UnknownCause;
use kiln_ir::{Block, Condition, Effect, Expr, Slot, Terminal, Value};

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

    #[test]
    fn a_containment_of_literals_of_another_kind_decides_nothing() {
        let key = when(
            contains(literal(1), text("b")),
            param("a"),
            concat(released_name(), text("x")),
        );

        assert_unknown(reading_by(key), UnknownCause::ConditionalAbortsOnOneArm);
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
