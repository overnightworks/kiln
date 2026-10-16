use super::structure::{AppliedInput, ComponentLayout, FunctionLayout, Layout};
use crate::family::Family;
use crate::form::{ComponentShape, Field};
use crate::malformed::{Binding, Malformed, Path, Step};
use crate::sort::{Expected, Sort};
use crate::term::{Operand, Term};

pub(super) type Checked<T> = Result<T, Malformed>;

/// Check 1 already holds, so every name a term mentions resolves.
const DECLARED: &str = "check 1 ran first";

/// Which term of a function is being walked: each binds different names.
#[derive(Clone, Copy)]
enum FunctionTerm {
    Valid,
    Atom,
    Respond,
}

/// Where a term is written, which decides what it may refer to.
#[derive(Clone, Copy)]
enum Place<'a> {
    Combine {
        fields: &'a [Field],
    },
    Apply {
        fields: &'a [Field],
        applied: &'a AppliedInput,
    },
    Function(FunctionTerm),
}

/// Checks 2 and 3 of protocol §3.1 over every term of the layout, with check
/// 2's first defect winning over check 3's: a count that is not total is
/// kept aside while the walk goes on looking for a type error.
pub(super) fn well_typed_and_total<'a>(family: &'a Family, layout: &'a Layout<'a>) -> Checked<()> {
    let mut walker = Walker::new(family);
    for (index, component) in layout.components.iter().enumerate() {
        walker.at(Step::Component(index), |walker| walker.component(component))?;
    }
    for (index, function) in layout.functions.iter().enumerate() {
        walker.at(Step::Function(index), |walker| {
            walker.function(function, layout)
        })?;
    }
    walker.deferred.map_or(Ok(()), Err)
}

pub(super) struct Walker<'a> {
    pub(super) family: &'a Family,
    place: Place<'a>,
    path: Vec<Step>,
    in_entry: bool,
    deferred: Option<Malformed>,
}

impl<'a> Walker<'a> {
    fn new(family: &'a Family) -> Self {
        Self {
            family,
            place: Place::Function(FunctionTerm::Valid),
            path: Vec::new(),
            in_entry: false,
            deferred: None,
        }
    }

    fn component(&mut self, component: &'a ComponentLayout<'a>) -> Checked<()> {
        self.place = Place::Combine {
            fields: component.fields,
        };
        self.at(Step::Combine, |walker| {
            walker.carrier(component.combine, component.fields)
        })?;
        self.place = Place::Apply {
            fields: component.fields,
            applied: &component.applied,
        };
        self.at(Step::Apply, |walker| {
            let found = walker.infer(component.apply)?;
            walker.require(&Expected::Sort(component.applied.result_sort()), found)
        })
    }

    fn function(&mut self, function: &FunctionLayout<'a>, layout: &Layout<'a>) -> Checked<()> {
        self.place = Place::Function(FunctionTerm::Valid);
        self.at(Step::Valid, |walker| {
            walker.expect_here(function.valid, &Sort::Bool)
        })?;
        for (index, (atom, component)) in function.atoms.iter().zip(&layout.components).enumerate()
        {
            self.place = Place::Function(FunctionTerm::Atom);
            self.at(Step::Atom(index), |walker| match component.shape {
                ComponentShape::Whole => walker.carrier(atom, component.fields),
                ComponentShape::PerKey => walker.entry(atom, component.fields),
            })?;
        }
        self.place = Place::Function(FunctionTerm::Respond);
        self.at(Step::Respond, |walker| {
            walker.expect_here(function.respond, &Sort::Outcome)
        })
    }

    /// A defect of check 3, kept unless an earlier one is.
    pub(super) fn defer(&mut self, defect: Malformed) {
        self.deferred.get_or_insert(defect);
    }

    pub(super) fn here(&self) -> Path {
        Path(self.path.clone())
    }

    pub(super) fn at<T>(&mut self, step: Step, check: impl FnOnce(&mut Self) -> T) -> T {
        self.path.push(step);
        let result = check(self);
        self.path.pop();
        result
    }

    pub(super) fn child(&mut self, index: usize, term: &Term) -> Checked<Sort> {
        self.at(Step::Child(index), |walker| walker.infer(term))
    }

    pub(super) fn require(&self, expected: &Expected, found: Sort) -> Checked<()> {
        let holds = match expected {
            Expected::Sort(sort) => *sort == found,
            Expected::AnyCategory => matches!(found, Sort::Cat(_)),
            Expected::AnyMap => matches!(found, Sort::Map(_)),
            Expected::Keyed => matches!(found, Sort::Keyed(_)),
            Expected::Entry => false,
        };
        if holds {
            Ok(())
        } else {
            Err(self.mismatch(expected.clone(), found))
        }
    }

    fn expect_here(&mut self, term: &Term, sort: &Sort) -> Checked<()> {
        let found = self.infer(term)?;
        self.require(&Expected::Sort(sort.clone()), found)
    }

    pub(super) fn expect(&mut self, index: usize, term: &Term, sort: &Sort) -> Checked<()> {
        self.at(Step::Child(index), |walker| walker.expect_here(term, sort))
    }

    pub(super) fn expect_all(&mut self, terms: &[&Term], sort: &Sort) -> Checked<()> {
        for (index, term) in terms.iter().enumerate() {
            self.expect(index, term, sort)?;
        }
        Ok(())
    }

    pub(super) fn mismatch(&self, expected: Expected, found: Sort) -> Malformed {
        Malformed::TypeMismatch {
            path: self.here(),
            expected,
            found,
        }
    }

    pub(super) fn category_of(&mut self, index: usize, term: &Term) -> Checked<String> {
        self.at(Step::Child(index), |walker| match walker.infer(term)? {
            Sort::Cat(category) => Ok(category),
            other => Err(walker.mismatch(Expected::AnyCategory, other)),
        })
    }

    pub(super) fn map_of(&mut self, index: usize, term: &Term) -> Checked<String> {
        self.at(Step::Child(index), |walker| match walker.infer(term)? {
            Sort::Map(category) => Ok(category),
            other => Err(walker.mismatch(Expected::AnyMap, other)),
        })
    }

    pub(super) fn each_child(&mut self, first: usize, terms: &[Term]) -> Checked<()> {
        for (index, term) in terms.iter().enumerate() {
            self.child(first + index, term)?;
        }
        Ok(())
    }

    pub(super) fn not_bound(&self, term: Binding) -> Malformed {
        Malformed::NotBound {
            path: self.here(),
            term,
        }
    }

    pub(super) fn category_size(&self, name: &str) -> usize {
        self.family
            .signature()
            .category(name)
            .map(|category| category.size)
            .expect(DECLARED)
    }

    pub(super) fn state_bound(&self) -> bool {
        matches!(self.place, Place::Function(FunctionTerm::Respond))
    }

    pub(super) fn key_bound(&self) -> bool {
        self.in_entry
    }

    pub(super) fn applied(&self) -> Option<&'a AppliedInput> {
        match self.place {
            Place::Apply { applied, .. } => Some(applied),
            Place::Combine { .. } | Place::Function(_) => None,
        }
    }

    /// The carrier fields an operand of this place is read from.
    pub(super) fn operand_fields(&self, operand: Operand) -> Option<&'a [Field]> {
        match (self.place, operand) {
            (Place::Combine { fields }, Operand::Left | Operand::Right) => Some(fields),
            (Place::Apply { fields, .. }, Operand::Summary) => Some(fields),
            _ => None,
        }
    }

    /// A term in a carrier position: it must be a carrier value of `fields`.
    fn carrier(&mut self, term: &Term, fields: &[Field]) -> Checked<()> {
        match term {
            Term::Tuple(parts) => self.fields_of(parts, fields, 0),
            Term::Ite(condition, when_true, when_false) => {
                self.expect(0, condition, &Sort::Bool)?;
                self.at(Step::Child(1), |walker| walker.carrier(when_true, fields))?;
                self.at(Step::Child(2), |walker| walker.carrier(when_false, fields))
            }
            other => {
                let found = self.infer(other)?;
                self.require(&Expected::Sort(Sort::Carrier), found)
            }
        }
    }

    /// The atom of a `per-key` component: an `Entry`, the one place `when`
    /// and the `key` it binds belong to.
    fn entry(&mut self, term: &Term, fields: &[Field]) -> Checked<()> {
        let Term::When(condition, parts) = term else {
            let found = self.infer(term)?;
            return Err(self.mismatch(Expected::Entry, found));
        };
        self.in_entry = true;
        let checked = self
            .expect(0, condition, &Sort::Bool)
            .and_then(|()| self.fields_of(parts, fields, 1));
        self.in_entry = false;
        checked
    }

    fn fields_of(&mut self, parts: &[Term], fields: &[Field], first_child: usize) -> Checked<()> {
        if parts.len() != fields.len() {
            return Err(Malformed::TupleArityMismatch {
                path: self.here(),
                expected: fields.len(),
                found: parts.len(),
            });
        }
        for (index, (part, field)) in parts.iter().zip(fields).enumerate() {
            self.expect(first_child + index, part, &field.sort)?;
        }
        Ok(())
    }

    /// The sort of `term`, checking everything below it on the way.
    pub(super) fn infer(&mut self, term: &Term) -> Checked<Sort> {
        match term {
            Term::Lit(_)
            | Term::Param(_)
            | Term::Key
            | Term::State(_)
            | Term::Applied
            | Term::Field { .. }
            | Term::CategoryValue { .. }
            | Term::Name(_)
            | Term::Absent
            | Term::True
            | Term::False => self.leaf(term),
            Term::Code(_)
            | Term::Get(..)
            | Term::Add(..)
            | Term::Sub(..)
            | Term::Min(..)
            | Term::Max(..)
            | Term::Clamp(..)
            | Term::Mod(..) => self.integer_operation(term),
            Term::Not(_)
            | Term::And(..)
            | Term::Or(..)
            | Term::Compare(..)
            | Term::CategoryEqual(..)
            | Term::IsPresent(_) => self.boolean_operation(term),
            Term::Lookup(..)
            | Term::Of { .. }
            | Term::Held(_)
            | Term::StateRow { .. }
            | Term::AppliedRow(_)
            | Term::Keyed { .. }
            | Term::Table { .. } => self.structured_operation(term),
            Term::Ite(..)
            | Term::Commit(_)
            | Term::Abort(_)
            | Term::Case { .. }
            | Term::Tuple(_)
            | Term::Whole(_)
            | Term::When(..) => self.control_operation(term),
        }
    }
}
