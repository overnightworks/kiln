# E3 — frozen part B: candidate language, component library, checker and manifest

Status: proposed, not yet frozen. Record: drafted in #83 by an author other
than the one who froze the families, as `docs/experiments.md` ("Freeze order
and authorship") requires; it becomes frozen when an independent reviewer of
a different provider has passed it. Part A is `inputs.md` (the six families,
their checking domains, the 52 mutants); this file is part B. Nothing below
changes after a synthesis attempt has run; a change re-freezes this part and
voids every result measured under the earlier one.

## 0. Scope

It freezes, in the order of #83: the candidate language (§1), the component
library and the search (§2), the checker's soundness contract (§3), the
bound-class measurement (§4) and the manifest (§5). §6 records the head's
rulings of 16.10.2026 as decisions with their reasons. It writes no code,
measures nothing, and states no summary for any family and no verdict.

Every frozen value carries a one-clause reason. Owners: `docs/experiments.md`
(E3), `docs/concept.md` §6 and §9.4, `inputs.md` §1 and §10, and E2's
`protocol.md` (the growth rule). The reference meaning is `crates/kiln-ir`:
`replay(&Program, &[Call], &Store) -> Result<Vec<Execution>, SchemaError>`,
with `Execution { store, outcome, access_log }` and
`Outcome::{Committed { response }, Aborted { reason }}`.

## 1. The candidate language

### 1.1 State signature, frozen by name

A family's state is read through its **signature**: a fixed list of named
state components, each with a kind. The classification is frozen here by
name, from `inputs.md` §1's named examples and each family's semantics; no
cardinality rule derives it (decision D2, §6).

| Family | Component | Kind | Class (`inputs.md` §1) |
|---|---|---|---|
| Quota (§3) | `remaining`, `quota[0]` | `Int` | quantity |
| Sequence (§4) | `last_value`, `sequence[0]` | `Int` | quantity |
| Dispatcher (§5) | `cursor`, `dispatcher[0]` | `Cat(cursor)`, values `0 … 4` | control |
| Order (§6) | `status`, `orders[0]` | `Cat(status)`, values `0 … 3` | categorical |
| Wallet (§7) | `balance`, `wallet[0]` | `Int` | quantity |
| Lease (§8) | `lease`, keyed by `resource` | `Keyed(Cat(resource) → Opt(Int))`, keys `0, 1` | key and row presence: categorical/control; `owner`: identifier |

Every function parameter is an `Int`: parameters are arguments, not state
components, and `inputs.md` §1 classifies state. A `Cat(c)` value is written
`v0 … v(k-1)` in ascending order of its integer code. `Opt(Int)` is a row
that is absent or present with an `owner`; reading a store through the
signature maps an absent `lease` row to `none`. **Reason:** the head ruling
lists exactly these components and classes; naming them leaves nothing to
infer from a value range.

### 1.2 Types

| Type | Values |
|---|---|
| `Int` | checked `i64` |
| `Bool` | true, false |
| `Cat(c)` | the `k ≤ 16` named values of component `c` |
| `Opt(Int)` | `none`, or `some(n)` with `n` an `Int` |
| `Map(c)` | a total table `Cat(c) → Cat(c)` with `k` entries |
| `Name` | an abort reason the `Program` can produce |
| `Outcome` | `commit(Int)` or `abort(Name)` |

`Name` ranges over the reasons read from the `Program`: every
`AbortReason::Declared(name)` its bodies abort with;
`Overflow(Width { table, column, key })` for every integer column without a
clamping domain and every key the family's store can hold;
`Overflow(Arithmetic)`; and `CheckViolation` for every declared check
constraint and key. The checker compares full `AbortReason` values
(`inputs.md` §1, "What equivalence means").

### 1.3 Grammar

Terms are typed; a term of the wrong type is malformed (§3.1). One grammar
serves two uses. **Hole terms** are what the enumerator writes (§2.4).
**Checker form** is what the checker evaluates: hole terms together with
the component definitions of §2.1, written in the same grammar, and the
mutants S4 writes (§3.8). Productions marked † occur only in checker form,
never in a hole.

```
Int     ::= lit(n)                 n in the constant pool P, §1.5
          | param(p)               an Int parameter of the function
          | key                    the code of the key an entry is for
          | state(c)               the value of an Int state component
          | code(Cat)              the integer code of a category value
          | get(Opt, Int)          the owner if present, else the default
          | Int + Int | Int - Int
          | min(Int, Int) | max(Int, Int) | clamp(Int, Int, Int)
          | ite(Bool, Int, Int)
          | O.f                    † an Int field of operand O ∈ {S, T, s}
          | Int mod n              † Euclidean remainder, n a positive literal
Bool    ::= true | false | not Bool | Bool and Bool | Bool or Bool
          | Int rel Int            rel in {<, <=, =, !=, >=, >}
          | Cat = Cat | present(Opt)
          | O.f                    † a Bool field of an operand
Cat     ::= name(v) | state(c) | ite(Bool, Cat, Cat)
          | lookup(Map, Cat)       † the entry of a table
          | of(c, Int)             † the value of c whose code is the Int
Opt     ::= none | some(Int) | state(c, v)
          | ite(Bool, Opt, Opt)
Map     ::= table(Cat_0, …, Cat_{k-1})
          | O.f                    † a Map field of an operand
Entry   ::= when(Bool, t_1, …, t_m)   a per-key atom entry, §2.1
Carrier ::= (t_1, …, t_m) | S | T | ite(Bool, Carrier, Carrier)   †
Outcome ::= commit(Int) | abort(Name) | ite(Bool, Outcome, Outcome)
          | case(Cat, Outcome_0, …, Outcome_{k-1})
```

`state(c)` reads a scalar state component and `state(c, v)` the row of a
keyed component at key `v`. In checker form, `S` and `T` are the left and
right operands of a combine and `s` the summary being applied; `x` is the
state component (or, under `per-key`, the key's row) being applied to; a
`Carrier` term is a combine's result. Where each production may occur:

| Hole | May reference |
|---|---|
| `valid[f]` | `lit`, `param`, connectives, comparisons |
| `atom[f]`, a field | `lit`, `param`, `key` (inside an `Entry` only), `none`, `some`, `table`, `name`, `ite`, arithmetic, comparisons |
| `respond[f]` | everything of `atom[f]` except `key`, `table` and `Entry`, plus `state`, `code`, `get`, `present`, `case` |

**Map-valued operands.** A `Map` is written as a literal `table` in an atom
or read as an operand field `S.m`, `T.m`, `s.m` in checker form; no hole
contains a map-valued reference. **Reason:** the holes need tables only as
atoms of `transform` (§2.1), and the operand productions exist so the
component definitions and S4's mutants are terms of one evaluator.

### 1.4 Primitive semantics

- `+` and `-` are checked `i64`, as the IR's `ArithmeticOp::{Add,
  Subtract}`; `min` and `max` are total.
- `clamp(z, l, u) = min(max(z, l), u)`, also for `l > u`, where it yields
  `u`. **Reason:** this is E2 `inputs.md` §2.4's definition verbatim, it is
  total, and a candidate with reversed bounds is then simply wrong and
  rejected by comparison, not undefined.
- `Int mod n` is the Euclidean remainder in `[0, n)`. `of(c, i)` with `i`
  not a code of `c` rejects the candidate as an overflow does; `rotate`
  passes a remainder, so there it is always defined.
- `ite` and `case` evaluate the condition or scrutinee and the chosen arm
  only. `get(o, d)` is the owner of `o` if present, else `d`.
- **Overflow.** If any evaluation of a candidate — an atom, a combine, an
  apply or a respond — overflows checked `i64` on any input the checker
  evaluates (§3.2-§3.4), the candidate is rejected with the typed result
  `Rejected(Overflow(witness))`. It is never mapped to a call's abort. A
  call's abort is reproduced only when `respond[f]` evaluates to
  `abort(name)`, and the checker compares that per call with `replay`
  (decision D4). **Reason:** a summary's own arithmetic is not the
  program's; mapping one to the other would let a candidate fail into a
  right answer.

### 1.5 Constants

An `Int` literal must be in the family's **constant pool** `P`: the integer
literals of its `Program` (function bodies, check constraints), the bounds
of every declared column and parameter width, and `0` and `1`. A key code
enters only through `key`. The checking domain's values are never literals:
wallet balance `32 666` or lease owner `3` cannot be written. **Reason:**
`inputs.md` §1 allows semantic constants in arithmetic and guards (5,
32 767, 1000, 1 000 000) and nothing else.

### 1.6 What the grammar does not enforce

Over a finite checking domain, any language with addition and comparison
can encode an indicator: `clamp(x, 0, 1) - clamp(x - 1, 0, 1)` is `1` at
`x = 1` and `0` at every other integer, and `1 + 1 + 1` builds the
unavailable literal `3`. No construct-level rule therefore makes a quantity
table unrepresentable, and this part does not claim one. The tabulation ban
of `inputs.md` §1 is enforced **semantically** by the checker contract: an
admitted candidate must also agree with `kiln_ir::replay` on batches drawn
from the family's full domain (§3.4), where a table of the small checking
domain is wrong (decision D1).

Two rules stay, as **search-space hygiene only**; they shrink the space and
enforce nothing:

- **Rule L.** Literals come from the pool (§1.5).
- **Rule M.** A table is indexed by a `Cat` only, and no hole converts an
  `Int` to a `Cat` (`of` is checker-form only).

How this relates to `inputs.md`: §1 says S2's grammar enforces the ban and
§10 item 1 that "S2 owns its enforcement in the grammar". S2 enforces it
through the checker contract instead, because the grammar cannot. §10 item
1's three recommendations hold: no hole takes a checking-domain value as a
literal, size is measured on the representation (§1.7), and the measurement
batches come from the family domain (§4.2). §10 item 2 asks that a
candidate valid over the checking domain but growing on family-domain
batches count as a miss; in this language no representation can grow
(§4.4), so such a candidate is caught by family-domain disagreement
instead. `inputs.md` is not edited.

### 1.7 Size and bytes

The **size** of a term is its node count: each constructor application,
literal and reference counts 1; a `table`, `case` or `Entry` counts 1 plus
its parts. It orders the search (§2.5). The **bytes** `B` of a summary are
its serialised length at fixed width: a tag byte, then every carrier field
(`Int` 8 bytes little-endian, `Bool` and `Cat` 1, `Opt(Int)` 9, `Map` `k`),
with the identity serialised as tag `0` and zeroed fields. **Reason:** the
frozen-representation rule of E2 `protocol.md` §3, "Size, B"; fixed width
makes `B` a property of the chosen components, not of the data.

### 1.8 What a candidate is

A **candidate** for a family's `Program` is:

| Part | Meaning |
|---|---|
| layout | one library component (§2.1) per signature component, in signature order |
| `valid[f]` | per function `f`, a `Bool` hole term; when false the call's summary is `ε` |
| `atom[f]` | per function `f`, one hole term per carrier field of the layout (an `Entry` per key for `per-key`) |
| `respond[f]` | per function `f`, an `Outcome` hole term: the call's outcome from the state just before it |

Combine and apply are not holes: they are the components' definitions. The
summary of a call is `atom[f]` if `valid[f]` holds, else `ε`; its outcome is
always `respond[f]`. This is the decomposition E2 uses for its hand
summaries (E2 `inputs.md` §1, "State, transformer, response"). Per
`inputs.md` §1 the access log is not compared, and the language has no
construct for it.

## 2. Component library and search

### 2.1 Components

The library is a fixed set of generic algebraic components. Each is a
semigroup on its carrier with an **adjoined identity** `ε`: `ε ⊕ T = T`,
`S ⊕ ε = S`, `apply(ε, x) = x`, and the body below runs only when both
operands are values. **Reason:** an adjoined identity is exact byte for
byte and needs no sentinel such as `i64::MIN`, whose arithmetic would
overflow. No component names a family, a field or a function; each one's
generality is argued by its source, a use outside these six families
(decision D3).

| Component | Kind it acts on | Carrier | Combine body `S ⊕ T` | Apply body `apply(s, x)` | Source |
|---|---|---|---|---|---|
| `shift` | `Int` | `a: Int` | `(S.a + T.a)` | `x + s.a` | the additive group; prefix sums (Blelloch, "Prefix sums and their applications", 1990) |
| `clamp-shift` | `Int` | `a, l, u: Int` | `(S.a + T.a, clamp(S.l + T.a, T.l, T.u), clamp(S.u + T.a, T.l, T.u))` | `clamp(x + s.a, s.l, s.u)` | the composition closure of saturating additions, as in bounded counters and token buckets |
| `reset-shift` | `Int` | `r: Bool, v: Int, a: Int` | `ite(T.r, T, (S.r, S.v, S.a + T.a))` | `ite(s.r, s.v, x) + s.a` | the segmented-scan operator (Blelloch 1990): a sum restarted at a flag |
| `raise` | `Int` | `m: Int` | `(max(S.m, T.m))` | `max(x, s.m)` | the max semilattice (a max-register) |
| `lower` | `Int` | `m: Int` | `(min(S.m, T.m))` | `min(x, s.m)` | the min semilattice |
| `assign` | `Int`, `Cat(c)`, `Opt(Int)` | `v` of the kind | `T` | `s.v` | the last-writer-wins register (Shapiro et al., CRDTs, 2011) |
| `fill` | `Opt(Int)` | `v: Int` | `S` | `ite(present(x), x, some(s.v))` | insert-if-absent: the first writer wins (SQL `ON CONFLICT DO NOTHING`) |
| `rotate` | `Cat(c)`, `k` values | `a: Int` | `((S.a + T.a) mod k)` | `of(c, (code(x) + s.a) mod k)` | the cyclic group `Z_k` acting on a `k`-cycle; E2's inventory is a translation mod 91 |
| `transform` | `Cat(c)`, `k` values | `m: Map(c)` | `(table(lookup(T.m, lookup(S.m, v0)), …, lookup(T.m, lookup(S.m, v(k-1)))))` | `lookup(s.m, x)` | the full transformation monoid; composing finite-state transition tables (Hillis and Steele, "Data parallel algorithms", 1986) |
| `per-key(σ)` | `Keyed(Cat(c) → Opt(Int))`, `k` keys | one value of `σ` or `ε` per key | per key, `σ`'s combine | per key, `σ`'s apply to that key's row | the product of `k` copies of a monoid, indexed by a finite set |

`σ` ranges over the components acting on `Opt(Int)` (`assign`, `fill`). A
`per-key` atom is one `Entry` per key: `when(b, t_1 … t_m)` is `σ`'s value
`(t_1 … t_m)` at that key if `b` holds, else `ε` there. The summary of a
whole state is the product of one component value per signature component,
combined and applied component by component; every family here has one
signature component, so its product has one factor.

### 2.2 Applicability

| Kind | Components tried, in this order |
|---|---|
| `Int` | `assign`, `shift`, `raise`, `lower`, `reset-shift`, `clamp-shift` |
| `Cat(c)` | `assign`, `transform`, `rotate` |
| `Keyed(Cat(c) → Opt(Int))` | `per-key(assign)`, `per-key(fill)` |

**Reason:** a deterministic order fixed before the run, narrow carriers
first for the six signatures; it decides nothing else.

### 2.3 What the enumerator receives

The enumerator receives the family's `Program` and the family's
**observation environments**, and nothing else: no mutant, no summary, no
family name or number. From the `Program` it reads the signature of §1.1,
the pool and the `Name` set. The observation environments are frozen:

- for `valid[f]` and `atom[f]`: the arguments of every call of `f` in the
  checking domain's alphabet `A`, malformed ones included;
- for `respond[f]` and the stage filters: every store of `X` with those
  arguments.

They are deterministic, typed by each program's declared parameter types
and drawn from the checking domain. They serve pruning (§2.5) and the stage
filters (§2.6) only, never as literals (rule L); this is what "the
enumerator receives only the `Program` and these observation environments"
means (decision D8). **Reason:** pruning needs fixed inputs to compare terms
on, and the checker evaluates this set anyway.

### 2.4 Holes and search limits

Every hole is a hole term of §1.3 of its type. The limits are **search
limits**: they bound the enumeration and decide nothing about correctness or
the bound class.

| Limit | Value | Reason |
|---|---|---|
| size of any hole of function `f` | the node count of `f`'s body in `kiln_ir` (every `Effect`, `Terminal`, `Condition` and `Expr` node), computed from the `Program` at run start and logged | a term describing part of `f` needs no more nodes than `f` itself; the number comes from the `Program`, not from a known fill |
| survivors per hole group (§2.6) | 64, in enumeration order | bounds the product of per-function survivors; later members are not tried |

There is no depth limit and no branch budget: depth is bounded by size, and
a branch budget enforced nothing (§1.6).

### 2.5 Enumeration order and pruning

The order is total and deterministic:

1. the layout, by §2.2's order, the last signature component varying
   fastest;
2. within a layout, the functions in the `Program`'s declaration order, and
   per function the hole group `(valid[f], atom[f])`, then `respond[f]`;
3. within a hole, ascending size, then the pre-order sequence of
   constructor ranks (the order of §1.3's alternatives), then payloads
   (literals ascending, parameters and fields in declaration order,
   categories in code order);
4. a candidate is a choice of one survivor per hole group; candidates are
   enumerated lexicographically by survivor index, the first function's
   groups varying slowest.

This order is **not globally smallest-first**: a larger hole term in an
earlier layout precedes a smaller one in a later layout. The admitted
candidate is the first admissible one in this order, not necessarily the
smallest (decision D9).

**Observational-equivalence pruning.** Two terms of one hole with equal
values on every observation environment of that hole (§2.3) form one class;
only the first in the order is kept, and the others are never revisited.
This is **incomplete**: two terms equal on the checking domain may differ on
the family domain, and the discarded one may be the only one that agrees
there. There is no backtracking into pruned classes. Every miss is
therefore scoped to this incomplete search: it is not a proof that no
summary exists in the language (decision D8). **Reason:** without pruning,
the number of syntactically distinct terms hides the far smaller number of
distinguishable ones; with frozen environments the pruning is reproducible.

### 2.6 Stage filters

Before a candidate reaches the checker, its hole groups pass two filters,
each a subset of the checker's own one-call checks (§3.2):

- `(valid[f], atom[f])`: for every store `x` of `X` and every call of `f`
  in `A`, applying the call's summary to `x` gives `replay`'s store after
  that single call, read through the signature;
- `respond[f]`: for every such pair, the outcome equals `replay`'s.

Each filter depends on one function's holes only, so functions are filtered
independently and only survivors are combined. A filter rejects only what
the checker would reject, so it never discards an admissible candidate.

### 2.7 Candidate count

The enumerator logs per family: the size of the enumeration space per
layout and hole, counted without evaluation before the run; for every
candidate that reaches a check, its index in the order of §2.5 and the
stage that rejected or admitted it; per hole the terms tried, pruned and
surviving; and the index of the admission or of the last candidate tried.
**Determinism** means the enumeration order is a function of the `Program`,
the observation environments and this part alone, so every index is
reproducible. How far the search gets within ten minutes depends on the
machine; the count reached at the deadline is recorded, not promised.

## 3. Checker soundness contract

The checker is a separate, small component (`docs/concept.md` §9.4). It
takes a candidate in checker form (§1.3), a family `Program`, that family's
checking domain `(X, A, L = 4)` from `inputs.md` and its family-domain
batches (§4.2), and returns one result. The checker, not the enumerator, is
the only acceptance.

| Result | Meaning |
|---|---|
| `Rejected(Malformed(reason))` | validation failed; no evaluation ran |
| `Rejected(Overflow(witness))` | the candidate's own evaluation overflowed checked `i64` (§1.4) |
| `Rejected(Divergent(witness))` | a comparison with `replay` or an associativity check failed; the witness is a store and batch, or a triple |
| `Rejected(FamilyDomain(witness))` | valid over the checking domain, but disagrees with `replay` on a family-domain batch (§3.4) |
| `Valid` | every check below held |

### 3.1 Validation precedes evaluation

Validation is a pure function of the candidate and the `Program`, run in
this order, stopping at the first failure with a typed reason:

1. **Well-formed:** every hole filled, every name declared, every component
   in the library and of the right kind for its signature component.
2. **Well-typed:** every term has its §1.3 type, including the category of
   every `Cat`, and uses only productions its hole allows.
3. **Literals** are in the pool (rule L); tables are `Cat`-indexed (rule M).
4. **Search limits** of §2.4 hold.

A malformed candidate is thereby told apart from one that evaluates and is
wrong, as `docs/experiments.md` asks ("typed rejection, distinct from an
abort"). Mutants in checker form (§3.8) skip check 4, which bounds the
search, not the checker.

### 3.2 Equivalence to `replay` over the checking domain

For every initial store `x` in `X` and every batch `c_1 … c_n`, `1 ≤ n ≤ 4`,
over `A` (malformed calls included), `P` pairs in all (`inputs.md` §9):

1. `replay(program, calls, x)` gives `n` executions; take each `outcome` and
   the last `store`.
2. The candidate's batch summary is formed twice: the left fold of the call
   summaries under combine, and the balanced pairwise fold (E2 `protocol.md`
   §3). The state before call `i` is `apply` of the left fold of the first
   `i - 1` calls to `x`; `respond[f_i]` on it is call `i`'s outcome.
3. The pair holds iff every outcome equals `replay`'s (the full `Outcome`, so
   a wrong `AbortReason` diverges), the final state of each fold equals
   `replay`'s final store read through the signature, and the two folds give
   the same final state.

States reached during a batch may lie outside `X`; the candidate must be
total there, and `replay` judges them exactly. All `P` pairs are checked,
not a sample. **Reason:** `inputs.md` §9 sizes every domain (at most
1 118 464 pairs) for exhaustion, and a mutant's witness is one specific
pair.

### 3.3 Associativity on representations

For every triple of nonempty batches `(b_1, b_2, b_3)` over `A` with
`|b_1| + |b_2| + |b_3| ≤ 4`, with `S_i` the left-fold summary of `b_i`, the
checker requires `(S_1 ⊕ S_2) ⊕ S_3 = S_1 ⊕ (S_2 ⊕ S_3)` as values, field by
field. **Reason:** E2 compares values (E2 `inputs.md` §1, "Combine is
associative on representations"); an `N` mutant is exact on every left fold
and shows only under another bracketing, as in its witness
`((c_1, c_2), (c_3, c_4))`. With adjoined identities, `ε ⊕ S = S ⊕ ε = S`
holds by definition and is not checked separately.

### 3.4 Agreement on the family domain

For every seed `i` and every size `n` of §4.2, the candidate is compared
with `replay` on the batch's first `n` calls from its initial store, as in
§3.2 step 3: every outcome, the final state of the left fold and of the
balanced fold. A disagreement is `Rejected(FamilyDomain(seed, n, call))`.
**Reason:** a candidate valid only over the finite checking domain may be a
table, not a summary (§1.6), and a table of the checking domain is wrong on
the family domain (decision D1). This check is **sampled**: 100 batches of
at most 256 calls, a fixed set and not every input. It strengthens the
evidence; it is not a proof and does not widen the certificate.

### 3.5 The certificate

A `Valid` result is written exactly as:

> **valid over the declared domains** — the checking domain `(X, A, L = 4)`
> of family `f` in `inputs.md`, version `<commit>`, and no other.

It is bounded, not a proof (`docs/concept.md` §9.4): it says nothing about
batches longer than 4, stores outside `X` that the batches do not reach,
arguments outside `A`, or another family version. The record states beside
it that the candidate also agreed with `replay` on the 100 family-domain
batches of §4.2, a sampled check, and gives the bound-class verdict with its
own scope (§4.5).

### 3.6 What the checker may not assume

It may not assume that the candidate's claims hold (it recomputes size,
types and layout); that arithmetic cannot overflow, combine commutes or a
representation is canonical; that reached states lie in `X` or a summary is
independent of grouping; or anything from the mutant list, the observation
environments, E2's hidden quota summary or an earlier candidate (it keeps no
cache). It may not assume `kiln_ir::replay` is wrong: it is the oracle
(`docs/experiments.md`, "Shared dependency"), and a suspected oracle defect
stops the run and is reported, never worked around.

### 3.7 Solver

**No solver is used** (#5's ruling: no SyGuS, no cvc5); every check is an
in-process evaluation over a finite set. A later freeze that adds a solver
confines it to the checker, tests it first against the interpreter on every
mutant and on randomised pairs, and re-freezes this part.

### 3.8 Rejection of the 52 mutants

S4 writes each preregistered mutant (`inputs.md` §9) in checker form: a
carrier and `valid`, `atom`, combine, apply and `respond` terms of §1.3,
which may use the † productions, so a mutant's deviating combine (an `N`,
`O` or `T` mutant) is written directly and is not limited to the library.
Each is run through §3.1 (checks 1-3), §3.2 and §3.3; a `Valid` result is a
false acceptance and fails E3 (`docs/experiments.md`, "Conjunctive success
condition"). A mutant that checker form cannot express is recorded as "not
expressible" with its reason (`inputs.md` §1), never dropped. Mutants are
not measured for the bound class. **Reason:** the checker's soundness does
not depend on what the library can produce, so the checker is tested on
the widest form it accepts.

## 4. Bound-class measurement

### 4.1 Rule reused unchanged

E2 `protocol.md` §4-§5 is applied unchanged to `B` (summary bytes), `C`
(largest combine cost in the balanced pairwise fold) and `A` (apply cost to
the initial state): the paired design on prefixes (§2 there), the window
64-256, `τ = 6/5` on integers, rise and fall per seed, and a quantity
**grows** iff rising seeds minus falling seeds is at least 20 over 100
seeds. E2 `protocol.md` §6 allows the pairing where the generator's calls
are independent of position and of `n`; §4.2's generators have that
property, so no separate error criterion is needed.

### 4.2 Batches

| Setting | Value | Reason |
|---|---|---|
| Sizes `n` | 1, 2, 4, 8, 16, 32, 64, 128, 256 | E2 `protocol.md` §2 |
| Seeds | batch index `j = 10 000 + i`, `i = 0 … 99` | **Decision.** 100 seeds as E2; the offset keeps every E3 batch distinct from E2's `j = 0 … 99`, as E2 `protocol.md` §6 requires |
| Batch of seed `i` | the 256 calls drawn from state `(f << 48) \| (256 << 32) \| j`; the size-`n` batch is its first `n` calls | E2 `inputs.md` §5; prefixes are batches under the paired design |
| Family numbers `f` | quota 1 (E2's number); sequence 4, dispatcher 5, order 6, wallet 7, lease 8 | `inputs.md` §10 item 4; 2 and 3 are E2's inventory and accounts |

SplitMix64, `uniform(lo, hi)` and the seed derivation are E2's (`inputs.md`
§5), unchanged: `kiln_e2::SplitMix64::from_state` with the state above,
since `kiln_e2::Family` numbers only E2's three families. Quota uses E2's
frozen generator (`kiln_e2::quota::generate`) with `j` as above. The five
new generators follow E2's structure: the initial state is drawn first,
then every call draws the same number of values in the same order whatever
it draws, so a call depends only on its stream position (E2 §2's property).
They draw from the **full family domain** (`inputs.md` §1), never from the
checking domain. **Decision** for every draw; each row's last clause gives
its reason.

| Family | Initial state | Per call, in draw order | Malformed (1% each) |
|---|---|---|---|
| Sequence, `f = 4` | `last_value = uniform(0, 1 000 000)` | `k = uniform(0, 99)`, `< 80` is `next_value`, else `restart`; `w = uniform(1, 1 000 000)` (drawn for every call); `m = uniform(0, 99)` | `m = 0` and `restart`: `w = 0` if the drawn `w ≤ 500 000`, else `1 000 001`. Restarts are a fifth of the mix so a batch holds both operations |
| Dispatcher, `f = 5` | `cursor = uniform(0, 4)` | `k = uniform(0, 99)`, `< 50` is `assign`, else `skip`; `c = uniform(1, 4)`; `m` | `m = 0` and `skip`: `c = 0` if the drawn `c ≤ 2`, else `5`. Even mix so wraps occur at every size |
| Order, `f = 6` | `status = uniform(0, 3)` | `k = uniform(0, 3)`: `pay`, `ship`, `cancel`, `reopen` | none: no function has an argument. Uniform so every transition recurs |
| Wallet, `f = 7` | `balance = uniform(0, 32 767)` | `k = uniform(0, 99)`, `< 50` is `debit`, else `credit`; `a = uniform(1, 100)`; `m` | `m = 0`: `a = 0` if the drawn `a ≤ 50`, else `101`. Mixed amounts, as `inputs.md` §7.5 requires |
| Lease, `f = 8` | for `r = 0, 1`: `z = uniform(0, 1)`; `o = uniform(1, 1 000 000)`; held by `o` iff `z = 1` | `k = uniform(0, 99)`: `< 40` is `acquire`, `< 75` is `release`, else `inspect`; `r = uniform(0, 1)`; `o = uniform(1, 1 000 000)` (drawn for every call); `m` | `m = 0`: `r = 2` if the drawn `o ≤ 500 000`, else `o = 0`; `inspect` takes `r = 2`. Owners from the whole range, as `inputs.md` §8.5's argument needs |

The same batches serve the bound class and the family-domain agreement of
§3.4, where a table of the checking domain's values is wrong.

### 4.3 Counting boundaries for the components

E2 `protocol.md` §3 counts executed work at fixed boundaries; restated for
§2.1. A cost is the number of work units **executed**, measured by
instrumenting the evaluator, never computed from sizes. The counter
increases by 1 for each:

- checked `i64` operation executed (`+`, `-`, `min`, `max`, `clamp`,
  `mod`), as E2 counts every addition, subtraction and clamp;
- test executed: an identity-tag test, an `ite` condition, a `present`
  test; an arm that does not run costs nothing;
- `Map` entry read or written.

Copying a field and converting between a category and its code cost
nothing: they change no value. **Boundaries:** a combine starts when it is
entered with two summaries and ends when it returns; an apply starts when
it is entered with a summary and a state and ends when it returns. The
evaluator keeps no memo or cache; one added later starts empty at each
entry and is counted there, as E2 `protocol.md` §3 requires. Outside the
boundaries only building a call's atomic summary (its `valid` and `atom`
terms), `respond`, serialising for `B` and generation run.

### 4.4 Stated bounds, derived from the library

Hole terms run outside the boundaries (§4.3), so `B`, `C` and `A` depend
only on the layout. Per component, from §1.7 and §4.3, with both operands
values (two tag tests per combine, one per apply):

| Component | `B` | `C` at most | `A` at most |
|---|---|---|---|
| `assign` | 9 (`Int`), 2 (`Cat`), 10 (`Opt(Int)`) | 2 | 1 |
| `shift`, `raise`, `lower` | 9 | 3 | 2 |
| `reset-shift` | 18 | 4: tag tests, `T.r`, one addition | 3 |
| `clamp-shift` | 25 | 7: tag tests, 3 additions, 2 clamps | 3 |
| `fill` | 9 | 2 | 2 |
| `rotate` | 9 | 4 | 3 |
| `transform`, `k` values | `1 + k` | `2 + 3k` | 2 |
| `per-key(σ)`, `k` keys | `1 + k · B(σ)` | `2 + k · C(σ)` | `1 + k · A(σ)` |

A family's **stated bound** is, for each quantity, the maximum over the
components its signature admits (§2.2):

| Family | `B` bytes | `C` units | `A` units | Derivation |
|---|---|---|---|---|
| Quota, sequence, wallet | 25 | 7 | 3 | `Int`: `clamp-shift` is widest and costliest; `reset-shift` also applies in 3 |
| Dispatcher | 9 | 17 | 3 | `Cat`, `k = 5`: `rotate` 9 bytes; `transform` `2 + 15` |
| Order | 9 | 14 | 3 | `Cat`, `k = 4`: `rotate` 9 bytes; `transform` `2 + 12` |
| Lease | 21 | 6 | 5 | `per-key`, `k = 2`: `1 + 2 · 10`, `2 + 2 · 2`, `1 + 2 · 2` |

A candidate is **within the bound class** iff (i) for each of `B`, `C`, `A`
the maximum over all 100 seeds and nine sizes is at most its family's bound
(`C` is undefined at `n = 1`), and (ii) no quantity grows (§4.1). **Reason:**
the numbers follow from the library's carriers and definitions alone, so
anyone can recompute them, and no expected fill entered them.

**Bounded by construction.** In this language every candidate's
representation has the fixed width of its layout, so measured growth of
`B` cannot occur, and a measured value above the bound means the
implementation does not realise §2.1: the document or the implementation
is defective and is re-frozen, voiding prior results, as E2 `protocol.md`
§5 rules for quota. E2's growth rule can act only on the cost counts, where
data varies them between bounded values (an identity operand skips a body;
`reset-shift` runs one arm); a finding there counts like any other.

### 4.5 What a pass does and does not show

As E2 `protocol.md` §5: "no growth detected over the window 64-256 under
this protocol", within the stated bound. It shows nothing about batches
longer than 256 or inputs outside the frozen generators, and it is not a
proof.

## 5. Manifest

`docs/experiments.md` ("Freeze order and authorship") requires the exact
hardware of the ten-minute budget and the retry policy to be recorded
before the gate runs.

### 5.1 Machine

Read on 2026-10-16 with `lscpu`, `/proc/cpuinfo`, `/proc/meminfo` and
`/sys/devices/system/cpu`, as E4 `protocol.md` §9 did. A different machine
re-collects these, and the budget is then a different budget.

| Fact | Value |
|---|---|
| CPU | AMD Ryzen 5 5600X (family 25, model 33, stepping 2), x86_64 |
| Cores / threads | 6 physical, SMT on, 12 logical CPUs (0-11), 1 socket, 1 NUMA node |
| Caches | L1d 192 KiB and L1i 192 KiB (6 each), L2 3 MiB (6), L3 32 MiB (1) |
| Clock | minimum 560.8 MHz, maximum 4654.3 MHz; boost on; driver `amd-pstate-epp`, governor `powersave` |
| Memory | 65 752 620 kB (`MemTotal`) |
| Kernel | Linux 6.17.0-35-generic |
| Toolchain | `rust-toolchain.toml`, 1.99.1 |

The governor is recorded, not changed: the budget is wall-clock on the
machine as it is, and system settings are outside this freeze.

### 5.2 Pinning and recorded conditions

The run takes `/tmp/probe-stack.lock` with `flock` (one probe stack per
machine) and holds it until the last family ends. The search process runs
under `taskset -c 2`. Nothing reserves CPU 2's SMT sibling, CPU 8
(`thread_siblings_list` reads `2,8`), and this part does not claim it stays
idle. Instead the conditions are **recorded, never gating**: a monitor
pinned to the complement of `{2, 8}` samples every 200 ms, as E4
`protocol.md` §9 does, CPU 8's idle field in `/proc/stat`, CPU 2's
`/proc/schedstat` field 8 (time waiting to run) and `/proc/loadavg`, and the
record keeps the series. Before the first family the harness waits while
the 1-minute load average is at or above 18 (1.5 times 12 logical CPUs);
after the start nothing rejects or repeats a family, since that would be a
retry. **Reason:** the ten-minute budget is a coarse wall-clock bound, not a
fine-grained timing, so interference is documented beside the result
rather than filtered out.

### 5.3 Budget and attempt

- **10 minutes of wall-clock per family**, on a monotonic clock, from the
  first candidate proposed to admission or the deadline. It covers
  enumeration, the stage filters, validation, every check of §3 and the
  measurement of §4, because "admitted" means all of them. Process start,
  loading the `Program` and writing the record are outside it.
- **One attempt per family, no retry, no extension** (`docs/experiments.md`,
  "Gate"). A harness crash or host fault fails the gate run **closed**: it
  gives no verdict, and its partial numbers are void. A re-run happens only
  through a logged amendment (a recorded cause and a commit to the run's
  record), never as an automatic restart (decision D9). The rule library's
  retry policy is none.
- Families run in the order of `inputs.md` §3-§8, so the total is at most
  60 minutes plus setup.

### 5.4 Admission, stopping and miss types

A candidate is **admitted** iff it passes, in this order: validation
(§3.1); equivalence and associativity over the checking domain (§3.2,
§3.3); the stated bound (§4.4 (i)); agreement on the family domain (§3.4);
and E2's growth rule on its costs (§4.4 (ii)). A candidate failing any
condition is discarded and the search continues with the next in order.
The clock stops at the first admission; the search does not look further
(decision D6).

A family not admitted is a miss of exactly one type, decided by the
furthest stage any of its candidates reached:

| Type | Furthest stage reached |
|---|---|
| **measured growth** | a candidate failed only E2's growth rule |
| **correctness** | a candidate passed the stage filters and was rejected for disagreeing with `replay` on the checking domain (multi-call batches or associativity), for its own overflow, or on the family domain |
| **expressiveness** | no candidate passed the stage filters, and the argument below proves none in the language can |
| **search** | no candidate passed the stage filters, and no such argument exists: the deadline came, or the capped and pruned enumeration ended |

A measured value above the stated bound is a defect that stops the run
(§4.4), not a miss. The record gives the type and what ended the search;
the verdict counts admissions as `docs/experiments.md` says ("Conjunctive
success condition"), and the type changes no count (decision D7).

**How the two expected misses arise.** Both are expressiveness misses,
provable from §2.1 against one single-call pair of the checking domain:

- **Wallet.** Every component acting on `Int` has an apply that is
  monotone non-decreasing in `x` (a translation, a clamp, a constant, a
  maximum or a minimum), and `ε` is the identity. `debit(3)` maps balance
  `2` to `2` and `3` to `0` (`inputs.md` §7.2; both stores and the call are
  in §7.3's domain), which decreases. No `(valid, atom)` for `debit`
  passes the filter.
- **Lease.** `per-key(σ)` applies `σ` to each key's row on its own, and `σ`
  either maps every row to one value (`assign`) or fixes every present row
  (`fill`, and `ε`). `release(0, 1)` maps holder `1` to `none` and fixes holder `2`
  (stores `(1, free)` and `(2, free)` and the call are in §8.3's domain).
  The library expresses `acquire` (insert-if-absent) and `inspect`, but not
  `release`. This is the honest answer to whether the library can express
  the lease: it cannot (decision D5).

`inputs.md` §7.5 and §8.5 stay expectations: if a candidate of either
family passes the stage filters, the argument is defective, the type
follows the table, and the defect is reported. The other four families are
expected admitted (`inputs.md` §2); a miss there has the type the run
reaches.

### 5.5 Leakage rule

E2's hand-written quota summary (E2 `inputs.md` §2.4) is hidden from the
synthesizer and compared only afterwards (#5 ruling). The enumerator, the
stage filters and the checker read the `Program` and the observation
environments, never that summary, and no E2 summary code is linked into the
synthesizer's build (`kiln-e2` holds families and generators only). This
document is not an input of the run, and it does **not** hide the formulas:
`clamp-shift`'s carrier, combine and apply (§2.1) coincide with E2's
`(a, l, u)` and its combine and apply. The rule hides E2's code and summary
text from the synthesizer, not the designer's knowledge (§5.8). The
comparison of §5.7 runs only after the quota family has ended.

### 5.6 What the run records

Per family, written after the family ends:

- the family, the commits of `inputs.md` and this file, the machine facts of
  §5.1 as re-read at run time, the `taskset` mask, the lock time;
- the budget's start and end on the monotonic clock, CPU 2's frequency at
  start, middle and end, and the monitor series of §5.2;
- the candidate log of §2.7 and the search limits computed from the
  `Program` (§2.4);
- admitted, or the miss type of §5.4 with what ended the search and the
  witness of the furthest rejection;
- for an admitted candidate: its text, layout and size, the certificate of
  §3.5, and per quantity `B`, `C`, `A` at every size the maximum and the
  counts of rising, falling and neutral seeds;
- for quota, the comparison of §5.7.

### 5.7 Comparison with E2's quota summary

After the quota family ends, whatever its outcome, the admitted candidate
(if any) is compared with E2's hand-written summary: bytes, combine and
apply counts over the same batches (in §4.3's units, which add identity-tag
tests to E2's count of 5 and 2), and equality of the two denoted
transformers on every `remaining` in `[0, 1000]`. A difference is reported,
not scored.

### 5.8 Threats to validity

- **The library was designed with knowledge of E2's quota summary.** Its
  designer knew that E2's quota summary is a clamped translation; a
  designer of the same project cannot be kept from that. A quota admission
  therefore shows that the search finds a layout and fills in a library
  that contains the right component, not that it discovered saturation;
  likewise for `transform` and `rotate`.
- **Expected misses decided by the library.** Wallet and lease miss for
  expressiveness (§5.4), so neither reaches the family-domain agreement or
  the growth rule. `inputs.md` §10 item 2 hoped the lease would also show
  that the measurement catches growth; under this library it does not, and
  the mutants (§3.8) remain the checker's only adversarial test.
- **An incomplete search.** Pruning and the survivor cap (§2.4, §2.5) can
  discard the only family-domain-correct term; a miss is scoped to this
  search.
- **Scheduling noise.** The governor is `powersave` and CPU 8 is not
  reserved; the recorded indices (§2.7), not the count at the deadline,
  are the comparable figure.
- **`L = 4`.** Batches longer than four are covered only by the sampled
  family-domain check; a flaw that shows only at length 5 within the
  checking domain may pass the certificate.

## 6. Decisions (head rulings, 16.10.2026)

- **D1, tabulation is enforced semantically** (§1.6, §3.4;
  `docs/experiments.md`, "Gate"). Admission also
  requires agreement with `replay` on the family-domain batches; rules L
  and M are hygiene. Reason: over a finite domain arithmetic and
  comparison encode indicators, so no construct rule bans a table.
- **D2, categoricals by name** (§1.1). Reason: a cardinality rule would
  classify a small quantity or identifier as enumerable.
- **D3, a generic library** (§2.1). Components are textbook structures with
  a source; layout, wiring and terms are search outputs; per-family shapes
  are gone. Reason: a component fitting one family's expected algebra is a
  hidden answer.
- **D4, exact semantics** (§1.3, §1.4, §2.1). Every component is defined in
  one grammar, `clamp` holds for reversed bounds, and a candidate's own
  overflow is a typed rejection. Reason: the implementation must not
  choose meanings.
- **D5, lease** (§5.4). The library cannot express `release`; the lease is
  an expected expressiveness miss. Reason: an honest statement of the
  library's reach.
- **D6, admission and stopping** (§5.4). The clock stops at the first
  candidate passing every condition; a failing candidate is discarded and
  the search continues. Reason: one rule, with no condition that ends a
  search early.
- **D7, bounds and miss types** (§4.4, §5.4). Bounds are derived from the
  library; growth acts on costs only; every miss has one type. Reason: a
  bound chosen around expected fills decides nothing.
- **D8, an incomplete search** (§2.3, §2.5, §2.7). Pruning uses frozen
  observation environments; misses are scoped to this search; the count at
  the deadline is recorded, not promised. Reason: pruning on samples is not
  semantic equivalence.
- **D9, manifest** (§2.5, §5.2, §5.3). One attempt; a crash fails closed;
  conditions are recorded, not assumed; the order is not smallest-first.
  Reason: a manifest records what the run is, not what it hopes.
