# E3 — frozen part B: candidate language, grammar, checker and manifest

Status: proposed, not yet frozen. Record: drafted in #83 by an author other
than the one who froze the families, as `docs/experiments.md` ("Freeze order
and authorship") requires; it becomes frozen when an independent reviewer of
a different provider has passed it. Part A is `inputs.md` (the six families,
their checking domains, the 52 mutants); this file is part B. Nothing below
changes after a synthesis attempt has run; a change re-freezes this part and
voids every result measured under the earlier one.

## 0. Scope and what this part does not do

It freezes five things in the order of #83: the candidate language (§1), the
grammar and rule library (§2), the checker's soundness contract (§3), the
bound-class measurement (§4) and the manifest (§5). It writes no code and
measures nothing. It states no summary for any family and no verdict.

Every frozen value carries a one-clause reason. A value marked **Decision**
is a choice this draft makes where the owner documents are silent; the
reviewer may overturn it, and §6 lists the ones I am least sure of.

Owner documents used, by section: `docs/experiments.md` ("E3", "Admissible
bound class"), `docs/concept.md` §6 and §9.4, `inputs.md` §1 and §10, and
E2's `protocol.md` §2-§6 (the growth rule, reused unchanged). The reference
meaning is `crates/kiln-ir`: `replay(&Program, &[Call], &Store) ->
Result<Vec<Execution>, SchemaError>`, with `Execution { store, outcome,
access_log }` and `Outcome::{Committed { response }, Aborted { reason }}`.

## 1. The candidate language

### 1.1 What a candidate is

A **candidate** is a typed description of one family's summary algebra, not
an IR function. For a family `Program` it supplies:

| Part | Meaning | Type |
|---|---|---|
| `shape` | the summary's field layout and its `apply` form, taken from the library of §2 | `Shape` |
| `valid[f]` | for each function `f`, an argument-only test; when false the call's summary is `ε` | `Bool` term over `f`'s parameters |
| `atom[f]` | for each function `f`, the summary of one valid call, one term per field | field-typed terms over `f`'s parameters |
| `combine` | `S ⊕ T`, one term per field | field-typed terms over the fields of `S` and `T` |
| `respond[f]` | the outcome of a call as a function of the state just before it | `Outcome` term over the state components and `f`'s parameters |

`ε` is the shape's fixed identity value. The state seen by `apply` and
`respond` is the family's **state signature** (§1.2). The checker forms the
summary of a batch by folding `atom` with `combine`, applies it to the
initial state, and reads each call's outcome from `respond` on the state
before that call. This is exactly the decomposition E2 uses for its hand
summaries (E2 `inputs.md` §1, "State, transformer, response"), so a
candidate is comparable with them.

### 1.2 State signature (derived, never written per family)

The signature is computed mechanically from the family's `Program`
(its `Schema` and function bodies) by one rule, applied identically to all
six:

- A **state component** is one integer column of one row, or, for a table
  whose rows are addressed by a parameter, one such column per key together
  with a presence flag per key.
- A component, or a row key, is **categorical** if the schema's check
  constraints (for a column) or the function bodies' argument guards (for a
  key parameter) confine it to at most 16 integers. Each such value gets a
  name `v0 … v(k-1)` in ascending order of its integer value, and a type
  `Cat(c)` of `k` values. **Reason:** this is `inputs.md` §1's rule
  ("categorical or control, at most 16 named values") made mechanical, so no
  one decides per family after seeing it.
- Every other component is a **quantity** (`Int`, checked `i64`). A stored
  integer that is not confined to 16 values by the schema or guards is a
  quantity even if it names someone, so owners are `Int` and the rule needs
  no word "identifier".

Applied to the six families this gives, as a check on the rule and not as an
input to it: quota `remaining` (check constraint 0-1000: 1001 values) is a
quantity; sequence `last_value` (at least 0 only) is a quantity; dispatcher
`cursor` (0-4) is `Cat(5)`; order `status` (0-3) is `Cat(4)`; wallet
`balance` (at least 0 only) is a quantity; lease: key `resource` (guards
confine it to 0-1) is `Cat(2)`, presence is `Bool`, and `owner` (1-1 000 000)
is a quantity. This matches `inputs.md` §1's classification exactly.

### 1.3 Types

| Type | Values | Use |
|---|---|---|
| `Int` | checked `i64` | quantities, shifts, bounds, responses |
| `Bool` | true, false | guards, presence |
| `Cat(c)` | the `k ≤ 16` names of component `c` | categorical components |
| `Name` | a declared abort, structural abort or none | outcomes |
| `Outcome` | `commit(Int)` or `abort(Name)` | `respond` |
| `Map(Cat(c) → τ)` | a total table with `k` entries, `τ ∈ {Int, Bool, Cat(c')}` | per-category structure |

`Name` ranges over the abort reasons the `Program` itself can produce, read
from it: every `AbortReason::Declared(name)` its bodies abort with, every
`AbortReason::Overflow(Width { .. })` for an integer column without a
clamping domain, `Overflow(Arithmetic)`, and every `CheckViolation` for a
declared check constraint. The checker compares full `AbortReason` values
(`inputs.md` §1, "What equivalence means").

### 1.4 Grammar

Terms are typed; a term of the wrong type is a malformed candidate (§3.1).

```
Int   ::= lit(k)            k in the constant pool P(f), §1.5
        | param(p)          an Int parameter of the function being described
        | field(i)          an Int field of a summary operand, or of the state
        | code(Cat)         the integer value of a categorical component
        | Int + Int | Int - Int
        | min(Int,Int) | max(Int,Int) | clamp(Int, Int, Int)
        | ite(Bool, Int, Int)
        | lookup(Map(Cat→Int), Cat)
Bool  ::= true | false | field(i) | not Bool | Bool and Bool | Bool or Bool
        | Int rel Int       rel in {<, <=, >, >=}       (order comparison)
        | Int ne Int        both sides non-constant     (see §1.6, rule E)
        | Cat eq Cat
        | ite(Bool, Bool, Bool) | lookup(Map(Cat→Bool), Cat)
Cat   ::= name(v)           a named value of the component's own category
        | field(i) | ite(Bool, Cat, Cat) | lookup(Map(Cat→Cat), Cat)
Map   ::= table(τ, e_0 … e_{k-1})   one term per category value, k ≤ 16
Outcome ::= commit(Int) | abort(Name) | ite(Bool, Outcome, Outcome)
          | case(Cat, Outcome^k)    one arm per category value
```

`Int + Int` and `-` use checked `i64` arithmetic like the IR
(`ArithmeticOp::{Add, Subtract}`); `min`, `max` and `clamp` are total on
`Int`. There is no multiplication, division, remainder, loop, recursion,
fold, quantifier, `Int`-indexed table or `Int`-to-`Cat` conversion. A
dispatcher-style wrap is therefore `ite(x >= 5, x - 5, x)`, as in `inputs.md`
§5.1. **Reason:** every construct is either an IR operation or a total
numeric primitive that the evaluator of §3 can define in a line; anything
more is a new boundary this experiment has no need to open.

### 1.5 Constants

An `Int` literal must be in the **constant pool** `P(f)` of the family: the
integer literals that occur in the family's `Program` (function bodies, check
constraints, declared widths, domain bounds), together with `0` and `1`.
Nothing else enters. In particular the checking domain (`X`, `A`, `L`, the
witnesses of the mutants) is not an input of the language or of the
enumerator; the enumerator is given the `Program` only (§2.1). A value that
occurs in the checking domain but in no program literal, such as wallet
balance `32 666` or lease owner `3`, cannot be written. **Reason:**
`inputs.md` §1 allows "semantic constants" in arithmetic and guards (5,
32 767, 1000, 1 000 000) and forbids the rest.

### 1.6 Why the language cannot tabulate a quantity

Four rules, each checked by validation (§3.1), together with the absence of
the constructs listed above. "Quantity-dependent" means a term whose value
depends on an `Int` field, an `Int` parameter or `code(…)`.

- **Rule L (literals).** Only pool constants are literals (§1.5). A table
  indexed by quantity values would need their literals; the checking
  domain's are unavailable, and the pool is the program's own handful.
- **Rule M (tables).** `Map` is indexed by `Cat(c)` only. There is no
  construct whose index type is `Int`, and no `Int`-to-`Cat` conversion, so
  a table cannot be addressed by a quantity or an identifier. A `Map`
  therefore has at most 16 entries by type, and its entries are counted in
  the candidate's size (§1.7).
- **Rule E (equality).** An `Int` term may not be tested for equality with a
  constant. `Int ne Int` is allowed only when neither side is a constant
  (for example a stored owner against an argument). Order comparisons
  against constants are allowed: they are thresholds, which `inputs.md` §1
  allows ("using a threshold is not enumerating the values below it").
  **Reason:** an `x = c` chain is the literal-and-branch encoding of a
  lookup that §1 names; thresholds and variable-against-variable tests are
  what the families' guards are made of (`s0.remaining >= quantity`,
  `s0.owner = owner`). Both `eq` and `ne` between variables are expressed
  with `ne` and `not`.
- **Rule B (branch budget).** A candidate contains at most 6 quantity-
  dependent `ite` or order-comparison nodes in all, and no two nested deeper
  than 3. **Decision, reason:** a staircase of thresholds is the one
  remaining way to approximate a table with pool constants; with the pool
  at most about a dozen values and the budget fixed, the staircase's
  resolution cannot grow with the number of values of the component. Six
  covers a clamp with both bounds on two components and a few guards; the
  value is not tuned against a family.

What remains enumerable is what `inputs.md` §1 allows: `Map` and `case`
over `Cat(c)` for the order status (4), dispatcher cursor (5) and lease
resource (2). The language offers no construct that reads the schema's
value range, so there is no "schema-derived domain" to iterate: the only
places a range appears are the pool constants, which are single numbers.

**Construct by construct** (the demand of #83):

| Construct | Can it encode a lookup over quantities or identifiers? |
|---|---|
| `lit` | only pool constants (rule L); no domain value |
| `param`, `field`, `code` | read one value; no table |
| `+`, `-`, `min`, `max`, `clamp` | arithmetic, no branching on a value |
| `ite`, order comparison with a constant | threshold; at most 6 and depth 3 (rule B) |
| `ne` between non-constants | compares two variables; cannot name a value (rule E) |
| `eq`, `lookup`, `table`, `case` over `Cat` | enumerates a categorical component of at most 16 values; allowed by `inputs.md` §1 |
| `Map` | index type `Cat` only (rule M) |

### 1.7 Size

The **size** of a candidate is the number of nodes in all its terms, counted
on its representation: each constructor application, literal and field
reference counts 1, and a `table` or `case` counts 1 plus the size of each
of its `k` entries. Size is a property of the candidate, not of any
evaluation. A table over `k` categories therefore costs at least `k + 1`,
which is what makes enumeration visible in the number the search orders by
(§2.4). The **bytes** of a summary value (§4) are measured on the
serialised value: `Int` 8 bytes little-endian, `Bool` and `Cat` 1 byte, a
`Map` the sum of its entries. **Reason:** the same frozen-representation
rule as E2 `protocol.md` §3, "Size, B".

### 1.8 What the language cannot express

Per `inputs.md` §1, a mutant the language cannot express is recorded by S4
as "not expressible" with its reason, never dropped. The language has no
construct for accessing the access log, so equivalence ignores it as
`inputs.md` §1 says.

## 2. Grammar and rule library

### 2.1 What the enumerator is given

The enumerator receives the family's `Program` and nothing else: no checking
domain, no mutant, no summary, no family name or number. From the `Program`
it derives the state signature (§1.2), the constant pool (§1.5) and the
`Name` set (§1.3). It may call `kiln_ir::replay` on **search samples**, which
are batches from the §4 generators with batch index `j = 20 000 + i`,
`i = 0 … 63`, size 4 (disjoint from the measurement indices of §4.2 and from
the checking domain). **Reason:** a cheap reject filter needs examples, and
drawing them from the family domain keeps checking-domain values out of the
search. A candidate that fits the samples and not the checking domain is
rejected by the checker (§3).

### 2.2 Shape library

A shape fixes a summary's fields and its `apply` form for one state
component; the candidate's `shape` is a choice of one shape per component, in
component order. The library is stated over **component kinds**, not families.

| Shape | Component kind | Fields | `apply` form | Fields' size |
|---|---|---|---|---|
| `shift` | `Int` | `a: Int` | `x + a` | 8 B |
| `clip` | `Int` | `a, l, u: Int` | `clamp(x + a, l, u)` | 24 B |
| `overwrite` | `Int` | `has: Bool, v, a: Int` | `ite(has, v, x) + a` | 17 B |
| `piecewise(m)`, `m` = 1, 2, 3 | `Int` | `t_1 … t_m, d_0 … d_m: Int` | `x + d_i`, `i` = number of thresholds `t_j` with `x >= t_j`, thresholds ascending | `16 m + 8` B |
| `table` | `Cat(c)` | `Map(Cat(c) → Cat(c))` | `lookup(map, x)` | `k` B |
| `percat(σ)` | `Map(Cat(c) → τ)` state | one `σ` per category, `σ` any shape above applied to `τ` | per category | `k` times `σ` |
| `exceptions(e)`, `e` = 1, 2 | `Bool`-presence plus `Int` identifier per category | per category: `set: Bool`, `val: Int`, `forget_1 … forget_e: Int` | the value `val` if `set`, else the current value, unless it equals one of `forget_j` (then absent) | `k (9 + 8 e)` B |

Reasons, shape by shape. `shift`, `clip` and `overwrite` are the
translation-like shapes of the standard repertoire: an offset, a saturating
offset, an offset after optionally overwriting. `table` is the
transformation-monoid element for a control component. `piecewise(m)` is the
smallest generalisation of `clip` whose breakpoints are data; `m ≤ 3` keeps it
a fixed-size shape (a shape whose `m` grew with the batch would be the
unbounded representation that the bound class rejects). `exceptions(e)` is
the smallest shape that pins released identifiers; `e ≤ 2` likewise. No
shape has a field count that depends on a batch.

No shape is named for a family or mentions one. Which shape suits which
family is not stated here; the search tries them all (§2.4). I note a
consequence honestly: `clip` is E2's quota summary shape, and the library
contains it because it is the standard saturating translation. A quota
admission thus shows the search finds a fill inside a known template, not
that it discovered saturation; the comparison of §5.6 reports it. The
alternative, a library without `clip`, would make a quota miss a statement
about the library's poverty (§6, question 2).

### 2.3 Template limits

Every hole is a term of the §1.4 grammar of the stated type, with these
limits (size in nodes as §1.7, depth in levels). **Decision** for all values;
reason: each leaves about four times the nodes of the smallest known fill
(e.g. a clipped-translation combine field is 6 nodes) and keeps the
enumeration finite.

| Hole | Max size | Max depth |
|---|---|---|
| `valid[f]` | 7 | 3 |
| `atom[f]`, per field | 9 | 4 |
| `combine`, per field | 13 | 5 |
| `respond[f]` | 17 | 5 |

A candidate's total size is at most 256, its quantity-dependent branches at
most 6 (rule B), and it has at most 6 components' shapes. The shape's
`apply` form is part of the shape and is not a hole. **Reason for fixing
`apply` by shape:** it gives each field its meaning, so `atom` and `combine`
are checked against a known denotation instead of an open one; it is the one
place the library guides the search, and §2.2 says so.

### 2.4 Rules that propose a candidate

Four rules, each generic in the state signature and the `Program`. None
mentions a family.

- **R1, shape rule.** For each component, propose each shape of its kind in
  the order of §2.2's rows; the candidate's shape is the product over
  components in component order, enumerated with the last component varying
  fastest.
- **R2, argument-guard rule.** For each function `f`, enumerate `valid[f]`
  terms over `f`'s parameters, pool constants and `lit`/`ite`/order
  comparison/boolean connectives, ascending in size. The first fill equal to
  the `Program`'s argument-only abort conditions on the search samples
  (calls whose body aborts before its first `read`) is kept; up to 2 ties.
  **Reason:** argument guards read no state, so the samples decide them.
- **R3, atom and combine rule.** For each function `f` and field, enumerate
  `atom[f]` terms; then, per surviving atom set, enumerate `combine`
  terms field by field in declaration order. A combine field may mention
  only fields of its two operands (`S.i`, `T.j`) and pool constants.
- **R4, response rule.** For each function `f`, enumerate `respond[f]` over
  the state components and `f`'s parameters, after `shape`, `valid`, `atom`
  and `combine` are fixed.

The rules run in a fixed order per shape: R2, then `atom` and `apply`
against one-call samples (state after one call equals `replay`'s), then
`combine` against two- to four-call samples, then R4 against every call of
the samples. At each stage at most **64** survivors, in enumeration order,
proceed. **Decision, reason:** the stage filters are cheap and sound only as
rejections, and a cap keeps the product of holes from being enumerated; the
checker is the only acceptance.

### 2.5 Enumeration order and ties

The order is total and deterministic:

1. shape tuple, by R1's order;
2. within a shape, the stage order of §2.4;
3. within a hole, ascending **size**, then ascending **depth**, then the
   pre-order sequence of constructor ranks, then payloads. Constructor rank
   is the order of §1.4's alternatives within their nonterminal, `Int`
   before `Bool` before `Cat` before `Map` before `Outcome`; payload order is
   ascending numeric value for literals, parameter and field index order for
   references, category order for names.

Two terms with identical behaviour on the search samples are one class; the
first in this order represents it and the others are not enumerated
(**observational-equivalence pruning**). The behaviour vector is computed on
the 64 search samples and is part of the log. **Reason:** without it the
number of syntactically distinct terms hides the smaller number of
distinguishable ones; with a fixed sample set the pruning is reproducible.

### 2.6 Candidate count

The enumerator logs, per family, deterministically and in this order:
the size of the enumeration space (counted without evaluation, by shape and
hole, before the run); the **index** of every candidate at which a stage
rejected, accepted or admitted it; per stage the number of candidates tried,
pruned by equivalence, and passed on; and the index of the admitted
candidate or of the last candidate tried. At the wall-clock deadline (§5.3)
the last index reached is not a deterministic number and is logged as such;
every other logged count is a function of the `Program` and the frozen
parameters alone, so a rerun with a longer budget reproduces the same indices
up to the deadline's. The run's result is the pair (index of admission or
"none", space size). **Reason:** ten minutes depends on the machine; an index
does not.

### 2.7 Nothing in the library is per family

The shape library keys on component kinds (`Int`, `Cat(c)`, per-category
`Map`), the rules on the state signature, and the limits are uniform. There
is no family name, family number, function name, abort name, constant or
template that appears in only one family; the pool and the `Name` set are
read from the `Program`. The shapes that fit no family are tried
all the same and cost enumeration time, which is the point of the gate. A
review checks this by grepping the library for the six families' function
and abort names: no hit.

## 3. Checker soundness contract

The checker is a separate, small component (`docs/concept.md` §9.4). It
takes a candidate, a family `Program` and that family's checking domain
`(X, A, L = 4)` from `inputs.md`, and returns one of three results. The
checker, not the enumerator, is the only acceptance.

| Result | Meaning |
|---|---|
| `Rejected(Malformed(reason))` | validation failed; no evaluation ran |
| `Rejected(Divergent(witness))` | evaluation ran and differs from `replay` or breaks associativity; the witness is a store and batch (or a triple) |
| `Valid` | all checks below held |

### 3.1 Validation precedes evaluation

Validation is a pure function of the candidate text and the `Program`. It
runs the checks below in this order and stops at the first failure; the
reason is a typed value, so a malformed candidate is told apart from a
candidate that evaluates and aborts.

1. **Well-formed:** every hole is filled, names are declared, the shape is
   in the library.
2. **Well-typed:** every term has the type §1.4 gives it, including the
   category of every `Cat` operand.
3. **Literals** are in the pool `P(f)` (rule L).
4. **Maps and equality:** no `Map` is indexed by anything but a `Cat`; no
   `Int` equality with a constant (rules M, E).
5. **Budgets:** hole sizes and depths (§2.3), total size at most 256, at most
   6 quantity-dependent branches at depth at most 3 (rule B).

An **abort** is something else: while evaluating a well-formed candidate,
checked `i64` arithmetic can overflow. That is `Aborted(Overflow(Arithmetic))`
of the evaluation, never a malformed result; if `replay` does not abort at
the same point the candidate is `Divergent`. **Reason:** `docs/experiments.md`
asks for "typed rejection, distinct from an abort", and a candidate that is
merely wrong must not be recorded as ill-formed, or the mutant count of
`Malformed` would hide soundness evidence.

### 3.2 Equivalence to `replay`

For every initial store `x` in `X` and every batch `c_1 … c_n`, `1 ≤ n ≤ 4`,
over `A` (malformed calls included), `P` pairs in all (`inputs.md` §9):

1. `replay(program, calls, x)` gives `n` executions; take each `outcome` and
   the last `store`.
2. The candidate gives, from the state signature of `x`, the batch summary
   twice: by the left fold of `atom` under `combine`, and by the balanced
   pairwise fold (E2 `protocol.md` §3). The state before call `i` is `apply`
   of the left-fold summary of the first `i - 1` calls; `respond[f]` on it
   gives call `i`'s outcome. The final state is `apply` of each full fold.
3. **Valid on this pair** iff every outcome equals `replay`'s (the full
   `Outcome`, so a wrong `AbortReason` is a divergence, `inputs.md` §1), the
   final state equals `replay`'s final store read through the signature, and
   both folds give the same final state.

States reached during a batch may lie outside `X`; the candidate must be
total there, and `replay` judges them exactly. The access log is not
compared (`inputs.md` §1). The checker enumerates all `P` pairs, not a
sample. **Reason:** `inputs.md` §9 sizes every domain (at most 1 118 464
pairs) for exhaustion, and a mutant witness is one specific pair.

### 3.3 Associativity on representations

For every triple of nonempty batches `(b_1, b_2, b_3)` over `A` with
`|b_1| + |b_2| + |b_3| ≤ 4`, let `S_i` be the left-fold summary of `b_i`.
The checker requires `(S_1 ⊕ S_2) ⊕ S_3 = S_1 ⊕ (S_2 ⊕ S_3)` as **values**
(field by field), not as functions, and requires `apply(ε ⊕ S, x) =
apply(S ⊕ ε, x) = apply(S, x)` for every `x` in `X`. **Reason:** E2 and the
property test compare values (E2 `inputs.md` §1, "Combine is associative on
representations"); a `N` mutant (`inputs.md` §1) is exact on every left fold
and shows only under another bracketing, and the triple
`(c_1, c_2, (c_3, c_4))` is its witness. The triples number at most about
200 000 for the largest alphabet (16 calls). The identity law is semantic
because E2 says so for quota (the representation is not canonical).

### 3.4 Bound-class measurement

The bound class (summary size, combine cost, apply cost) is measured by the
procedure of §4 on the candidate, with its operation counts from a counting
evaluator that follows §4.3. The checker returns the three maxima and the
growth finding to the run's record. It does not change the result of
`Valid`, which concerns correctness; the run (§5) admits a family only when
both `Valid` and §4's verdict hold.

### 3.5 The certificate

A `Valid` result is written exactly as:

> **valid over the declared domains** — the checking domain `(X, A, L = 4)`
> of family `f` in `inputs.md`, version `<commit>`, and no other.

It is bounded, not a proof (`docs/concept.md` §9.4). It says nothing about
batches longer than 4, stores outside `X` that the batches do not reach,
arguments outside `A`, or any other family version. The wording appears in
the run's record next to the bound-class verdict, which has its own scope
(§4.5).

### 3.6 What the checker may not assume

- That the candidate's own claims (its size, its types, a comment) hold; it
  recomputes them.
- That arithmetic cannot overflow, that `combine` commutes, that `ε` is
  neutral byte for byte, or that the representation is canonical.
- That the states a batch reaches lie in `X`.
- That a batch's summary depends on a grouping: both folds are run.
- Anything from the mutant list, the search samples, the hidden quota summary
  of E2 `inputs.md` §2.4, or a previous candidate. The checker has no cache
  across candidates.
- That `kiln_ir::replay` is wrong. It is the oracle (`docs/experiments.md`,
  "Shared dependency"); a suspected oracle defect stops the run and is
  reported, never worked around.

### 3.7 Solver

**No solver is used.** The enumeration (§2) and the checker (§3.2-§3.3) are
exhaustive in-process searches over finite domains. This implements #5's
ruling that no SyGuS and no cvc5 are used. If a later freeze adds a solver
to the checker, it stays confined to the checker, never enters the
enumerator, and is first tested against the interpreter on every
preregistered mutant and on randomised pairs before it may return `Valid`.
Adding one re-freezes this part.

### 3.8 Rejection of the 52 mutants

S4 runs each preregistered mutant (`inputs.md` §9) as a candidate against the
checker; a `Valid` result for a mutant is a false acceptance and fails E3
(`docs/experiments.md`, "Conjunctive success condition"). A mutant the
language cannot express is recorded as "not expressible" (§1.8).

## 4. Bound-class measurement

### 4.1 Rule reused unchanged

E2 `protocol.md` §4-§5 is applied unchanged to the three quantities `B`
(summary bytes), `C` (largest combine cost in the balanced pairwise fold) and
`A` (apply cost to the initial state): the same paired design on prefixes
(§2 there), the window 64-256, `τ = 6/5` on integers, rise and fall per seed,
and a quantity **grows** iff rising seeds minus falling seeds is at least 20
over 100 seeds. E2 `protocol.md` §6 allows the pairing where the generator's
calls are independent of position and of `n`; §4.2 below gives such
generators, so no separate error criterion is needed.

### 4.2 Batches

| Setting | Value | Reason |
|---|---|---|
| Sizes `n` | 1, 2, 4, 8, 16, 32, 64, 128, 256 | E2 `protocol.md` §2 |
| Seeds | batch index `j = 10 000 + i`, `i = 0 … 99` | **Decision.** 100 seeds as E2; the offset keeps every E3 batch distinct from E2's (`j = 0 … 99`), as E2 `protocol.md` §6 requires |
| Batch of seed `i` | `generate(256, j)` for the family, seeded `(f << 48) \| (256 << 32) \| j`; the size-`n` batch is its first `n` calls | E2 `inputs.md` §5; prefixes are batches under the paired design |
| Family numbers `f` | quota 1 (E2's number); sequence 4, dispatcher 5, order 6, wallet 7, lease 8 | `inputs.md` §10 item 4: 4 to 8 in the order of §4-§8; numbers 2 and 3 are E2's inventory and accounts |

The SplitMix64 generator, `uniform(lo, hi)` and the seed derivation are E2's
(`inputs.md` §5, `kiln_e2::SplitMix64`), unchanged. Quota uses E2's frozen
generator (`kiln_e2::quota::generate`) with `j` as above. The five new
generators follow E2's structure: the initial state is drawn first, then
every call draws the same number of values in the same order whatever it
draws, so a call depends only on the stream position (the property E2 §2
needs). They draw from the **full family domain** (`inputs.md` §1), never
from the checking domain. **Decision** for all draws; the reason is in each
row's last clause.

| Family | Initial state | Per call, in draw order | Malformed (1% each) |
|---|---|---|---|
| Sequence, `f = 4` | `last_value = uniform(0, 1 000 000)` | `k = uniform(0, 99)`, `< 80` is `next_value`, else `restart`; `w = uniform(1, 1 000 000)` (drawn for every call); `m = uniform(0, 99)` | `m = 0` and `restart`: `w = 0` if the drawn `w ≤ 500 000`, else `1 000 001`. Restarts are a fifth of the mix so a batch holds both operations |
| Dispatcher, `f = 5` | `cursor = uniform(0, 4)` | `k = uniform(0, 99)`, `< 50` is `assign`, else `skip`; `c = uniform(1, 4)`; `m` | `m = 0` and `skip`: `c = 0` if the drawn `c ≤ 2`, else `5`. Even mix so wraps occur at every size |
| Order, `f = 6` | `status = uniform(0, 3)` | `k = uniform(0, 3)`: `pay`, `ship`, `cancel`, `reopen` | none: no function has an argument. Uniform so every transition recurs |
| Wallet, `f = 7` | `balance = uniform(0, 32 767)` | `k = uniform(0, 99)`, `< 50` is `debit`, else `credit`; `a = uniform(1, 100)`; `m` | `m = 0`: `a = 0` if the drawn `a ≤ 50`, else `101`. Mixed amounts, as `inputs.md` §7.5 requires for a piece count to show |
| Lease, `f = 8` | for `r = 0, 1`: `z = uniform(0, 1)`; `o = uniform(1, 1 000 000)`; held by `o` iff `z = 1` | `k = uniform(0, 99)`: `< 40` is `acquire`, `< 75` is `release`, else `inspect`; `r = uniform(0, 1)`; `o = uniform(1, 1 000 000)` (drawn for every call); `m` | `m = 0`: `r = 2` if the drawn `o ≤ 500 000`, else `o = 0`; `inspect` takes `r = 2`. Owners are drawn from the whole range so released owners are almost always distinct, which is what `inputs.md` §8.5's argument needs |

For the lease, drawing from `[1, 1 000 000]` means two different releases
nearly never name the same owner and nearly never match the initial holder
(a chance of about one in a million per release). The measurement judges the
summary's *size*, which must carry each distinct released owner; it is not an
estimate of how often a release commits. The correctness check against
`replay` on these batches (§4.6) is the separate guard against a candidate
that is small only because it is wrong.

### 4.3 Counting boundaries for this language

E2 `protocol.md` §3 counts executed work and fixes the boundaries before any
implementation. Restated for §1's operations; a cost is the number of work
units **executed** by instrumenting the evaluator, never computed from sizes.
The counter increases by 1 for each of:

- a term node evaluated (a constructor, a literal, a field reference), once
  per evaluation; for `ite` and `case` only the evaluated arm and the
  condition or scrutinee count, so branches that do not fire cost nothing;
- a `lookup`, once, plus 1 per entry of the `Map` it reads (a table copied
  or read whole costs its length);
- a checked `i64` operation (`+`, `-`, `min`, `max`, `clamp`), once each, in
  addition to its node, whether or not it saturates;
- a value copied into a new summary, per field, hence per entry of a `Map`.

**Boundaries.** A combine starts when `combine` is entered with two summaries
and ends when it returns the result; an apply starts when `apply` is entered
with the summary and the state and ends when it returns. `B` is the
serialised byte length of §1.7. The evaluator keeps no memo, no hash-consing
and no cache; if an implementation adds one it starts empty at the entry of
each combine and each apply and is counted there, as E2 `protocol.md` §3
requires, so no work moves into a warm cache. Outside the boundaries only
building one call's atomic summary, serialising for `B` and generation may
run.

### 4.4 Stated bounds, one per family

The bound class of `docs/experiments.md` needs "a fixed, stated bound". Each
family below has one, frozen now. A candidate is **within the bound class**
iff (i) for each of `B`, `C`, `A`, the maximum over all 100 seeds and all
nine sizes is at most the family's bound, and (ii) no quantity grows (§4.1).
(`C` is undefined for `n = 1`.) **Decision** for every number. The bytes and
nodes are those of a candidate of the shape that the family's semantics
suggests, with room for one spare field; the bound does not name a summary,
and the hidden quota summary (E2 `inputs.md` §2.4) is not consulted beyond
the byte count its section 6 already prints.

| Family | `B` bytes | `C` nodes | `A` nodes | Reason, from the frozen semantics |
|---|---|---|---|---|
| Quota (§3) | 32 | 64 | 32 | one quantity with two-sided saturation: an offset and two bounds fit in 24 bytes (`inputs.md` §2.4 of E2); one spare word. Combine adds 5 terms of about 6 nodes, apply 7 |
| Sequence (§4) | 32 | 64 | 32 | translate-or-overwrite on one quantity: a flag, a value and a shift are 17 bytes; one spare word |
| Dispatcher (§5) | 16 | 96 | 32 | a map of the 5-valued cursor to itself is 5 bytes, or one `Int` rotation; room for one more word. Combine reads 5 entries, so more nodes |
| Order (§6) | 16 | 96 | 32 | a map of a 4-valued status to itself is 4 bytes; combine reads 4 entries and nests lookups |
| Wallet (§7) | 64 | 128 | 64 | expected to miss: `piecewise(3)` is 56 bytes. The bound is generous so a miss comes from growth, not from a tight constant |
| Lease (§8) | 64 | 128 | 64 | expected to miss: `percat(exceptions(2))` over 2 resources is 50 bytes. Same reason |

**Why a miss is still possible where `inputs.md` expects one.** For wallet
and lease the bound is stated and a candidate within it can exist in the
library. The expected miss is for another reason: the exact summary needs
size growing with batch length (`inputs.md` §7.5, §8.5). A candidate that
fits the fixed bound is then inexact, so `Divergent` at the checker or at
the family-domain agreement check (§4.6), or it is exact on short batches
and its measured quantity grows. Either is recorded; the bound does not make
the expectation true, and an admission would be a result (`inputs.md` §7.5,
§8.5).

### 4.5 What a pass does and does not show

It is scoped as E2 `protocol.md` §5: "no growth detected over the window
64-256 under this protocol", within the bound. It does not show a bound for
batches longer than 256, growth below `τ`, or any property outside the
frozen generators. It is not a proof.

### 4.6 Agreement with `replay` on the measurement batches

For every seed and every size the candidate's outcomes and final state on the
batch are compared with `replay` (as §3.2, from the batch's initial store).
A disagreement on any batch makes the family a miss, recorded as "diverges on
the family domain". **Decision, reason:** the certificate is only "valid
over the declared domains"; a small summary that is wrong off the checking
domain is not a summary of the family, and `inputs.md` §10 item 2 requires
that such a candidate count as a miss. I flag it for the reviewer as a
possible addition to the gate (§6, question 3).

### 4.7 Mutated candidates are not measured

Preregistered mutants (`inputs.md` §9) are checked by §3 only. A mutant that
passes the checker is a false acceptance and fails E3 whatever its size.

## 5. Manifest

`docs/experiments.md` ("Freeze order and authorship") requires the exact
hardware the ten-minute budget is measured on and the retry policy, if any,
to be recorded before the gate runs.

### 5.1 Machine

Read on 2026-10-16 with `lscpu` and `/proc/cpuinfo` (`/proc/meminfo`,
`/sys/devices/system/cpu`), as E4 `protocol.md` §9 did. A different machine
re-collects these fresh and the budget is then a different budget.

| Fact | Value |
|---|---|
| CPU | AMD Ryzen 5 5600X (family 25, model 33, stepping 2), x86_64 |
| Cores / threads | 6 physical, SMT on, 12 logical CPUs (0-11), 1 socket, 1 NUMA node |
| Caches | L1d 192 KiB and L1i 192 KiB (6 each), L2 3 MiB (6), L3 32 MiB (1) |
| Clock | minimum 560.8 MHz, maximum 4654.3 MHz; boost enabled (`cpufreq/boost` = 1); driver `amd-pstate-epp`, governor `powersave` |
| Memory | 65 752 620 kB (`MemTotal`) |
| Kernel | Linux 6.17.0-35-generic |
| Toolchain | the one in `rust-toolchain.toml`, 1.99.1 |

The governor is `powersave` and the clock varies with load. I record it and
do not change it: the budget is wall-clock on the machine as it is, and
changing system settings is outside a documentation freeze. The run's own
record (§5.5) holds the frequencies it saw.

### 5.2 Pinning and exclusion

The run takes `/tmp/probe-stack.lock` with `flock` (shared across
repositories; one probe stack per machine) and holds it until all six
families have run. It runs the whole process under `taskset -c 2`; CPU 2's
SMT sibling is CPU 8 (`/sys/devices/system/cpu/cpu2/topology/thread_siblings_list`
reads `2,8`), which stays idle for the run: the search is single-threaded and
no helper may use CPU 8. **Reason:** the same pin as E4 `protocol.md` §9, so
a sibling's load cannot shift a ten-minute figure. Before the first family
starts the harness reads `/proc/loadavg`; if the 1-minute average is at or
above 18 (1.5 times 12 logical CPUs, the machine-load floor in use) it waits
and does not start. After start there is no interference test and no
rejection of a family: the budget is wall-clock, and E4's rejected-attempt
regime would be a retry.

### 5.3 Budget, attempt, retry

- **10 minutes of wall-clock per family**, read from a monotonic clock, from
  the first candidate proposed to the admission or the deadline. It covers
  proposal, the sample filters, validation, the equivalence and associativity
  checks and the bound-class measurement (§4), because "admitted" means all
  of them (`docs/experiments.md`, "Gate", "Conjunctive success condition").
  Process start, loading the `Program` and writing the record are outside it.
- **One attempt per family, no retry, no extension** ("A family not admitted
  within ten minutes is a miss, not a retry with more budget"). A crash or a
  host fault that stops the run before a family ends stops the run; E3 is
  re-run from the first family under a recorded cause, and the aborted
  numbers are void as under the freeze protocol. The retry policy of the rule
  library is none.
- Families run one after another in the order of `inputs.md` §3-§8 (quota,
  sequence, dispatcher, order, wallet, lease), so total wall-clock is at most
  60 minutes plus setup.
- On the first `Valid` candidate within the bound class (§4.4) the family is
  admitted and the clock stops; the search does not look for a smaller one.
  **Reason:** the enumeration is ordered by size (§2.5), so the first
  admitted is also the smallest the order reaches.

### 5.4 Leakage rule

E2's hand-written quota summary (E2 `inputs.md` §2.4, the triple `(a, l, u)`
with its combine and apply) is hidden from the synthesizer and compared only
afterwards (#5 ruling). Concretely: the enumerator, the sample filters and
the checker read the family's `Program`, never the summary; no E2 summary
code is linked into the synthesizer's build; no E3 document available to
the run (this one included) states the summary's formulas. This document
does state the *shape* `clip` and the byte count 24 as it states every shape
(§2.2, §4.4); that is the known disclosure, and §5.6 reports it with the
comparison. The comparison runs only after the quota family has ended.

### 5.5 What the run records

Per family, in a record written after the family ends:

- the family, `inputs.md` commit and this file's commit, the machine facts
  of §5.1 as re-read at run time, `taskset` mask, the lock acquisition time;
- start and end of the budget on the monotonic clock, and the wall-clock
  frequency samples of CPU 2 at start, middle and end;
- the candidate log of §2.6 (space size, indices, per-stage counts, pruning
  counts, admitted index or "none");
- the outcome: admitted, or the miss reason: deadline reached, enumeration
  exhausted, `Divergent` at the checker, diverges on the family domain
  (§4.6), or outside the bound class with the failing quantity;
- for an admitted candidate: its text, its size, the §3.5 certificate, and
  per quantity `B`, `C`, `A` at every size: the maximum, and the counts of
  rising, falling and neutral seeds (§4.1);
- for quota, the §5.6 comparison.

### 5.6 Comparison with E2's quota summary

After the quota family ends, whatever its outcome, the admitted candidate (if
any) is compared with E2's hand-written summary: bytes, the combine and
apply node counts of §4.3 over the same batches, and equality of the two
denoted transformers on `remaining` in `[0, 1000]` by applying both to every
store. A difference is reported, not scored.

### 5.7 Threats to validity

- **The library knows the answer shapes.** `clip` is E2's quota shape and
  `table` is the finite-monoid shape for the order status; both are standard
  and in the library. An admission shows the search finds a fill inside a
  template, which is what #5's ruling asks ("enumerative search over a typed
  template grammar"), not discovery from scratch.
- **Scheduling noise.** The governor is `powersave` and one family's ten
  minutes may cover fewer candidates than another run's. §2.6 makes the
  indices, not the count at the deadline, the comparable figure.
- **A solver-free checker is only as exhaustive as `L = 4`.** Batches longer
  than four are not covered; `T` mutants (`inputs.md` §1, exact up to 3 calls)
  are rejected because their witnesses have four calls. A flaw at length 5
  would not be found.

## 6. Open questions for the reviewer

1. **Rule E and rule B (§1.6).** Forbidding `Int` equality with a constant
   and capping quantity-dependent branches at 6 are the two rules that do
   the work of "the grammar enforces the ban". Rule B is a judgement.
   Recommendation: keep both; if the reviewer finds a six-branch staircase
   that tabulates a family's quantity within the pool, tighten the cap
   rather than add rules.
2. **`clip` in the library (§2.2).** It is E2's quota shape. Recommendation:
   keep it, because removing standard saturation would make a quota miss
   about the library, and report the leakage in §5.7. An alternative is to
   express `clip` only as `shift` plus the `clamp` primitive in `apply`, which
   adds nothing, so I kept the named shape.
3. **Family-domain agreement as part of admission (§4.6).** `docs/experiments.md`
   does not list it; I added it because `inputs.md` §10 item 2 needs a
   checking-domain-valid but wrong candidate to be a miss, and bound-class
   measurement alone does not compare outcomes. Recommendation: keep it; if
   rejected, the lease's expected miss rests on the growth test alone.
4. **First admission stops the clock (§5.3).** Recommendation: keep; the
   alternative (search for the smallest) invites unequal budgets.
5. **Joint search cost (§2.4).** Enumerating `combine` fields jointly with
   13-node holes may be too large to finish in ten minutes for three-field
   shapes. Recommendation: freeze as written. A search that cannot finish is
   a measured miss; shrinking the limits after seeing this is exactly what
   the freeze forbids.
6. **Bounds (§4.4).** All are decisions with one spare word of headroom.
   Recommendation: keep; the growth test, not the constant, decides the
   interesting cases.
7. **Seed offset `j = 10 000 + i` (§4.2).** Recommendation: keep, to avoid
   reusing E2's batches as E2 `protocol.md` §6 says.
