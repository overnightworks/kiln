# `kiln-ir`

`crates/kiln-ir` is the reference interpreter for the IR. This file owns the
observable behavior of that crate: what `run` and `replay` return for a given
program, call, and store, and what the schema check, store, and access log
expose. It cites the reference meaning `docs/concept.md` §2.1-§2.2
(decisions D1-D9) owns rather than restating it. A test claims a criterion by
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
- [KILN-IR-09] A range read returns every row whose key lies within its inclusive bounds, comparing keys component by component, in key order, including a range that holds a single row, whether the store returns it or a call iterates it.
- [KILN-IR-10] A range read whose start sorts after its end, or over a table with no rows, reads as an empty sequence, and iterating it runs the body zero times.
- [KILN-IR-11] A range read that matches no row is branchable as an absence, the same as a point read of an absent key.
- [KILN-IR-12] A range read that exceeds its declared bound, or whose declared bound is negative even when no row matches, aborts the call and discards every effect the call produced before it.
- [KILN-IR-13] A value projected from an earlier read can be used as the key of a later read and as an arithmetic operand.
- [KILN-IR-14] A bounded iteration runs its body once per row in key order, and the body may use dependent reads and writes. An abort inside the body discards the effects of earlier iterations.
- [KILN-IR-15] Projecting from a slot whose read found no row, projecting a column the bound row lacks, or projecting from a range-bound slot aborts with a typed reason. A slot bound by an iteration (its row slot and any slot its body reads into) reads as absent once the iteration has ended, whether or not the range held rows.
- [KILN-IR-16] A range bound or an arithmetic operand that resolves to a non-integer value aborts with a typed reason.

## Writing

- [KILN-IR-17] An update or delete that targets an absent row aborts; there is no implicit insert or no-op.
- [KILN-IR-18] Inserting an already existing key aborts; two keys are the same when every component is.
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
- [KILN-IR-35] Each access log entry carries its table, key (every component) or range bounds, and touched columns, and origin (transaction logic or constraint check), for every access kind: point read, range read, insert, update and delete. An insert's touched columns are the non-key columns it names; its key is in its target.
- [KILN-IR-36] A range read logs its bounds, including when the range turns out empty.
- [KILN-IR-43] Every access the transaction's own effects make is logged with logic origin.
- [KILN-IR-44] A constraint check logs its accesses with constraint origin; they stay in the log when the check aborts the call.

## Text columns

- [KILN-IR-37] A schema can declare a column as text; a text value written to it commits.
- [KILN-IR-38] An integer value written to a column declared as text aborts with a typed reason and changes no state.

## Unique constraints

- [KILN-IR-39] A unique constraint is declared in the schema on an ordered list of one or more columns of one table (`KILN-IR-89`; a single column is the list of one). A schema that declares one on an undeclared table or column is rejected with a typed schema error.
- [KILN-IR-40] A duplicate tuple that exists only in the middle of a transaction and is gone from its final state commits.
- [KILN-IR-41] A duplicate tuple in a transaction's final state aborts the call with a typed reason naming the constraint and the tuple; the call changes no state and publishes no response.
- [KILN-IR-42] The unique check runs once at the end of the transaction and logs exactly one constraint lookup, naming the constraint and the unique tuple, per row the transaction wrote any of the constraint's columns on, however often it wrote the row; a row the transaction deleted, or wrote without any of those columns, or that lacks one of them in its final state, is not checked. The check's logged footprint is those constraint lookups, never a point read, range read or table scan.

## Foreign keys

- [KILN-IR-45] A foreign key is declared in the schema on an ordered list of one or more columns of one table (a single column is the list of one) and references another declared table's whole primary key (`KILN-IR-87`). A schema that declares one on an undeclared table or column, that references an undeclared table, or whose column count differs from the referenced table's primary key's, a keyless table having none, is rejected with a typed schema error.
- [KILN-IR-46] A row the transaction wrote any of the foreign key's columns on, whose final state holds all of them and names a key tuple that no row of the referenced table holds aborts the call with a typed reason naming the foreign key and the dangling tuple; the call changes no state.
- [KILN-IR-47] A reference that is dangling only in the middle of a transaction, because the parent is inserted or the child is repointed or removed before the end, commits.
- [KILN-IR-48] A parent row the transaction deleted, while a row of the final state still references its key, aborts the call with a typed reason naming the foreign key and the key; the call changes no state.
- [KILN-IR-49] A parent row the transaction deleted commits when no row of the final state references its key any more, including when the transaction removed or repointed the children first, and when the transaction inserted the parent again.
- [KILN-IR-50] A child check logs one constraint-origin point read, touching no column, on the referenced table at the referenced key per child row the transaction wrote any of the foreign key's columns on, however often it wrote the row; a child row lacking any of those columns in its final state is not checked. A parent-deletion check logs one constraint-origin constraint lookup on the child table, naming the foreign key and the deleted key's tuple (the representation the unique check uses), per deleted parent key absent from the final state. Neither logs a range read or table scan.

## Check constraints

- [KILN-IR-51] A check constraint is declared in the schema on one column of one table as a comparison (at least, at most) against a literal. A schema that declares one on an undeclared table or column is rejected with a typed schema error.
- [KILN-IR-52] A row the transaction wrote the constrained column on, whose final state violates a check constraint aborts the call with a typed reason naming the constraint and the row's key; the call changes no state. A violation that exists only in the middle of the transaction commits.
- [KILN-IR-53] The same out-of-range write has three distinct outcomes: a check constraint aborts with a check violation, a clamping domain writes the clamped value and commits, and a value outside the column's width without a domain aborts with an overflow.
- [KILN-IR-54] A check constraint reads only the row it judges: it logs no access of constraint origin, and the log holds only the row's own write.

## Constraint order

- [KILN-IR-55] Constraints are checked serially in one order: unique constraints, then foreign keys, then check constraints, each in declaration order. The first violation aborts the call, and constraints after it log no checks.

## Typed constraint declarations

- [KILN-IR-56] A check constraint whose operand is an integer on an integer column, or text on a text column, is accepted. An operand of the other type is rejected with a typed schema error, because such a comparison would hold or fail for every row regardless of the data.
- [KILN-IR-57] Each column of a foreign key must have the kind of the referenced primary key's column at its position: an integer column onto an integer key and a text column onto a text key are accepted, any other pairing is rejected with a typed schema error, because no row's key could ever equal its value.

## Replay

- [KILN-IR-58] `replay` returns one execution per call, each at the position of its call; an empty sequence returns no executions.
- [KILN-IR-59] `replay` runs each call against the store the previous call left, as `run` would: a committed call's changes are visible to the calls after it, an aborted call changes no state and does not stop the calls after it.
- [KILN-IR-60] `replay` returns the typed schema error for an invalid schema instead of panicking, whether or not the call sequence is empty.

## Example transactions

- [KILN-IR-61] Ticket quota: buying a ticket raises an event's sold count by one under a check constraint that the count stay at most the quota. Replayed serially, the purchase that would pass the quota aborts with a check violation, the purchases before and after it commit, and the final store holds the counts of the committed purchases.
- [KILN-IR-62] Unique sign-up: signing up inserts a user with an email under a unique constraint on the email. Replayed serially, a second sign-up with a taken email aborts with a unique violation naming the email, and the sign-ups before and after it commit.
- [KILN-IR-63] Doctors on call: going off call deletes the doctor from the on-call table only when the range of the other doctors on call is not empty, and aborts with a declared reason otherwise. Replayed serially, the call that would leave nobody on call aborts, and the calls before and after it commit.
- [KILN-IR-64] Mixed functions (concept §2.2): replaying `transfer` then a `sign_up` with a taken email in one program commits the transfer and aborts the sign-up with a unique violation, and the final store keeps the transfer's balances.

## Secondary indexes

- [KILN-IR-65] A secondary index is declared in the schema by a name, one table, one or more columns of that table (integer or text) and whether it is unique. An index on an undeclared table or column, with no column, or sharing its name with another index is rejected with a typed schema error.
- [KILN-IR-66] An indexed range read returns the rows whose indexed columns, taken in the index's column order, lie within its inclusive bounds, ordered by those values and, among rows holding equal values, by primary key. A non-unique index returns every row holding a value; a row lacking an indexed column is not in the index.
- [KILN-IR-67] An indexed range read whose start sorts after its end, or that matches no row, reads as an empty sequence, branchable as an absence, and iterating it runs the body zero times.
- [KILN-IR-68] An indexed range read sees the transaction's own earlier writes, and a bounded iteration over it visits its rows in index order.
- [KILN-IR-69] An indexed range read matching more rows than its bound aborts with the typed bound-exceeded reason and binds nothing.
- [KILN-IR-70] An indexed range read logs one transaction-logic entry of a distinct indexed-range kind naming the table, the index and both bounds, including for an empty or reversed range and for a call that then aborts; it logs no point read or primary-key range read.
- [KILN-IR-71] Rows reached through an indexed range read expose only the indexed columns and their key, so a column outside the index that is no key column cannot be projected from them (typed missing-column abort); a base row is reached by a point read at the projected key, so a call that stops at the index leaves only the indexed-range entry and one that follows it to base rows leaves that entry plus one point read per row accessed.
- [KILN-IR-72] Every row a read binds carries its key, and projecting a key column yields that component of the key the point read used or of the store key of the range or index row; projecting a key column from an absent row aborts with the typed projection-source-absent reason.

## Comparison conditions

- [KILN-IR-73] A branch can compare two expressions (arguments, literals, projections including key columns, arithmetic) by equal, not equal, less, less or equal, greater or greater or equal, and takes its first arm when the comparison holds and its second otherwise. Integers compare numerically and text compares by its bytes.
- [KILN-IR-74] A comparison of an integer with a text value aborts the call with a typed reason and changes no state; it never holds or fails by accident.
- [KILN-IR-75] An operand of a comparison that cannot be resolved (a projection from an absent row, an overflowing sum) aborts the call with that operand's own typed reason, and a comparison logs no access.

## Compound keys

- [KILN-IR-76] A table declares its primary key as an ordered, non-empty list of its own columns, or declares itself keyless. A schema whose key list is empty, names an undeclared column or repeats a column, or that declares a secondary index on a keyless table, is rejected with a typed schema error.
- [KILN-IR-77] A table keyed by three columns supports read, insert, update and delete by a key of three components: keys are equal when every component is, and a row is found only by its whole key.
- [KILN-IR-78] Key columns live only in the key: a stored row holds none of them, and projecting a key column from a row bound by a point read, a range read or an indexed range read yields the matching component of that row's key, also when an index names the key column.
- [KILN-IR-79] A key expression of a read, insert, update or delete, or a range read's bound, with a component count other than its table's key column count is rejected with a typed program error before any call runs: nothing is logged, no state changes, and the error is the same whichever function the call names.
- [KILN-IR-80] An update that names a key column, and an insert whose columns name one, is rejected with a typed program error before any call runs, so a row never moves.
- [KILN-IR-81] An effect that names an undeclared table, or a keyless one, is rejected with a typed program error before any call runs.
- [KILN-IR-82] A key component whose kind differs from its column's, or that lies outside the column's declared width, aborts with a typed reason naming the table and the component's column and never clamps, even where the column declares a clamping domain; the store is unchanged. A component that can never fit, whatever the call: a literal of another kind or outside the width, or a parameter, projected column or arithmetic whose static kind differs from the column's, is rejected with a typed program error before any call runs; a projection counts only when every table its slot can hold anywhere in the function (an iteration's row slot holds those of its source slot) has a column it can never fit, so a program the evaluator can run without an abort is never rejected; a component known only at run time stays an abort.
- [KILN-IR-83] An insert writes its key columns through its key: a unique, foreign-key or check constraint on a key column is checked for the inserted row against the component of its key, and an update, which cannot write a key column, never checks a constraint none of whose columns it writes: a constraint over a key column and a column the update writes is checked, with the key column's component taken from the row's key.

## Values

- [KILN-IR-84] An integer expression can multiply two integer expressions, checked like addition (D3): a product beyond the integer range aborts with the typed arithmetic overflow, and a product written to a column beyond its declared width aborts with the typed width overflow unless the column clamps; it never wraps. A scaled decimal is an integer at a declared scale and multiplying adds the scales, so the product is exact (a quantity times a price at scale 2 is the price total at scale 2).
- [KILN-IR-85] A conditional value evaluates its condition, then only the chosen arm: a failing projection in the other arm never runs and never aborts, while the chosen arm aborts with its own typed reason. Its condition is any branch condition and aborts the same way a branch on it does.
- [KILN-IR-86] A multiplication has the static kind integer, and a conditional the kind both its arms share, or none when they differ. A key component of a kind its column cannot take is rejected as in `KILN-IR-82`; a conditional whose arms differ in kind is left to the run.
- [KILN-IR-94] A text expression can concatenate two text expressions and truncate a text to a length, and lengths count Unicode scalar values (not bytes, and not grapheme clusters: a combining mark is its own scalar value). Truncation keeps the first n scalar values, leaves a text of n or fewer whole, and to length 0 gives the empty text; a negative length aborts with a typed reason. An operand of a concatenation, or the text of a truncation, that is not text aborts with a typed reason, as does a truncation length that is not an integer. Text declares no width (D3), so a result is never clamped and never aborts for its length.
- [KILN-IR-95] A containment condition holds when its second text occurs anywhere in its first, compared by scalar value and case-sensitively; every text contains the empty text. It is any branch condition: it decides a branch and a conditional value like a comparison, and an operand that is not text aborts with a typed reason, as a failing projection in an operand aborts with its own.
- [KILN-IR-96] A concatenation and a truncation have the static kind text, and a containment is a condition with no value kind. A key component of a kind its column cannot take is rejected as in `KILN-IR-82`, also through a conditional of text forms; a text form that aborts in every call that reaches it is rejected as a typed program error before any call: a truncation to a literal negative length (`NegativeLength`), an operand of a concatenation or containment, or the text of a truncation, whose static kind is not text (`OperandNotText`), and a truncation length whose static kind is not integer (`LengthNotInteger`), wherever an expression stands, in an arm of a conditional or of a branch no call reaches too, as a key component's kind is (`KILN-IR-82`). The static kind is the one `KILN-IR-82` uses, with a projection taking the kind its column has in every table its slot is bound from. A form whose kind only the run knows aborts at run.

## Composite constraints

- [KILN-IR-87] A foreign key maps an ordered list of one or more of the referencing table's columns, in order, onto the referenced table's whole primary key (a one-column key is the list of one). Each row the call inserted or wrote any of those columns on is checked at the transaction's end against the key formed from its final values of all of them, with one constraint-origin point read of the parent that touches no column. A row lacking any of the columns references nothing; a full tuple that names no row of the referenced table aborts with a typed reason naming the foreign key and the tuple, also when all but one component match. A key column of the child is read from its key (`KILN-IR-83`).
- [KILN-IR-88] Deleting a row of the referenced table aborts with a typed reason naming the foreign key and the deleted key when a row of the referencing table still holds that tuple in the foreign key's columns at the transaction's end, after one logged constraint lookup of the referencing table by that tuple. A call that repoints or deletes the child before the end, in any order with the delete, commits.
- [KILN-IR-89] A unique constraint names an ordered list of one or more columns: a row binds it only when it holds every one, and two rows holding the same tuple at the transaction's end abort the call with a typed reason naming the constraint and the tuple, after one logged constraint lookup by that tuple. Tuples differing in any component are distinct.
- [KILN-IR-90] A schema whose foreign key names an undeclared table or column, names no column or a column count other than its referenced table's primary key's, or has a column of another kind (integer or text) than the referenced key column at its position, or whose unique constraint names no column or an undeclared one, is rejected with a typed schema error before any call runs.
