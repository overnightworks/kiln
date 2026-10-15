# `kiln-ir`

`crates/kiln-ir` is the reference interpreter for the IR. This file owns the
observable behavior of that crate: what `run` and `replay` return for a given
program, call, and store, and what the schema check, store, and access log
expose. It cites the reference meaning `docs/concept.md` §2.1-§2.2
(decisions D1-D7) owns rather than restating it. A test claims a criterion by
living in the module named for it: `mod kiln_ir_07 { #[test] fn
absent_key_is_branchable() {...} }` claims `KILN-IR-07`. One criterion per
module; several tests may share it.

## Declarations

- [KILN-IR-01] A clamping domain is accepted when its minimum is at most its maximum, including when they are equal, and rejected with a typed schema error when they are reversed.
- [KILN-IR-02] A clamping domain is accepted when it lies within its column's declared width, up to the exact width bounds, and rejected with a typed schema error when it reaches one past either bound.

## Invocation

- [KILN-IR-03] `run` returns the typed schema error for an invalid schema instead of panicking.
- [KILN-IR-04] A call whose argument does not fit its parameter's declared integer width, or whose argument kind (text or integer) differs from the parameter's declared type, aborts with a typed reason.
- [KILN-IR-05] A call whose arguments fit their parameters' declared widths and types commits.
- [KILN-IR-06] A call whose argument count differs from its function's parameter count aborts with a typed reason, whether or not the body references the parameter: a missing argument names its parameter, a surplus one its position.

## Reading

- [KILN-IR-07] A read sees the transaction's own earlier writes, and write effects apply in program order. Example: a write of 5 followed by a write of 9 to the same key leaves 9, and a read between them sees 5.
- [KILN-IR-08] A point read of an absent key is a value the transaction can branch on, like any other value.
- [KILN-IR-09] A range read returns every row whose key lies within its inclusive bounds, in key order, including a range that holds a single row, whether the store returns it or a call iterates it.
- [KILN-IR-10] A range read whose start sorts after its end, or over a table with no rows, reads as an empty sequence, and iterating it runs the body zero times.
- [KILN-IR-11] A range read that matches no row is branchable as an absence, the same as a point read of an absent key.
- [KILN-IR-12] A range read that exceeds its declared bound, or whose declared bound is negative even when no row matches, aborts the call and discards every effect the call produced before it.
- [KILN-IR-13] A value projected from an earlier read can be used as the key of a later read and as an arithmetic operand.
- [KILN-IR-14] A bounded iteration runs its body once per row in key order, and the body may use dependent reads and writes. An abort inside the body discards the effects of earlier iterations.
- [KILN-IR-15] Projecting from a slot whose read found no row, projecting a column the bound row lacks, or projecting from a range-bound slot aborts with a typed reason. A slot bound by an iteration (its row slot and any slot its body reads into) reads as absent once the iteration has ended, whether or not the range held rows.
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
- [KILN-IR-35] Each access log entry carries its table, key or range bounds, and touched columns, and origin (transaction logic or constraint check), for every access kind: point read, range read, insert, update and delete.
- [KILN-IR-36] A range read logs its bounds, including when the range turns out empty.
- [KILN-IR-43] Every access the transaction's own effects make is logged with logic origin.
- [KILN-IR-44] A constraint check logs its accesses with constraint origin; they stay in the log when the check aborts the call.

## Text columns

- [KILN-IR-37] A schema can declare a column as text; a text value written to it commits.
- [KILN-IR-38] An integer value written to a column declared as text aborts with a typed reason and changes no state.

## Unique constraints

- [KILN-IR-39] A unique constraint is declared in the schema on one column of one table. A schema that declares one on an undeclared table or column is rejected with a typed schema error.
- [KILN-IR-40] A duplicate value that exists only in the middle of a transaction and is gone from its final state commits.
- [KILN-IR-41] A duplicate value in a transaction's final state aborts the call with a typed reason naming the constraint and the value; the call changes no state and publishes no response.
- [KILN-IR-42] The unique check runs once at the end of the transaction and logs exactly one point read on the unique value per row the transaction wrote that column on, however often it wrote the row; a row the transaction deleted, or wrote without that column, is not checked. The check's logged footprint is those point reads, never a range read or table scan.
