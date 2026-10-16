# E3 — frozen part B: candidate language, component library, checker and manifest

Status: frozen. Record: drafted in #83 by an author other than the one who
froze the families, as `docs/experiments.md` ("Freeze order and
authorship") requires, and passed by an independent reviewer of a different
provider on 2026-10-16. Part A is `inputs.md` (the six families,
their checking domains, the 52 mutants); this file and `grammar.md` are
part B. Nothing in either
changes after a synthesis attempt has run; a change re-freezes this part and
voids every result measured under the earlier one.

## 0. Scope

It freezes, in the order of #83: the candidate language (§1), the component
library and the search (§2), the checker's soundness contract (§3), the
bound-class measurement (§4) and the manifest (§5). §6 records the head's
rulings of 16.10.2026 as decisions with their reasons. Part B writes no code,
measures nothing, and states no summary for any family and no verdict.

Every frozen value carries a one-clause reason. Owners: `docs/experiments.md`
(E3), `docs/concept.md` §6 and §9.4, `inputs.md` §1 and §10, and E2's
`protocol.md` (the growth rule). The reference meaning is `crates/kiln-ir`:
`replay(&Program, &[Call], &Store) -> Result<Vec<Execution>, SchemaError>`,
with `Execution { store, outcome, access_log }` and
`Outcome::{Committed { response }, Aborted { reason }}`.

§1 (the candidate language) and §2 (the component library and the search)
are in [`grammar.md`](grammar.md); section numbers run across both files.

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

Validation is a pure function of its input and the `Program`. It runs the
lists below in order and stops at the first failure with a typed reason. A
malformed input is thereby told apart from one that evaluates and is wrong,
as `docs/experiments.md` asks ("typed rejection, distinct from an abort").

**Checker-form validation**, for every input of the checker, a candidate or
a mutant (§3.8):

1. **Declared:** per signature component, a carrier whose fields each have
   a type of the language (`Int`, `Bool`, `Opt(Int)`, or `Cat(c)` or
   `Map(c)` for a category `c` of the signature), a combine and an apply;
   per function, `valid`, `atom` and `respond`; every parameter, state
   component and `Name` declared by the `Program` (§1.2), and every category
   and category value declared by the signature (§1.1).
2. **Well-typed:** every term has its §1.3 type, including the category of
   every `Cat`. Combine is a `Carrier` term over `S`, `T`, literals and
   the operators; apply is a term of the component's kind (§1.3, "the apply
   input is typed") over `s`, `x` and, for a keyed state, `x(v)`, its result a `keyed` literal; `atom` is
   a value of the declared carrier (or an `Entry` per key); `O.f` names a
   declared field; operands are bound only there. Under `per-key(σ)` the
   carrier, combine and apply are `σ`'s, run once per key (§2.1).
3. **Total:** every declared operation is defined on every input of its
   type: each `case` has one arm per value, each `table` one entry per
   value, and no part listed in 1 is missing. An error during evaluation
   (overflow, `of` outside the codes, §1.4) stays a typed rejection at
   evaluation, not a validation failure.

Waived in checker form: membership in the library (§2.1, §2.2), the
per-hole production lists (§1.3), rules L and M (§1.6) and
the search limits (§2.4). **Reason:** these bound or tidy the search and
enforce nothing (§1.6); applying them to a mutant would let it fail
structurally instead of reaching the semantic checks it was written for.
**"Not expressible"** in `inputs.md` §1 means not expressible in checker
form, the language of this list, and means only that wherever it is used.

**Candidate validation**, additionally, for a candidate of the enumerator:

4. **Library:** every component is in §2.1, of a kind §2.2 tries for its
   signature component, with exactly §2.1's carrier, combine and apply.
5. **Holes:** every hole term uses only the productions its hole allows
   (§1.3).
6. **Literals** are in the pool (rule L, §1.5). Rule M has no check of its
   own (§1.6).
7. **Search limits** of §2.4 hold.

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

S4 writes each preregistered mutant (`inputs.md` §9) in checker form: its
own carrier and `valid`, `atom`, combine, apply and `respond` terms of §1.3,
which may use the † productions, so a mutant's deviating combine (an `N`,
`O` or `T` mutant) is written directly and is not limited to the library;
§2.1's adjoined identity holds for its carrier too. Each is run through
§3.1's checker-form validation (checks 1-3 only), §3.2 and §3.3; a `Valid`
result is a false acceptance and fails E3 (`docs/experiments.md`,
"Conjunctive success condition"). A mutant checker form cannot express is
recorded as "not expressible" in §3.1's sense, with its reason, never
dropped. A mutant's behaviour is the one `inputs.md` freezes, whatever
text expresses it. Mutants are
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
- **branch test** executed: an identity-tag test, or the decision of an
  `ite`, `case`, `get`, `when`, `and` or `or`. The Boolean term deciding it
  (a comparison, `present`, `Cat =`, `not`, a field read) is part of that
  one test and adds nothing, and an arm that does not run costs nothing.
  Library bodies evaluate Boolean terms only as such decisions;
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
only on the layout. Per component, from §1.7 and §4.3, at most: a combine
of two values pays two tag tests and its body, an apply of a value one tag
test and its body, and an identity operand pays less (it skips the body):

| Component | `B` bytes | `C` units at most | `A` units at most |
|---|---|---|---|
| `assign` | `1 + 8` (`Int`), `1 + 1` (`Cat`), `1 + 9` (`Opt(Int)`) | 2: two tags | 1: one tag |
| `shift`, `raise`, `lower` | `1 + 8 = 9` | 3: two tags, one `+`, `max` or `min` | 2: one tag, one operation |
| `reset-shift` | `1 + 1 + 8 + 8 = 18` | 4: two tags, the `ite` on `T.r`, one `+` | 3: one tag, the `ite` on `s.r`, one `+` |
| `clamp-shift` | `1 + 3 · 8 = 25` | 7: two tags, three `+`, two `clamp` | 3: one tag, one `+`, one `clamp` |
| `fill` | `1 + 8 = 9` | 2: two tags | 2: one tag, the `ite` on `present(x)` |
| `rotate` | `1 + 8 = 9` | 4: two tags, one `+`, one `mod` | 3: one tag, one `+`, one `mod` |
| `transform`, `k` values | `1 + k` | `2 + 3k`: two tags; per entry two reads and one write | 2: one tag, one read |
| `per-key(σ)`, `k` keys | `1 + k · B(σ)` | `2 + k · C(σ)`: two tags, then `σ`'s combine per key | `1 + k · A(σ)`: one tag, then `σ`'s apply per key |

**Worked, for the lease.** `fill`'s apply of a value: its tag test (1),
then the single decision of `ite(present(x), x, some(s.v))` (1), so
`A(fill) = 2`. `per-key(fill)` with `k = 2`: `A = 1 + 2 · 2 = 5`, `C = 2 +
2 · 2 = 6`, `B = 1 + 2 · 9 = 19`. `per-key(assign)`: `A = 1 + 2 · 1 = 3`,
`C = 2 + 2 · 2 = 6`, `B = 1 + 2 · 10 = 21`. The lease's bound is the larger
of the two per quantity: `B = 21`, `C = 6`, `A = 5`.

A family's **stated bound** is, for each quantity, the maximum over the
components its signature admits (§2.2):

| Family | `B` bytes | `C` units | `A` units | Derivation |
|---|---|---|---|---|
| Quota, sequence, wallet | 25 | 7 | 3 | `Int`: `clamp-shift` gives all three; `reset-shift`'s `A` is also 3 |
| Dispatcher | 9 | 17 | 3 | `Cat`, `k = 5`: `rotate` gives `B` and `A`; `transform` gives `C = 2 + 3 · 5` |
| Order | 9 | 14 | 3 | `Cat`, `k = 4`: `rotate` gives `B` and `A`; `transform` gives `C = 2 + 3 · 4` |
| Lease | 21 | 6 | 5 | `per-key`, `k = 2`: the worked lines above |

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
  moment enumeration for the family begins, before any pruning, filtering
  or candidate assembly, to admission or the deadline. It covers
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
and E2's growth rule on its costs (§4.4 (ii)). A candidate failing a
condition is discarded and the search continues with the next in order,
except for a validation defect or a value above the stated bound, which
stop the run (below). The clock stops at the first admission; the search does not look further
(decision D6).

A family not admitted is a miss of exactly one type. A candidate exists
only once every hole group it draws on has passed its stage filter (§2.6),
so every candidate has passed stage F; its later stages run in the order
above. Each stage ends in one of three ways:

| Stage, in order | passed | failed | interrupted by the deadline |
|---|---|---|---|
| F, stage filters (§2.6) | the candidate is assembled | that group member joins no candidate | search, unless "No candidate" below gives expressiveness |
| V, validation (§3.1) | next stage | defect: the enumerator wrote a term outside §1 | search |
| K, checking domain (§3.2, §3.3; overflow, §1.4) | next stage | correctness | search |
| Bd, stated bound (§4.4 (i)) | next stage | a value above the bound: defect (§4.4); the candidate's own overflow: correctness | search |
| FD, family domain (§3.4) | next stage | correctness | search |
| G, growth rule (§4.4 (ii)) | admitted; the clock stops | measured growth | search |

**Precedence.** Each candidate's **terminal stage** is the first stage it
did not pass, failed or interrupted; stages run in the order F < V < K < Bd
< FD < G. The miss is classified from the candidate whose terminal stage is
furthest in that order, ties broken by the earliest in §2.5's order, and its
type is that terminal stage's entry in the failed or interrupted column.
**Reason:** the furthest terminal stage is the most specific evidence the run
produced, and the earliest tied candidate's result does not depend on how
far the machine got before the deadline (§2.7).

**No candidate.** If none was assembled, whether the clock expired
before any candidate was assembled or the capped and pruned enumeration
ended, the miss is **expressiveness** iff an
argument from §2.1 proves that, for some function, no `(valid, atom)` (or
no `respond`) of any layout the library offers passes its filter, and
**search** otherwise. **Reason:** such an argument covers every term the
library can build, so neither the deadline nor the cap decided the outcome;
"expressiveness" always refers to the library, never to checker form.

A **defect** is not a miss: it stops the run, the family gets no verdict
and its partial numbers are void, as for a harness fault (§5.3), and the
document or implementation is re-frozen (§4.4). The record gives the type,
the deciding candidate's index and stage, and what ended the search
(admission, deadline, exhausted enumeration or defect). The verdict counts
admissions as `docs/experiments.md` says ("Conjunctive success condition"),
and the type changes no count (decision D7).

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

- **The component selection was informed by the families.** Its designer
  knew the six families and that E2's quota summary is a clamped
  translation; a designer of the same project cannot be kept from that, and
  a family-informed selection can encode answer shapes. D3 therefore does
  not rest on an uninformed selection: it rests on each component having a
  generic source or, for `clamp-shift`, a derivation from cited identities
  (§2.1). A quota admission shows that the search finds a layout and fills
  in a library that contains the right component, not that it discovered
  saturation; likewise for `transform` and `rotate`.
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
  `docs/experiments.md`, "Gate"). Admission also requires agreement with
  `replay` on the family-domain batches, a sampled admission condition that
  does not prove a table impossible; rules L and M are hygiene. Reason:
  over a finite domain arithmetic and comparison encode indicators, so no
  construct rule bans a table.
- **D2, categoricals by name** (§1.1). Reason: a cardinality rule would
  classify a small quantity or identifier as enumerable.
- **D3, a generic library** (§2.1, §5.8). Each component has a textbook
  source or a derivation from cited identities; layout, wiring and terms
  are search outputs; per-family shapes are gone. The selection was
  informed by the families, so D3 rests on the sources, not on the
  selection. Reason: a component fitting one family's expected algebra is
  a hidden answer.
- **D4, exact semantics** (§1.3, §1.4, §2.1, §3.1). Every component is
  defined in one grammar with typed operand fields and apply input; `clamp`
  holds for reversed bounds; `and`, `or`, `get`, `ite`, `case` and `when`
  are lazy and everything else strict; a candidate's own overflow is a
  typed rejection; checker-form validation is its own list. Reason: the
  implementation must not choose meanings.
- **D5, lease** (§5.4). The library cannot express `release`; the lease is
  an expected expressiveness miss. Reason: an honest statement of the
  library's reach.
- **D6, admission and stopping** (§5.4). The clock stops at the first
  candidate passing every condition; a failing candidate is discarded and
  the search continues. Reason: one rule, with no condition that ends a
  search early.
- **D7, bounds and miss types** (§4.3, §4.4, §5.4). Bounds are derived
  from the library with one unit per branch test; growth acts on costs
  only; every miss has one type, read from the furthest terminal stage.
  Reason: a bound chosen around expected fills decides nothing.
- **D8, an incomplete search** (§2.3, §2.5, §2.7). Pruning uses frozen
  observation environments with `key` bound, keeps erroring terms, and the
  group product runs by total size; misses are scoped to this search; the
  count at the deadline is recorded, not promised. Reason: pruning on samples is not
  semantic equivalence.
- **D9, manifest** (§2.5, §5.2, §5.3). One attempt; a crash fails closed;
  conditions are recorded, not assumed; the order is not smallest-first.
  Reason: a manifest records what the run is, not what it hopes.
- **Clarifications of 16.10.2026, item #100,** made before any synthesis
  attempt, so they void no result: frozen name sets (§1.2), typed key terms
  and the category source (§1.1, §1.3, §3.1), hole production lists (§1.3),
  pool bounds (§1.5), uniform size counting (§1.7, §2.4), rule M as hygiene
  (§1.6).
