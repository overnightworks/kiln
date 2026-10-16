//! Hand-written quota checker forms and the term builders they use.
#![allow(dead_code)]

use kiln_e2::quota::QuotaAbort;
use kiln_e2::quota::{program, QuotaState};
use kiln_e3::{
    validate, Category, CheckerForm, ComponentDef, Family, Field, FunctionDef, Malformed, Operand,
    Path, Signature, Sort, StateComponent, Step, Term,
};
use kiln_ir::{AbortName, AbortReason, Relation};

pub fn boxed(term: Term) -> Box<Term> {
    Box::new(term)
}

pub fn lit(number: i64) -> Term {
    Term::Lit(number)
}

pub fn add(left: Term, right: Term) -> Term {
    Term::Add(boxed(left), boxed(right))
}

pub fn sub(left: Term, right: Term) -> Term {
    Term::Sub(boxed(left), boxed(right))
}

pub fn min(left: Term, right: Term) -> Term {
    Term::Min(boxed(left), boxed(right))
}

pub fn clamp(value: Term, low: Term, high: Term) -> Term {
    Term::Clamp(boxed(value), boxed(low), boxed(high))
}

pub fn compare(left: Term, relation: Relation, right: Term) -> Term {
    Term::Compare(boxed(left), relation, boxed(right))
}

pub fn and(left: Term, right: Term) -> Term {
    Term::And(boxed(left), boxed(right))
}

pub fn not(term: Term) -> Term {
    Term::Not(boxed(term))
}

pub fn ite(condition: Term, when_true: Term, when_false: Term) -> Term {
    Term::Ite(boxed(condition), boxed(when_true), boxed(when_false))
}

pub fn commit(term: Term) -> Term {
    Term::Commit(boxed(term))
}

pub fn abort(reason: AbortReason) -> Term {
    Term::Abort(boxed(Term::Name(reason)))
}

pub fn declared(abort: QuotaAbort) -> AbortReason {
    AbortReason::Declared(AbortName(abort.name().to_owned()))
}

pub fn field(operand: Operand, name: &str) -> Term {
    Term::Field {
        operand,
        field: name.to_owned(),
    }
}

pub fn remaining() -> Term {
    Term::State("remaining".to_owned())
}

pub fn int_field(name: &str) -> Field {
    Field {
        name: name.to_owned(),
        sort: Sort::Int,
    }
}

/// The name of quota's single parameter, as the `Program` declares it.
pub fn quantity(family: &Family) -> Term {
    let function = &family.program().functions[0];
    Term::Param(function.parameters[0].name.0.clone())
}

#[derive(Clone, Copy)]
pub enum Variant {
    Shift,
    ClampShift,
}

fn shift_component() -> ComponentDef {
    ComponentDef {
        carrier_fields: vec![int_field("a")],
        combine: Some(Term::Tuple(vec![add(
            field(Operand::Left, "a"),
            field(Operand::Right, "a"),
        )])),
        apply: Some(add(Term::Applied, field(Operand::Summary, "a"))),
    }
}

fn clamp_shift_component() -> ComponentDef {
    let shifted_bound = |bound: &str| {
        clamp(
            add(field(Operand::Left, bound), field(Operand::Right, "a")),
            field(Operand::Right, "l"),
            field(Operand::Right, "u"),
        )
    };
    ComponentDef {
        carrier_fields: vec![int_field("a"), int_field("l"), int_field("u")],
        combine: Some(Term::Tuple(vec![
            add(field(Operand::Left, "a"), field(Operand::Right, "a")),
            shifted_bound("l"),
            shifted_bound("u"),
        ])),
        apply: Some(clamp(
            add(Term::Applied, field(Operand::Summary, "a")),
            field(Operand::Summary, "l"),
            field(Operand::Summary, "u"),
        )),
    }
}

fn quantity_in_range(family: &Family) -> Term {
    and(
        compare(lit(1), Relation::LessOrEqual, quantity(family)),
        compare(quantity(family), Relation::LessOrEqual, lit(50)),
    )
}

fn respond(family: &Family, empty: AbortReason, empty_at: i64, granted: Term) -> Term {
    ite(
        not(quantity_in_range(family)),
        abort(declared(QuotaAbort::InvalidQuantity)),
        ite(
            compare(remaining(), Relation::Equal, lit(empty_at)),
            abort(empty),
            commit(granted),
        ),
    )
}

fn atom(variant: Variant, shift: Term) -> Vec<Term> {
    match variant {
        Variant::Shift => vec![Term::Tuple(vec![shift])],
        Variant::ClampShift => vec![Term::Tuple(vec![shift, lit(0), lit(1000)])],
    }
}

/// Quota's checker form: `allocate` shifts down and `refill` up.
pub fn quota_form(variant: Variant) -> CheckerForm {
    let family = Family::quota();
    let component = match variant {
        Variant::Shift => shift_component(),
        Variant::ClampShift => clamp_shift_component(),
    };
    let allocate = FunctionDef {
        valid: Some(quantity_in_range(&family)),
        atom: atom(variant, sub(lit(0), quantity(&family))),
        respond: Some(respond(
            &family,
            declared(QuotaAbort::QuotaExhausted),
            0,
            min(quantity(&family), remaining()),
        )),
    };
    let refill = FunctionDef {
        valid: Some(quantity_in_range(&family)),
        atom: atom(variant, quantity(&family)),
        respond: Some(respond(
            &family,
            declared(QuotaAbort::QuotaFull),
            1000,
            min(quantity(&family), sub(lit(1000), remaining())),
        )),
    };
    CheckerForm {
        components: vec![component],
        functions: vec![allocate, refill],
    }
}

pub fn family_with_phase_category() -> Family {
    let signature = Signature {
        components: vec![StateComponent {
            name: "remaining".to_owned(),
            kind: Sort::Int,
        }],
        categories: vec![Category {
            name: "phase".to_owned(),
            size: 3,
        }],
    };
    Family::new(
        program(),
        signature,
        &QuotaState { remaining: 0 }.to_store(),
    )
}

pub fn path(steps: &[Step]) -> Path {
    Path(steps.to_vec())
}

pub fn with_form(change: impl FnOnce(&mut CheckerForm)) -> CheckerForm {
    let mut form = quota_form(Variant::ClampShift);
    change(&mut form);
    form
}

pub fn with_respond(respond: Term) -> CheckerForm {
    with_form(|form| form.functions[0].respond = Some(respond))
}

pub fn rejection(family: &Family, form: &CheckerForm) -> Malformed {
    validate(family, form).expect_err("the form is malformed")
}
