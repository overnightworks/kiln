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

## Foreign keys

- [KILN-IR-45] A foreign key is declared in the schema on one column of one table and references another declared table's key. A schema that declares one on an undeclared table or column, or that references an undeclared table, is rejected with a typed schema error.
- [KILN-IR-46] A row the transaction wrote the foreign-key column on, whose final state names a key that no row of the referenced table holds aborts the call with a typed reason naming the foreign key and the dangling value; the call changes no state.
- [KILN-IR-47] A reference that is dangling only in the middle of a transaction, because the parent is inserted or the child is repointed or removed before the end, commits.
- [KILN-IR-48] A parent row the transaction deleted, while a row of the final state still references its key, aborts the call with a typed reason naming the foreign key and the key; the call changes no state.
- [KILN-IR-49] A parent row the transaction deleted commits when no row of the final state references its key any more, including when the transaction removed or repointed the children first, and when the transaction inserted the parent again.
- [KILN-IR-50] A child check logs one constraint-origin point read, touching no column, on the referenced table at the referenced key per child row the transaction wrote the foreign-key column on, however often it wrote the row; a child row written without that column is not checked. A parent-deletion check logs one constraint-origin lookup on the child table by the deleted key (the representation the unique check uses) per deleted parent key absent from the final state. Neither logs a range read or table scan.

## Check constraints

- [KILN-IR-51] A check constraint is declared in the schema on one column of one table as a comparison (at least, at most) against a literal. A schema that declares one on an undeclared table or column is rejected with a typed schema error.
- [KILN-IR-52] A row the transaction wrote the constrained column on, whose final state violates a check constraint aborts the call with a typed reason naming the constraint and the row's key; the call changes no state. A violation that exists only in the middle of the transaction commits.
- [KILN-IR-53] The same out-of-range write has three distinct outcomes: a check constraint aborts with a check violation, a clamping domain writes the clamped value and commits, and a value outside the column's width without a domain aborts with an overflow.
- [KILN-IR-54] A check constraint reads only the row it judges: it logs no access of constraint origin, and the log holds only the row's own write.

## Constraint order

- [KILN-IR-55] Constraints are checked serially in one order: unique constraints, then foreign keys, then check constraints, each in declaration order. The first violation aborts the call, and constraints after it log no checks.

## Typed constraint declarations

- [KILN-IR-56] A check constraint whose operand is an integer on an integer column, or text on a text column, is accepted. An operand of the other type is rejected with a typed schema error, because such a comparison would hold or fail for every row regardless of the data.
- [KILN-IR-57] Tables are keyed by integers, so a foreign key on an integer column is accepted and one on a text column is rejected with a typed schema error: no row's key could ever equal its value.

## Replay

- [KILN-IR-58] `replay` returns one execution per call, each at the position of its call; an empty sequence returns no executions.
- [KILN-IR-59] `replay` runs each call against the store the previous call left, as `run` would: a committed call's changes are visible to the calls after it, an aborted call changes no state and does not stop the calls after it.
- [KILN-IR-60] `replay` returns the typed schema error for an invalid schema instead of panicking, whether or not the call sequence is empty.

## Example transactions

- [KILN-IR-61] Ticket quota: buying a ticket raises an event's sold count by one under a check constraint that the count stay at most the quota. Replayed serially, the purchase that would pass the quota aborts with a check violation, the purchases before and after it commit, and the final store holds the counts of the committed purchases.
- [KILN-IR-62] Unique sign-up: signing up inserts a user with an email under a unique constraint on the email. Replayed serially, a second sign-up with a taken email aborts with a unique violation naming the email, and the sign-ups before and after it commit.
- [KILN-IR-63] Doctors on call: going off call deletes the doctor from the on-call table only when the range of the other doctors on call is not empty, and aborts with a declared reason otherwise. Replayed serially, the call that would leave nobody on call aborts, and the calls before and after it commit.
- [KILN-IR-64] Mixed functions (concept §2.2): replaying `transfer` then a `sign_up` with a taken email in one program commits the transfer and aborts the sign-up with a unique violation, and the final store keeps the transfer's balances.
