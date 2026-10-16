//! The typed result of running one batch: what the hand-written folds
//! produce and what the oracle's replay is converted into, so the two are
//! compared like with like.

use kiln_ir::{AbortName, AbortReason, Outcome, Value};

/// An abort a family declares by name.
pub trait DeclaredAbort: Sized {
    fn named(name: &AbortName) -> Option<Self>;
}

/// Defines a family's abort enum with the frozen name of each variant.
macro_rules! declared_aborts {
    ($(#[$meta:meta])* $enum:ident { $($variant:ident => $name:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $enum {
            $($variant),+
        }

        impl $enum {
            /// The frozen name the program declares for this abort.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }
        }

        impl $crate::run::DeclaredAbort for $enum {
            fn named(name: &kiln_ir::AbortName) -> Option<Self> {
                match name.0.as_str() {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}
pub(crate) use declared_aborts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abort<N> {
    /// One of the family's declared aborts.
    Declared(N),
    /// Any other abort of the oracle, with its payload: a fold never yields
    /// one, so its presence is itself a divergence.
    Structural(AbortReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallOutcome<N> {
    Committed(Value),
    Aborted(Abort<N>),
}

impl<N> CallOutcome<N> {
    pub fn committed(response: i64) -> Self {
        Self::Committed(Value::Integer(response))
    }

    pub fn declared(abort: N) -> Self {
        Self::Aborted(Abort::Declared(abort))
    }
}

impl<N: DeclaredAbort> From<&Outcome> for CallOutcome<N> {
    fn from(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::Committed { response } => Self::Committed(response.clone()),
            Outcome::Aborted {
                reason: AbortReason::Declared(name),
            } => N::named(name).map_or_else(
                || Self::Aborted(Abort::Structural(AbortReason::Declared(name.clone()))),
                Self::declared,
            ),
            Outcome::Aborted { reason } => Self::Aborted(Abort::Structural(reason.clone())),
        }
    }
}

/// Every call's outcome in order, and the state after the last call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run<N, S> {
    pub outcomes: Vec<CallOutcome<N>>,
    pub final_state: S,
}

impl<N, S: Copy> Run<N, S> {
    /// Applies `calls` in order from `initial`, one `step` each.
    pub fn fold<C: Copy>(initial: S, calls: &[C], step: fn(S, C) -> (S, CallOutcome<N>)) -> Self {
        let mut state = initial;
        let outcomes = calls
            .iter()
            .map(|call| {
                let (next, outcome) = step(state, *call);
                state = next;
                outcome
            })
            .collect();
        Self {
            outcomes,
            final_state: state,
        }
    }
}
