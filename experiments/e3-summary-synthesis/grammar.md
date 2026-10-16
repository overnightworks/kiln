# E3 — frozen part B, §1–§2: candidate language, component library and search

This file holds §1 and §2 of E3's part B; `protocol.md` holds §0 and §3–§6,
with the status, record and scope of the whole part. Section numbers are
shared by the two files, so every reference names one place.

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
components, and `inputs.md` §1 classifies state. **Category values come from
this signature, not from the `Program`:** the `Program` stores a category as
an `Integer` column and names no values, so a `Cat(c)`, its values and the key
values of a `Keyed(c)` are the rows above, while parameters, columns and abort
names are declared by the `Program`. A `Cat(c)` value is written
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
| `Keyed(c)` | the keyed state `Cat(c) → Opt(Int)`: one `Opt(Int)` row per value of `c` |
| `Name` | one of the family's frozen names, the list below |
| `Outcome` | `commit(Int)` or `abort(Name)` |

`Name` ranges over the family's **frozen names**, listed by name below, so
no derivation is left to the implementation (the principle of §1.1). A
family's names are its declared aborts, each an `AbortReason::Declared(name)`
of its bodies (`inputs.md` §3-§8), and its structural aborts, written as the
checker compares them (`inputs.md` §1, "What equivalence means"):
`Overflow(Arithmetic)`; `Overflow(Width{table, column, key})` for every integer
column without a clamping domain and every key the family's store can hold
(no program here declares a `ClampDomain`); and
`CheckViolation{table.column cmp n, key k}` for every declared check
constraint and key. `Overflow(Argument)` is in no set: no call of either
domain carries an out-of-width argument (`inputs.md` §5.2, §7.2, §8.2).
The width and check-violation names are †: they occur only in checker form
(a mutant's or a component's term), never in a hole. A structural name marked
*unreachable* is raised by no call on any store of the family domain; it is
listed because the derivation lists it, so the set rests on no reachability
argument.

- **Quota:** `invalid_quantity`, `quota_exhausted`, `quota_full`;
  `Overflow(Arithmetic)`; † `Overflow(Width{quota, remaining, 0})`,
  `CheckViolation{quota.remaining AtLeast 0, key 0}`,
  `CheckViolation{quota.remaining AtMost 1000, key 0}`. All structural names
  unreachable: operands and writes stay in `[0, 1000]`, inside `I32`.
- **Sequence:** `invalid_value`; `Overflow(Arithmetic)` (`next_value` at
  `i64::MAX`, outside both domains, `inputs.md` §4.2);
  † `Overflow(Width{sequence, last_value, 0})` (unreachable: `I64`, so the
  arithmetic overflow aborts first),
  `CheckViolation{sequence.last_value AtLeast 0, key 0}` (unreachable).
- **Dispatcher:** `invalid_count`; `Overflow(Arithmetic)` (unreachable);
  † `Overflow(Width{dispatcher, cursor, 0})` (unreachable),
  `CheckViolation{dispatcher.cursor AtLeast 0, key 0}` (unreachable),
  `CheckViolation{dispatcher.cursor AtMost 4, key 0}` (raised only by mutant
  D-1).
- **Order:** `already_paid`, `order_cancelled`, `not_paid`,
  `already_shipped`, `already_cancelled`, `not_cancelled`;
  `Overflow(Arithmetic)`; † `Overflow(Width{orders, status, 0})`,
  `CheckViolation{orders.status AtLeast 0, key 0}`,
  `CheckViolation{orders.status AtMost 3, key 0}`. All structural names
  unreachable: no arithmetic, and every write is a literal in `[0, 3]`.
- **Wallet:** `invalid_amount`, `insufficient_funds`; `Overflow(Arithmetic)`
  (unreachable: `I16` operands); † `Overflow(Width{wallet, balance, 0})`
  (a credit above 32 767), `CheckViolation{wallet.balance AtLeast 0, key 0}`
  (unreachable).
- **Lease:** `invalid_resource`, `invalid_owner`, `already_held`,
  `held_by_other`, `not_held`, `not_owner`; `Overflow(Arithmetic)`;
  † `Overflow(Width{lease, owner, k})`,
  `CheckViolation{lease.owner AtLeast 1, key k}`,
  `CheckViolation{lease.owner AtMost 1000000, key k}`, each for `k` in
  `0, 1`. All structural names unreachable: no arithmetic, and every
  inserted owner passed the argument test.

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
          | x                      † the applied-to value, kind Int
          | Int mod n              † Euclidean remainder, n a positive literal
Bool    ::= true | false | not Bool | Bool and Bool | Bool or Bool
          | Int rel Int            rel in {<, <=, =, !=, >=, >}
          | Cat = Cat | present(Opt)
          | O.f                    † a Bool field of an operand
Cat     ::= name(v) | state(c) | ite(Bool, Cat, Cat)
          | O.f                    † a Cat(c) field of an operand
          | x                      † the applied-to value, kind Cat(c)
          | lookup(Map, Cat)       † the entry of a table
          | of(c, Int)             † the value of c whose code is the Int
Opt     ::= none | some(Int) | state(c, Cat)
          | ite(Bool, Opt, Opt)
          | O.f                    † an Opt(Int) field of an operand
          | x                      † the applied-to row, kind Opt(Int)
          | x(Cat)                 † the row at a key of a keyed state
Keyed   ::= keyed(Opt_0, …, Opt_{k-1})   † the keyed result, below
Map     ::= table(Cat_0, …, Cat_{k-1})
          | O.f                    † a Map(c) field of an operand
Entry   ::= when(Bool, t_1, …, t_m)   a per-key atom entry, §2.1
Carrier ::= (t_1, …, t_m) | S | T | ite(Bool, Carrier, Carrier)   †
Name    ::= a name of the family's list, §1.2: a declared name, or
            Overflow(Arithmetic)
          | Overflow(Width{t, c, k}) | CheckViolation{t.c cmp n, key k}   †
Outcome ::= commit(Int) | abort(Name) | ite(Bool, Outcome, Outcome)
          | case(Cat, Outcome_0, …, Outcome_{k-1})
```

`state(c)` reads a scalar state component and `state(c, v)` the row of a
keyed component at key `v`. **Key terms are typed:** `v` is a `Cat` term of
the key's category (lease: `Cat(resource)`, for example `name(v0)`), never a
raw integer code; the same holds for `x(v)`. In checker form, `S` and `T` are the left and
right operands of a combine and `s` the summary being applied; a `Carrier`
term is a combine's result. **Operand fields are typed:** `O.f` has the type
of field `f` in the carrier's declaration, and a carrier field is one of
`Int`, `Bool`, `Cat(c)`, `Opt(Int)` or `Map(c)`, so every field of every
carrier, `assign`'s `s.v` included, has a production. **The apply input is
typed:** `x` is the value being applied to, of the kind the component acts
on (§2.1): an `Int` or a `Cat(c)` for a scalar component, and under
`per-key` the key's row, an `Opt(Int)`. `x(v)` reads the row at key `v` of a
keyed state; only a checker-form apply that is not `per-key` needs it
(§3.1), and its result is a `keyed` literal. **The keyed result:**
`keyed(o_0, …, o_{k-1})` is the keyed state whose row at the key of code
`i` is `o_i`, one `Opt(Int)` term per declared key of the component, in key
code order (lease: `keyed(o_0, o_1)`). Its **type rule:** it has type
`Keyed(c)` iff `k` is the number of values of `c` and every `o_i` is an
`Opt(Int)`; a missing or extra row is malformed (§3.1). Its **evaluation**
is strict, all `o_i` left to right, the default rule of §1.4. **Reason:**
every row belongs to the resulting state, so no row is an unchosen arm to
skip, and strictness leaves an overflow in any row a rejection (§1.4).
Where each production may occur. A hole may use exactly the productions
listed for it, in the column of the type it has, and no others; anything else
is a hole violation (§3.1 check 5). *Arithmetic* is `+`, `-`, `min`, `max`,
`clamp`; *comparisons* are `Int rel Int` and `Cat = Cat`; *connectives* are
`true`, `false`, `not`, `and`, `or`. A † production occurs in no hole.

| Hole | `Int` | `Bool` | `Cat` | `Opt` | `Outcome` |
|---|---|---|---|---|---|
| `valid[f]` | `lit`, `param`, arithmetic (comparison operands only) | connectives, `Int rel Int` | none | none | none |
| `atom[f]`, a field | `lit`, `param`, `key` (inside an `Entry` only), arithmetic, `ite` | `Int rel Int`, `Cat = Cat` | `name`, `ite` | `none`, `some`, `ite` | none |
| `respond[f]` | `lit`, `param`, `state(c)`, `code`, `get`, arithmetic, `ite` | `Int rel Int`, `Cat = Cat`, `present` | `name`, `state(c)`, `ite` | `none`, `some`, `state(c, Cat)`, `ite` | `commit`, `abort(Name)` with a non-† name, `ite`, `case` |

An `atom[f]` field of kind `Map(c)` is a `table` and a `per-key` field an
`Entry` (`when`); neither occurs in `valid[f]` or `respond[f]`.

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
- **Evaluation order.** A production evaluates all its operands, left to
  right, except these, which evaluate lazily (a call's
  `atom[f]` is likewise evaluated only when its `valid[f]` holds, §1.8):
  - `ite` and `case` evaluate the condition or scrutinee, then the chosen
    arm only;
  - `a and b` and `a or b` **short-circuit** left to right: `b` is evaluated
    only when `a` is true (for `and`) or false (for `or`), so they equal
    `ite(a, b, false)` and `ite(a, true, b)`. **Reason:** this is how the IR
    decides a conjunction of guards, as nested `Terminal::Branch`es in which
    the first deciding test ends the evaluation (`inputs.md` §1, "Check
    order"), and it gives the connectives the one rule `ite` already has;
  - `get(o, d)` evaluates `o`, then `d` **only when `o` is `none`**; it is
    `ite(present(o), owner of o, d)`. **Reason:** the default is the absent
    arm of that branch, so it follows `ite`'s rule;
  - `when(b, t_1, …, t_m)` evaluates `b`, then the `t_i` only when `b`
    holds. **Reason:** it is `ite(b, (t_1 … t_m), ε)` at its key.

  **Consequence for overflow:** a sub-term that is not evaluated never
  overflows, so a candidate is rejected for overflow only on a sub-term its
  evaluation actually reaches.
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

An `Int` literal must be in the family's **constant pool** `P`, the union of
these value classes, each with its source, and nothing else:

| Value class | Source |
|---|---|
| the integer literals of the function bodies | the `Program` (`inputs.md` §3-§8) |
| the operands of the check constraints | the `Program`'s schema |
| the minimum and maximum of every `IntegerWidth` that a column (its key column included) or a parameter declares | the `Program`'s schema and parameters; the widths below |
| both bounds of every `ClampDomain` | the `Program`'s schema; none of the six declares one, so this class is empty |
| `0` and `1` | this rule |

Width bounds, from `IntegerWidth::range`: `I8` `-128 … 127`; `I16`
`-32 768 … 32 767`; `I32` `-2 147 483 648 … 2 147 483 647`; `I64`
`-9 223 372 036 854 775 808 … 9 223 372 036 854 775 807`. A key column is
`I64`, as quota's `Program` declares it.

| Family | Body literals | Check operands | Widths declared |
|---|---|---|---|
| Quota | `0`, `1`, `50`, `1000` | `0`, `1000` | `I32` (`remaining`, `quantity`), `I64` |
| Sequence | `1`, `1000000` | `0` | `I64` (`last_value`, `value`) |
| Dispatcher | `0`, `1`, `4`, `5` | `0`, `4` | `I8` (`cursor`, `count`), `I64` |
| Order | `0`, `1`, `2`, `3` | `0`, `3` | `I8` (`status`), `I64` |
| Wallet | `1`, `100` | `0` | `I16` (`balance`, `amount`), `I64` |
| Lease | `0`, `1`, `1000000` | `1`, `1000000` | `I8` (`resource`), `I32` (`owner`), `I64` |

A key code enters only through `key`. The checking domain's values are never
literals: wallet balance `32 666` or lease owner `3` cannot be written.
**Reason:** `inputs.md` §1 allows semantic constants in arithmetic and guards
(5, 32 767, 1000, 1 000 000) and nothing else; each of those is a body
literal, a check operand or a width bound above.

### 1.6 What the grammar does not enforce

Over a finite checking domain, any language with addition and comparison
can encode an indicator: `clamp(x, 0, 1) - clamp(x - 1, 0, 1)` is `1` at
`x = 1` and `0` at every other integer, and `1 + 1 + 1` builds the
unavailable literal `3`. No construct-level rule therefore makes a quantity
table unrepresentable, and this part does not claim one. The tabulation ban
of `inputs.md` §1 is enforced **semantically** by the checker contract: an
admitted candidate must also agree with `kiln_ir::replay` on batches drawn
from the family's full domain (§3.4), where a table of the small checking
domain is wrong (decision D1). That agreement is a sampled admission
condition; it does not prove that no table passes.

Two rules stay, as **search-space hygiene only**; they shrink the space and
enforce nothing:

- **Rule L.** Literals come from the pool (§1.5).
- **Rule M.** A table is indexed by a `Cat` only, and no hole converts an
  `Int` to a `Cat` (`of` is checker-form only). It is implied by §3.1
  check 2 (a `lookup` takes a `Cat`) and check 5 (`lookup` and `of` are †, so
  no hole holds them), and needs no implementation of its own; it stays
  listed as hygiene.

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

The **size** of a term is its node count, uniformly 1 plus the sizes of its
parts: a literal, a reference (`param`, `key`, `state(c)`, `x`, `O.f`) and a
leaf (`none`, `name(v)`, `true`, `abort(Name)`) are 1, and `some(t)`,
`state(c, t)`, `ite`, `case`, `table`, `Entry`, `commit` and every operator
are 1 plus their sub-terms. A payload (a number, a name, a component, a
relation) is not a part. It orders the search (§2.5). The **bytes** `B` of a summary are
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
summary of a call is `atom[f]` if `valid[f]` holds, else `ε`, and `atom[f]`
is evaluated only in the first case; its outcome is always `respond[f]`. This is the decomposition E2 uses for its hand
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
overflow. No component names a family, a field or a function. Each one's
generality rests on its **source**: a textbook structure, or for
`clamp-shift` the derivation below from cited identities, with a use outside
these six families (decision D3). The selection was informed by the
families (§5.8); the sources, not the selection, carry D3.

| Component | Kind it acts on | Carrier | Combine body `S ⊕ T` | Apply body `apply(s, x)` | Source |
|---|---|---|---|---|---|
| `shift` | `Int` | `a: Int` | `(S.a + T.a)` | `x + s.a` | the additive group; prefix sums (Blelloch, "Prefix sums and their applications", 1990) |
| `clamp-shift` | `Int` | `a, l, u: Int` | `(S.a + T.a, clamp(S.l + T.a, T.l, T.u), clamp(S.u + T.a, T.l, T.u))` | `clamp(x + s.a, s.l, s.u)` | the composition closure of saturating additions, derived below from lattice-ordered group identities; used in bounded counters and token buckets |
| `reset-shift` | `Int` | `r: Bool, v: Int, a: Int` | `ite(T.r, T, (S.r, S.v, S.a + T.a))` | `ite(s.r, s.v, x) + s.a` | the segmented-scan operator (Blelloch 1990): a sum restarted at a flag |
| `raise` | `Int` | `m: Int` | `(max(S.m, T.m))` | `max(x, s.m)` | the max semilattice (a max-register) |
| `lower` | `Int` | `m: Int` | `(min(S.m, T.m))` | `min(x, s.m)` | the min semilattice |
| `assign` | `Int`, `Cat(c)`, `Opt(Int)` | `v` of the kind | `T` | `s.v` | the right zero semigroup, `xy = y` (Howie, *Fundamentals of Semigroup Theory*, 1995): ordered overwrite, the later write in the batch order wins |
| `fill` | `Opt(Int)` | `v: Int` | `S` | `ite(present(x), x, some(s.v))` | the left zero semigroup, `xy = x` (Howie 1995), acting by insert-if-absent: the earlier write in the batch order wins (SQL `ON CONFLICT DO NOTHING`) |
| `rotate` | `Cat(c)`, `k` values | `a: Int` | `((S.a + T.a) mod k)` | `of(c, (code(x) + s.a) mod k)` | the cyclic group `Z_k` acting on a `k`-cycle; E2's inventory is a translation mod 91 |
| `transform` | `Cat(c)`, `k` values | `m: Map(c)` | `(table(lookup(T.m, lookup(S.m, v0)), …, lookup(T.m, lookup(S.m, v(k-1)))))` | `lookup(s.m, x)` | the full transformation monoid; composing finite-state transition tables (Hillis and Steele, "Data parallel algorithms", 1986) |
| `per-key(σ)` | `Keyed(Cat(c) → Opt(Int))`, `k` keys | one value of `σ` or `ε` per key | per key, `σ`'s combine | per key, `σ`'s apply to that key's row | the product of `k` copies of a monoid, indexed by a finite set |

`σ` ranges over the components acting on `Opt(Int)` (`assign`, `fill`). A
`per-key` atom is one `Entry` per key: `when(b, t_1 … t_m)` is `σ`'s value
`(t_1 … t_m)` at that key if `b` holds, else `ε` there. The summary of a
whole state is the product of one component value per signature component,
combined and applied component by component; every family here has one
signature component, so its product has one factor.

**`assign` is ordered overwrite, not last-writer-wins arbitration.** In a
sequential composition "first `S`, then `T`" the later write wins because
the batch order puts it later; combine is not commutative, and no timestamp
or tie-break enters it. Last-writer-wins in Shapiro et al. ("Conflict-free
replicated data types", SSS 2011) arbitrates between concurrent writes that
have no order, by timestamp; nothing here is concurrent, so that register
is neither this component's source nor its meaning.

**`clamp-shift`, derived.** Write `p ∧ q` for `min(p, q)` and `p ∨ q` for
`max(p, q)`, so `clamp(z, l, u) = (z ∨ l) ∧ u` (§1.4). The integers under
`+`, `∧`, `∨` form a totally ordered, hence lattice-ordered, group. The
derivation uses three identities, each standard (Birkhoff, *Lattice
Theory*, 3rd ed., AMS Colloquium Publications 25, 1967: the chapter on
lattice-ordered groups for I1, distributive lattices and chains for I2, the
lattice axioms for I3):

- I1, a translation is a lattice automorphism: `t + (p ∧ q) = (t + p) ∧ (t
  + q)` and `t + (p ∨ q) = (t + p) ∨ (t + q)`;
- I2, a chain is a distributive lattice: `p ∨ (q ∧ r) = (p ∨ q) ∧ (p ∨ r)`
  and `(p ∧ q) ∨ r = (p ∨ r) ∧ (q ∨ r)`;
- I3, `∧` and `∨` are associative and commutative, and absorb: `u ∧ (u ∨
  z) = u`.

Step 1, a shift passes through a clamp: `clamp(z, l, u) + a = ((z ∨ l) ∧ u)
+ a = ((z ∨ l) + a) ∧ (u + a)` (I1) `= ((z + a) ∨ (l + a)) ∧ (u + a)` (I1)
`= clamp(z + a, l + a, u + a)`.

Step 2, nested clamps absorb, for any bounds (no `L ≤ U` or `l ≤ u` is
assumed, so reversed bounds are covered):

1. `clamp(clamp(z, L, U), l, u) = (((z ∨ L) ∧ U) ∨ l) ∧ u` (definition);
2. `= ((z ∨ L ∨ l) ∧ (U ∨ l)) ∧ u` (I2, second form, then I3);
3. `clamp(z, clamp(L, l, u), clamp(U, l, u)) = (z ∨ ((L ∨ l) ∧ u)) ∧ ((U ∨
   l) ∧ u)` (definition);
4. `= (z ∨ L ∨ l) ∧ (z ∨ u) ∧ (U ∨ l) ∧ u` (I2, first form, then I3);
5. `(z ∨ u) ∧ u = u` (I3, absorption), so line 4 equals line 2.

Step 3, closure and the composed triple: `apply(T, apply(S, x)) =
clamp(clamp(x + S.a, S.l, S.u) + T.a, T.l, T.u)` `= clamp(clamp(x + S.a +
T.a, S.l + T.a, S.u + T.a), T.l, T.u)` (Step 1) `= clamp(x + (S.a + T.a),
clamp(S.l + T.a, T.l, T.u), clamp(S.u + T.a, T.l, T.u))` (Step 2). The
composite is again `x ↦ clamp(x + a, l, u)`, so the maps are closed under
composition, and its triple is exactly the combine in the table.

Step 4, associativity on values: both groupings of three summaries have
`a = a_1 + a_2 + a_3`; the `l` of `(S_1 ⊕ S_2) ⊕ S_3` is `clamp(clamp(l_1 +
a_2, l_2, u_2) + a_3, l_3, u_3)`, which Steps 1 and 2 turn into `clamp(l_1 +
a_2 + a_3, clamp(l_2 + a_3, l_3, u_3), clamp(u_2 + a_3, l_3, u_3))`, the `l`
of `S_1 ⊕ (S_2 ⊕ S_3)`; `u` is the same with `u_1` for `l_1`. E2 `inputs.md`
§2.4 proves the same lemma by cases for ordered bounds; this derivation
needs no ordering and no E2 text.

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
family name or number. From the `Program` it reads the pool and the `Name` set;
the signature of §1.1 is part of this freeze, not an input. The observation environments are frozen:

- for `valid[f]` and `atom[f]`: the arguments of every call of `f` in the
  checking domain's alphabet `A`, malformed ones included;
- for the `Entry` of key `v` in a `per-key` atom: those arguments with
  `key` bound to `v`'s code; there is one `Entry` hole per key of the
  family's categorical key set, in code order (lease: keys `0`, `1`), so
  every `key` an `Entry` can read is bound;
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
| size of any hole of function `f` | the node count of `f`'s body in `kiln_ir`, computed from the `Program` at run start and logged: every `Effect`, `Terminal`, `Condition` and `Expr` node counts 1 plus its parts, a `Block` is a wrapper and counts 0, and names, slots, operators and literal values are payload | a term describing part of `f` needs no more nodes than `f` itself; the number comes from the `Program`, not from a known fill |
| survivors per hole group (§2.6) | 64, in enumeration order | bounds the product of per-function survivors; later members are not tried |

**Example, quota** (`crates/kiln-e2/src/quota/program.rs`). A bounds test
`Branch(Compare(Param, Lit), Abort)` is 1 + (1 + 1 + 1) + 1 = 5; a `read` of
a literal key is 1 + 1 = 2. `allocate`: the two argument tests 5 + 5; the
`read` and the `remaining = 0` test 2 + 5 = 7; the final branch 1 + 3
(`remaining >= quantity`) + 7 (`update` 1 + 1 + 3, `Return` 1 + 1) + 5
(`update` 1 + 1 + 1, `Return` 1 + 1) = 16; in all 5 + 5 + 7 + 16 = 33.
`refill`: 5 + 5 + 7, then 1 + 5 (`remaining + quantity <= 1000`) + 7 + 7 =
20; in all 37. The program's count is 33 + 37 = 70, and a hole of `allocate`
has size limit 33, of `refill` 37.

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
   categories in code order); each hole's kept terms (pruning, below) are
   numbered in this order;
4. within the group `(valid[f], atom[f])`, a member is a tuple of one kept
   term per hole: `valid[f]`, then the atom's holes in the layout's
   declared field order (§2.1's carrier, e.g. `a, l, u`; for `per-key` the
   `Entry` holes in key code order). Tuples are enumerated by ascending
   total size, the sum of their terms' sizes, then lexicographically by the
   holes' term numbers, `valid[f]` varying slowest and the last field
   fastest; the group's survivors are the first 64 tuples in this order
   that pass the stage filter (§2.4, §2.6). `respond[f]` is a group of one
   hole, its survivors the first 64 passing terms in item 3's order.
   **Reason:** a plain lexicographic product would try every atom tuple
   under the first `valid[f]` before the second, so a small correct tuple
   could sit behind a vast number of larger ones; ascending total size
   extends item 3's rule to the product;
5. a candidate is a choice of one survivor per hole group; candidates are
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

**Evaluation errors during pruning.** A term whose evaluation errors on an
environment (in a hole, only an overflow, §1.4) is **kept**: the error is
one more observed value, equal to every other error, so two terms are one
class iff they agree on every environment, errors included. **Reason:** a
term may error where a candidate never evaluates it (an `atom[f]` on a
malformed call where `valid[f]` is false, or a lazy arm, §1.4), so dropping
it could discard an admissible candidate; where the checker would evaluate
it, the stage filter rejects it (§2.6).

### 2.6 Stage filters

Before a candidate reaches the checker, its hole groups pass two filters,
each a subset of the checker's own one-call checks (§3.2):

- `(valid[f], atom[f])`: for every store `x` of `X` and every call of `f`
  in `A`, applying the call's summary to `x` gives `replay`'s store after
  that single call, read through the signature;
- `respond[f]`: for every such pair, the outcome equals `replay`'s.

Each filter depends on one function's holes only, so functions are filtered
independently and only survivors are combined. An overflow on such a pair
fails the filter, as it fails the checker (§1.4). A filter rejects only
what the checker would reject, so it never discards an admissible
candidate.

### 2.7 Candidate count

The enumerator logs per family: the size of the enumeration space per
layout and hole, counted without evaluation before the run; for every
candidate that reaches a check, its index in the order of §2.5 and the
stage that rejected or admitted it; per hole the terms tried, pruned and
kept, and per hole group the tuples tried and surviving; and the index of the admission or of the last candidate tried.
**Determinism** means the enumeration order is a function of the `Program`,
the observation environments and this part alone, so every index is
reproducible. How far the search gets within ten minutes depends on the
machine; the count reached at the deadline is recorded, not promised.
