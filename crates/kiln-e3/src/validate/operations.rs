use super::structure::AppliedInput;
use super::walker::{Checked, Walker};
use crate::malformed::{Binding, Malformed, Path};
use crate::sort::{Expected, Sort};
use crate::term::{Operand, Term};

const NOT_DISPATCHED: &str = "infer sends each term to the group that handles it";

fn binding_of(operand: Operand) -> Binding {
    match operand {
        Operand::Left => Binding::LeftOperand,
        Operand::Right => Binding::RightOperand,
        Operand::Summary => Binding::SummaryOperand,
    }
}

impl Walker<'_> {
    pub(super) fn leaf(&mut self, term: &Term) -> Checked<Sort> {
        match term {
            Term::Lit(_) => Ok(Sort::Int),
            Term::True | Term::False => Ok(Sort::Bool),
            Term::Absent => Ok(Sort::Opt),
            Term::Param(_) => Ok(Sort::Int),
            Term::Key if self.key_bound() => Ok(Sort::Int),
            Term::Key => Err(self.not_bound(Binding::Key)),
            Term::State(name) => match self.state_kind(name)? {
                Sort::Keyed(category) => Err(self.no_production(Sort::Keyed(category))),
                scalar => Ok(scalar),
            },
            Term::Applied => match self.applied() {
                Some(AppliedInput::Value(sort)) => Ok(sort.clone()),
                Some(AppliedInput::Rows(_)) | None => Err(self.not_bound(Binding::Applied)),
            },
            Term::Field { operand, field } => self.field(*operand, field),
            Term::CategoryValue { category, .. } => Ok(Sort::Cat(category.clone())),
            Term::Name(_) => Ok(Sort::Name),
            _ => unreachable!("{NOT_DISPATCHED}"),
        }
    }

    pub(super) fn no_production(&self, sort: Sort) -> Malformed {
        Malformed::NoProduction {
            path: self.here(),
            sort,
        }
    }

    fn state_kind(&self, name: &str) -> Checked<Sort> {
        let component = self
            .family
            .signature()
            .component(name)
            .expect("check 1 ran first");
        if self.state_bound() {
            Ok(component.kind.clone())
        } else {
            Err(self.not_bound(Binding::State))
        }
    }

    fn field(&self, operand: Operand, name: &str) -> Checked<Sort> {
        let fields = self
            .operand_fields(operand)
            .ok_or_else(|| self.not_bound(binding_of(operand)))?;
        fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.sort.clone())
            .ok_or_else(|| Malformed::UnknownField {
                path: self.here(),
                name: name.to_owned(),
            })
    }

    pub(super) fn integer_operation(&mut self, term: &Term) -> Checked<Sort> {
        match term {
            Term::Code(category) => {
                self.category_of(0, category)?;
            }
            Term::Get(row, default) => {
                self.expect(0, row, &Sort::Opt)?;
                self.expect(1, default, &Sort::Int)?;
            }
            Term::Add(left, right)
            | Term::Sub(left, right)
            | Term::Min(left, right)
            | Term::Max(left, right) => self.expect_all(&[left, right], &Sort::Int)?,
            Term::Clamp(value, low, high) => self.expect_all(&[value, low, high], &Sort::Int)?,
            Term::Mod(value, modulus) => {
                if *modulus <= 0 {
                    return Err(Malformed::ModulusNotPositive {
                        path: self.here(),
                        modulus: *modulus,
                    });
                }
                self.expect(0, value, &Sort::Int)?;
            }
            _ => unreachable!("{NOT_DISPATCHED}"),
        }
        Ok(Sort::Int)
    }

    pub(super) fn boolean_operation(&mut self, term: &Term) -> Checked<Sort> {
        match term {
            Term::Not(inner) => self.expect(0, inner, &Sort::Bool)?,
            Term::And(left, right) | Term::Or(left, right) => {
                self.expect_all(&[left, right], &Sort::Bool)?;
            }
            Term::Compare(left, _, right) => self.expect_all(&[left, right], &Sort::Int)?,
            Term::CategoryEqual(left, right) => {
                let category = self.category_of(0, left)?;
                self.expect(1, right, &Sort::Cat(category))?;
            }
            Term::IsPresent(row) => self.expect(0, row, &Sort::Opt)?,
            _ => unreachable!("{NOT_DISPATCHED}"),
        }
        Ok(Sort::Bool)
    }

    pub(super) fn structured_operation(&mut self, term: &Term) -> Checked<Sort> {
        match term {
            Term::Lookup(map, key) => {
                let category = self.map_of(0, map)?;
                self.expect(1, key, &Sort::Cat(category.clone()))?;
                Ok(Sort::Cat(category))
            }
            Term::Of { category, code } => {
                self.expect(0, code, &Sort::Int)?;
                Ok(Sort::Cat(category.clone()))
            }
            Term::Held(owner) => {
                self.expect(0, owner, &Sort::Int)?;
                Ok(Sort::Opt)
            }
            Term::StateRow { component, key } => {
                let kind = self.state_kind(component)?;
                self.keyed_row(kind, key)
            }
            Term::AppliedRow(key) => match self.applied() {
                Some(AppliedInput::Rows(category)) => {
                    self.expect(0, key, &Sort::Cat(category.clone()))?;
                    Ok(Sort::Opt)
                }
                Some(AppliedInput::Value(sort)) => {
                    Err(self.mismatch(Expected::Keyed, sort.clone()))
                }
                None => Err(self.not_bound(Binding::Applied)),
            },
            Term::Keyed { category, rows } => {
                self.parts(category, rows, &Sort::Opt, |path, expected, found| {
                    Malformed::KeyedRowCountMismatch {
                        path,
                        expected,
                        found,
                    }
                })?;
                Ok(Sort::Keyed(category.clone()))
            }
            Term::Table { category, entries } => {
                let entry = Sort::Cat(category.clone());
                self.parts(category, entries, &entry, |path, expected, found| {
                    Malformed::TableEntryCountMismatch {
                        path,
                        expected,
                        found,
                    }
                })?;
                Ok(Sort::Map(category.clone()))
            }
            _ => unreachable!("{NOT_DISPATCHED}"),
        }
    }

    fn keyed_row(&mut self, kind: Sort, key: &Term) -> Checked<Sort> {
        let Sort::Keyed(category) = kind else {
            return Err(self.mismatch(Expected::Keyed, kind));
        };
        self.expect(0, key, &Sort::Cat(category))?;
        Ok(Sort::Opt)
    }

    /// One term per value of `category`, each of sort `part`.
    fn parts(
        &mut self,
        category: &str,
        terms: &[Term],
        part: &Sort,
        count_mismatch: fn(Path, usize, usize) -> Malformed,
    ) -> Checked<()> {
        let size = self.category_size(category);
        if terms.len() != size {
            self.defer(count_mismatch(self.here(), size, terms.len()));
        }
        for (index, term) in terms.iter().enumerate() {
            self.expect(index, term, part)?;
        }
        Ok(())
    }

    pub(super) fn control_operation(&mut self, term: &Term) -> Checked<Sort> {
        match term {
            Term::Ite(condition, when_true, when_false) => {
                self.expect(0, condition, &Sort::Bool)?;
                let sort = self.child(1, when_true)?;
                if !matches!(sort, Sort::Int | Sort::Cat(_) | Sort::Opt | Sort::Outcome) {
                    return Err(self.no_production(sort));
                }
                self.expect(2, when_false, &sort)?;
                Ok(sort)
            }
            Term::Commit(response) => {
                self.expect(0, response, &Sort::Int)?;
                Ok(Sort::Outcome)
            }
            Term::Abort(reason) => self.abort(reason),
            Term::Case { scrutinee, arms } => self.case(scrutinee, arms),
            Term::Tuple(parts) => {
                self.each_child(0, parts)?;
                Ok(Sort::Carrier)
            }
            Term::Whole(operand) => self.whole(*operand),
            Term::When(..) => Err(self.not_bound(Binding::Entry)),
            _ => unreachable!("{NOT_DISPATCHED}"),
        }
    }

    fn abort(&mut self, reason: &Term) -> Checked<Sort> {
        self.expect(0, reason, &Sort::Name)?;
        Ok(Sort::Outcome)
    }

    fn case(&mut self, scrutinee: &Term, arms: &[Term]) -> Checked<Sort> {
        let category = self.category_of(0, scrutinee)?;
        let size = self.category_size(&category);
        if arms.len() != size {
            self.defer(Malformed::ArmCountMismatch {
                path: self.here(),
                expected: size,
                found: arms.len(),
            });
        }
        for (index, arm) in arms.iter().enumerate() {
            self.expect(1 + index, arm, &Sort::Outcome)?;
        }
        Ok(Sort::Outcome)
    }

    fn whole(&self, operand: Operand) -> Checked<Sort> {
        match operand {
            Operand::Summary => Err(self.not_bound(Binding::SummaryOperand)),
            Operand::Left | Operand::Right => self
                .operand_fields(operand)
                .map(|_| Sort::Carrier)
                .ok_or_else(|| self.not_bound(binding_of(operand))),
        }
    }
}
