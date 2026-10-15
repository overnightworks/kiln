# `kiln-ir`

`crates/kiln-ir` is the reference interpreter for the IR. This file owns the
observable behavior of that crate: what `run` and `replay` return for a given
program, call, and store, and what the schema check, store, and access log
expose. It cites the reference meaning `docs/concept.md` §2.1-§2.2
(decisions D1-D7) owns rather than restating it. A test claims a criterion by
its function name: `fn kiln_ir_07__absent_key_is_branchable()` claims
`KILN-IR-07`.

## Declarations

- [KILN-IR-01] A clamping domain is accepted when its minimum is at most its maximum, including when they are equal, and rejected with a typed schema error when they are reversed.
- [KILN-IR-02] A clamping domain is accepted when it lies within its column's declared width, up to the exact width bounds, and rejected with a typed schema error when it reaches one past either bound.

## Invocation

- [KILN-IR-03] `run` returns the typed schema error for an invalid schema instead of panicking.
- [KILN-IR-04] A call whose argument does not fit its parameter's declared integer width, or whose argument kind (text or integer) differs from the parameter's declared type, aborts with a typed reason.
- [KILN-IR-05] A call whose arguments fit their parameters' declared widths and types commits.
- [KILN-IR-06] A parameter with no matching argument aborts the call with a typed reason.

## Reading

- [KILN-IR-07] A read sees the transaction's own earlier writes, and write effects apply in program order. Example: a write of 5 followed by a write of 9 to the same key leaves 9, and a read between them sees 5.
- [KILN-IR-08] A point read of an absent key is a value the transaction can branch on, like any other value.
- [KILN-IR-09] A range read returns every row whose key lies within its inclusive bounds, in key order, including a range that holds a single row.
- [KILN-IR-10] A range read whose start sorts after its end, or over a table with no rows, reads as an empty sequence.
- [KILN-IR-11] A range read that matches no row is branchable as an absence, the same as a point read of an absent key.
- [KILN-IR-12] A range read that exceeds its declared bound aborts the call and discards every effect the call produced before it.
- [KILN-IR-13] A value projected from an earlier read can be used as the key of a later read and as an arithmetic operand.
- [KILN-IR-14] A bounded iteration runs its body once per row in key order, and the body may use dependent reads and writes.
- [KILN-IR-15] Projecting from a slot whose read found no row, projecting a column the bound row lacks, or projecting from a range-bound slot outside its iteration aborts with a typed reason.
- [KILN-IR-16] A range bound or an arithmetic operand that resolves to a non-integer value aborts with a typed reason.

## Writing

- [KILN-IR-17] An update or delete that targets an absent row aborts; there is no implicit insert or no-op.
- [KILN-IR-18] Inserting an already existing key aborts.
- [KILN-IR-19] A successful delete removes the row, so a row inserted and then deleted leaves the store equal to the store before the insert.
- [KILN-IR-20] An update changes only the columns it names and preserves the row's other columns.
- [KILN-IR-21] An abort discards every effect the call produced before it, not only the operation that triggered it.
- [KILN-IR-22] A text value written into a column declared as an integer aborts with a typed reason.

## Arithmetic

- [KILN-IR-23] Values are checked against the column's declared width at its exact boundary for add and subtract, whether written by insert or update: the width's limit commits and one beyond it aborts.
- [KILN-IR-24] A computed value outside the target column's declared width aborts when the column declares no domain.
- [KILN-IR-25] A literal value outside the target column's declared width aborts when the column declares no domain.
- [KILN-IR-26] An `i64` overflow at any step of a nested expression aborts, even when the target column clamps.
- [KILN-IR-27] A nested expression whose intermediate result exceeds the column's width but whose final value fits commits.
- [KILN-IR-28] Arithmetic overflow in a response expression aborts with a typed reason.
- [KILN-IR-29] An overflow abort changes no state and keeps every access the call attempted.
- [KILN-IR-30] A column with a clamping domain clamps computed and literal writes alike to the domain.
- [KILN-IR-31] A value already inside the clamping domain is written unchanged.
- [KILN-IR-32] An overflow into a clamped column clamps to the domain, even where the domain's bound lies past the column's own width.
- [KILN-IR-33] A write to a column the schema does not declare is never clamped or aborted.

## Access log

- [KILN-IR-34] The access log records every attempted access, including the one that aborts the call.
- [KILN-IR-35] Each access log entry carries its table, key or range bounds, touched columns, and origin, for every access kind.
- [KILN-IR-36] A range read logs its bounds, including when the range turns out empty.
