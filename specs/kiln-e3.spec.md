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

- [KILN-E3-01] The quota family context holds `kiln_e2::quota::program()`, a signature of one `Int` component `remaining`, the literal pool read from the program (its literals, the declared column and parameter width bounds, `0` and `1`) and the `Name` set: the three declared aborts and the width overflow of `quota.remaining`.
- [KILN-E3-02] A hand-written quota checker form validates, in a variant with a `shift` component and in one with a `clamp-shift` component, each with the full functions `allocate` and `refill`; a `case` with one arm per category value validates.
- [KILN-E3-03] A checker form missing a declared part is rejected with its own typed `Malformed` reason and the path of the part, never a panic: a missing combine, apply, `valid` or `respond`, a wrong number of components, functions or atom terms, a carrier field of a sort the language has no field for or of an undeclared category.
- [KILN-E3-04] A checker form with an ill-typed or non-total term is rejected with its own typed `Malformed` reason and the path of the offending node, never a panic: an `Int + Bool`, an unknown parameter, state component, field or category, a `Name` or `abort` outside the family's `Name` set, a `case` with a missing arm, a table or keyed literal with a missing entry, a non-positive modulus, and a term used where nothing binds it (`x` outside an apply, `key` outside an entry, an operand outside its combine or apply).
