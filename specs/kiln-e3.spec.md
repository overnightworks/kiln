# `kiln-e3`

`crates/kiln-e3` owns E3's instrument: the candidate term, the family context a
checker form is checked against, and checker-form validation. This file owns the
observable behavior of that crate. It cites the freeze rather than restating it:
`experiments/e3-summary-synthesis/grammar.md` (sections 1.1 to 1.3) and
`protocol.md` (section 3.1, checks 1 to 3). A test claims a criterion by living
in the module named for it: `mod kiln_e3_02 { #[test] fn ... }` claims
`KILN-E3-02`. One criterion per module; several tests may share it. The crate
evaluates nothing; this slice builds the quota family only.

## Quota family and checker-form validation

- [KILN-E3-01] The quota family context holds `kiln_e2::quota::program()`, a signature of one `Int` component `remaining`, the literal pool read from the program and the `Name` set copied from grammar section 1.2's frozen table, nothing derived from the program or a store. The pool is the program's body literals, check operands, the width bounds of non-key value columns and parameters, `0` and `1`; key columns contribute nothing, so quota's pool is exactly `{0, 1, 50, 1000, i32::MIN, i32::MAX}`. The set is the three declared aborts, `Overflow(Arithmetic)`, `Overflow(Width{quota, remaining, key 0})`, `CheckViolation{quota.remaining AtLeast 0, key 0}` and `CheckViolation{quota.remaining AtMost 1000, key 0}`; the width and check-violation names are marked checker-only.
- [KILN-E3-02] A hand-written quota checker form validates, in a variant with a `shift` component and in one with a `clamp-shift` component, each with the full functions `allocate` and `refill`; a `case` with one arm per category value, an `ite` over `Int`, `Cat`, `Opt` or `Outcome`, a `per-key(fill)` component whose apply is given the key's row, and a whole-keyed apply returning a `keyed` literal validate.
- [KILN-E3-03] A checker form missing a declared part is rejected with its own typed `Malformed` reason and the path of the part, never a panic: a missing combine, apply, `valid` or `respond`, a wrong number of components, functions or atom terms, a carrier field of a sort the language has no field for or of an undeclared category, `per-key` over a component that is not keyed. A carrier with a field of each supported sort (`Int`, `Bool`, `Opt`, `Cat`, `Map`) validates.
- [KILN-E3-04] A checker form with an ill-typed or non-total term is rejected with its own typed `Malformed` reason and the path of the offending node, never a panic. The checks run in protocol section 3.1's order over the whole form and the first failing check reports: undeclared references (parameter, state component, category, category value, `Name`, an `abort` outside the name set), then ill-typed terms (an `Int + Bool`, an unknown field, a non-positive modulus, an `ite` over names, maps, `Bool`s or carriers, a keyed state read as a value, a `per-key` atom that is not an `Entry`, a whole-keyed apply that is not a `keyed` literal), then non-total ones (a `case` with a missing arm, a table or keyed literal with a missing entry). A term used where nothing binds it is rejected: `x` outside an apply, `key` outside the entry of a `per-key` atom, `when` outside a `per-key` atom, `state` in `valid` or an atom, an operand outside its combine or apply.
