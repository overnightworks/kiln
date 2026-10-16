use kiln_ir::Parameter;

use super::structure::Layout;
use crate::family::Family;
use crate::malformed::{Malformed, Path, Step};
use crate::term::Term;

/// Check 1 of protocol §3.1 for the terms: every parameter, state component,
/// `Name`, category and category value a term mentions is declared. It looks
/// at no types, so a type error elsewhere cannot hide an undeclared reference.
pub(super) fn declared(family: &Family, layout: &Layout<'_>) -> Result<(), Malformed> {
    for (index, component) in layout.components.iter().enumerate() {
        let mut references = References::new(family, &[], Step::Component(index));
        references.rooted(Step::Combine, component.combine)?;
        references.rooted(Step::Apply, component.apply)?;
    }
    for (index, function) in layout.functions.iter().enumerate() {
        let mut references = References::new(family, function.parameters, Step::Function(index));
        references.rooted(Step::Valid, function.valid)?;
        for (atom_index, atom) in function.atoms.iter().enumerate() {
            references.rooted(Step::Atom(atom_index), atom)?;
        }
        references.rooted(Step::Respond, function.respond)?;
    }
    Ok(())
}

struct References<'a> {
    family: &'a Family,
    parameters: &'a [Parameter],
    path: Vec<Step>,
}

impl<'a> References<'a> {
    fn new(family: &'a Family, parameters: &'a [Parameter], root: Step) -> Self {
        Self {
            family,
            parameters,
            path: vec![root],
        }
    }

    fn rooted(&mut self, step: Step, term: &Term) -> Result<(), Malformed> {
        self.path.push(step);
        let result = self.term(term);
        self.path.pop();
        result
    }

    fn here(&self) -> Path {
        Path(self.path.clone())
    }

    fn term(&mut self, term: &Term) -> Result<(), Malformed> {
        if let Term::Abort(reason) = term {
            if let Term::Name(name) = reason.as_ref() {
                return self.abort_name(name);
            }
        }
        self.mention(term)?;
        for (index, child) in term.children().into_iter().enumerate() {
            self.rooted(Step::Child(index), child)?;
        }
        Ok(())
    }

    fn abort_name(&self, name: &kiln_ir::AbortReason) -> Result<(), Malformed> {
        if self.family.has_name(name) {
            Ok(())
        } else {
            Err(Malformed::AbortOutsideNameSet {
                path: self.here(),
                name: Box::new(name.clone()),
            })
        }
    }

    fn mention(&self, term: &Term) -> Result<(), Malformed> {
        match term {
            Term::Param(name) => self.parameter(name),
            Term::State(name)
            | Term::StateRow {
                component: name, ..
            } => self.component(name),
            Term::Name(name) if !self.family.has_name(name) => Err(Malformed::UnknownName {
                path: self.here(),
                name: Box::new(name.clone()),
            }),
            Term::CategoryValue { category, code } => {
                if *code < self.category_size(category)? {
                    Ok(())
                } else {
                    Err(Malformed::CategoryValueOutOfRange {
                        path: self.here(),
                        category: category.clone(),
                        code: *code,
                    })
                }
            }
            Term::Of { category, .. }
            | Term::Table { category, .. }
            | Term::Keyed { category, .. } => self.category_size(category).map(drop),
            _ => Ok(()),
        }
    }

    fn parameter(&self, name: &str) -> Result<(), Malformed> {
        if self
            .parameters
            .iter()
            .any(|parameter| parameter.name.0 == name)
        {
            Ok(())
        } else {
            Err(Malformed::UnknownParameter {
                path: self.here(),
                name: name.to_owned(),
            })
        }
    }

    fn component(&self, name: &str) -> Result<(), Malformed> {
        if self.family.signature().component(name).is_some() {
            Ok(())
        } else {
            Err(Malformed::UnknownStateComponent {
                path: self.here(),
                name: name.to_owned(),
            })
        }
    }

    fn category_size(&self, name: &str) -> Result<usize, Malformed> {
        self.family
            .signature()
            .category(name)
            .map(|category| category.size)
            .ok_or_else(|| Malformed::UnknownCategory {
                path: self.here(),
                name: name.to_owned(),
            })
    }
}
