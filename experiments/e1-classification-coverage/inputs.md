# E1 — frozen inputs

Status: **frozen**, pending independent review. This document is #2's first
deliverable under the freeze protocol in `docs/experiments.md` ("Deferred to
#2"): the corpus, the transaction-enumeration rule, and the trace source and
weights, decided before any footprint extractor or classifier exists and
before any application's code has been looked at for classification
purposes. Nothing below changes after a classification result is seen — a
change here, after extraction has started, voids every result produced
under the prior version (`docs/experiments.md`, "Freeze protocol").

No extractor, classifier, or other code was written to produce this
document. Every fact below was either read directly from a fetched primary
source (cited with its URL) or verified by a command whose output is quoted
or described. `experiments/e1-classification-coverage/corpus-manifest.csv`
is the machine-readable form of §2's application set.

## 1. TPC-C: specification version and transaction mix

**Pinned specification.** TPC Benchmark™C, Standard Specification,
**Revision 5.11, February 2010**
([PDF](https://www.tpc.org/TPC_Documents_Current_Versions/pdf/tpc-c_v5.11.0.pdf),
fetched 2026-10-15; title page reads "Standard Specification / Revision
5.11 / February 2010"). This is the current version published at
<https://www.tpc.org/tpc_documents_current_versions/current_specifications5.asp>
(fetched 2026-10-15); 5.11 is an editorial-only revision of 5.10.1, so no
later technical revision exists to pin instead.

**Transaction mix (TPC-C's own weights, used as E1's frequency weights for
TPC-C per the third concept critique, `docs/concept.md` §14).** Clause 5.2.3
("Mix of Transactions," spec p. 70) states a minimum percentage of mix for
four of the five transaction types, over the measurement interval:

| Transaction type | Minimum % of mix |
|---|---|
| New-Order | n/a — "There is no minimum for the New-Order transaction as its measured rate is the reported throughput." |
| Payment | 43.0 |
| Order-Status | 4.0 |
| Delivery | 4.0 |
| Stock-Level | 4.0 |

The spec leaves the exact mix to the "test sponsor," subject only to these
four minimums (Clause 5.2.4.1) or an equivalent weighted deck (Clause
5.2.4.2, whose own illustrative example — 10 New-Order and 10 Payment cards
plus one card each for the other three types, in a 23-card deck — already
satisfies every minimum, at 43.5%/43.5%/4.3%/4.3%/4.3%). E1 freezes one
specific, fully determined mix now, so nothing about "the" TPC-C mix is
decided after a result is seen:

**Frozen TPC-C weights: New-Order 45.0%, Payment 43.0%, Order-Status 4.0%,
Delivery 4.0%, Stock-Level 4.0%.** Reason: Payment/Order-Status/Delivery/
Stock-Level are fixed at the spec's own stated minimums (satisfying Clause
5.2.3 with equality); New-Order itself has no spec-stated minimum, so its
share is a choice, not a derivation — 45.0% is fixed here as the remainder
that sums the five to exactly 100%, which is also the mix every public
TPC-C-style benchmark harness this review is aware of uses in practice
(BenchmarkSQL, py-tpcc, HammerDB, OLTPBench). Freezing this now, rather than
after seeing which mix makes TPC-C's coverage number more favorable, is what
keeps the weighting honest.

## 2. The application corpus

**The 111-application study published its own application list as part of
the paper itself — confirmed, used directly as the candidate population; no
substitute selection rule is needed.**

Nguyen, Niechen, DeCarolis & Abadi, "Are Database System Researchers Making
Correct Assumptions About Transaction Workloads?", *Proc. ACM Manag. Data* 3,
3 (SIGMOD), Article 131 (June 2025), <https://doi.org/10.1145/3725268>
([PDF](https://www.cs.umd.edu/~abadi/papers/database-workloads.pdf), fetched
and read in full 2026-10-15; text extracted with `pdftotext -layout` and
cross-checked with `pdftotext -raw`, both agreeing).

Tables 2 and 3 (paper pp. 24–26) list all 111 applications the study
annotated, each with its exact `Owner/Repository` and an abbreviated **7-hex-
character git commit hash** ("Githash" column) the study pinned its
annotation to — a published artifact in the paper's own text, not merely a
percentage. §3.1 states the population rule the study itself used to build
this list, which E1 inherits as the already-published selection rule rather
than writing a new one: GitHub code search for Django (`django.db`) or
TypeORM (`createQueryBuilder`) usage, Django repositories required to have
at least 1000 GitHub stars, TypeORM repositories at least 300 stars,
repositories excluded where ORM usage was only in documentation or in an
example application, "exhausted all web applications meeting our criteria
on GitHub." Final counts stated by the paper: 78 Django applications, 33
TypeORM applications, 111 total.

### 2.1 Reconstructing the list from the paper's tables

The paper's tables are typeset, not distributed as a separate data file —
no companion dataset repository exists: the one GitHub link the paper gives
(<https://github.com/umd-dslam/splinter>, the annotation tool) was fetched
and inspected (`gh api repos/umd-dslam/splinter/git/trees/main?recursive=1`,
2026-10-15) and contains only the VS Code extension's source, no corpus
manifest. The list was therefore reconstructed by extracting Tables 2 and 3
from the fetched PDF (`pdftotext -layout` and `-raw`, agreeing) into 111
rows of `Owner/Repository`, stars, and abbreviated commit hash. Two rows'
`Owner/Repository` cell lost its `owner/` prefix in extraction (a PDF
layout artifact, not a data error — the stars and commit-hash columns for
both rows extracted correctly and are unaffected): `cool-admin-midway` and
`decompiler-explorer`. Each was re-identified by web search and confirmed
by resolving its stated commit hash against a live clone:

- `cool-admin-midway` → **`cool-team-official/cool-admin-midway`** (admin
  dashboard on Midway.js/TypeORM; confirmed via
  `git cat-file -t edc1383` against a clone of that repository, 2026-10-15,
  returning `commit`).
- `decompiler-explorer` → **`decompiler-explorer/decompiler-explorer`**
  (confirmed the same way against commit `0df1700`, 2026-10-15, returning
  `commit`; the GitHub organization `Vector35` redirects to the same
  repository).

### 2.2 Verification: every pinned commit checked to exist

Every one of the 111 rows' `Owner/Repository` + abbreviated hash was
resolved and verified with `gh api repos/<owner>/<repo>/commits/<hash>`,
which both confirms the commit exists in that repository and expands the
abbreviated hash to its full 40-character SHA (run 2026-10-15; GitHub API
rate limit confirmed sufficient beforehand with `gh api rate_limit`). **110
of 111 resolved successfully.** One did not:

- **`nukeop/nuclear` @ `0da69f7` — excluded, could not be verified.**
  `gh api repos/nukeop/nuclear/commits/0da69f7` returned HTTP 422, "No
  commit found for SHA: 0da69f7." The repository itself exists, is not a
  fork, and is not archived (`gh api repos/nukeop/nuclear`). The same
  7-character string was independently confirmed from both `pdftotext`
  extraction modes, so this is not a text-extraction misread. A direct
  `git fetch origin 0da69f7` (and two nearby 8-character guesses) against a
  fresh clone of the live repository also failed to resolve it
  (`fatal: couldn't find remote ref 0da69f7`). The most likely explanation
  is that the commit the study annotated in 2024 was later made
  unreachable by a history rewrite (a force-push, squash, or rebase on
  `nukeop/nuclear`'s default branch) and has since been garbage-collected
  on GitHub's side — this is exactly the failure mode "never invent a SHA"
  exists to catch, so the application is dropped from the frozen corpus
  rather than silently repointed at a different, unverified revision.

**Frozen application-set size: 110** (78 Django, 32 TypeORM — the paper's
78/33 split, minus the one excluded TypeORM application). The full list,
with each repository's URL and verified full commit SHA, is
`experiments/e1-classification-coverage/corpus-manifest.csv`.

## 3. The transaction-enumeration rule

**What counts as a transaction function, for both Django and TypeORM code,**
decided now so a builder cannot widen or narrow it after seeing which
choice raises coverage:

**Rule.** A transaction function is the smallest of the following two
shapes that applies, in order:

**(a) An explicit transaction block.** In Django: the body of a function or
method decorated with `@transaction.atomic(...)`, or the body of a
`with transaction.atomic(...):` block. In TypeORM: the body between
`queryRunner.startTransaction()` and its matching `commitTransaction()` /
`rollbackTransaction()`, or the callback passed to
`dataSource.transaction(...)` / `manager.transaction(...)` /
`connection.transaction(...)`. One block is one transaction function,
regardless of how many ORM calls happen inside it. Its parameters are the
enclosing function's own declared parameters — the IR's "function of its
arguments" (`docs/concept.md` §2.1) refers to these.

**(b) Failing (a), each one-shot statement.** A single top-level statement,
in request-handling or service-layer code (Django views/forms/serializers;
TypeORM controllers/services/repositories), that evaluates exactly one
database-executing ORM call and is not nested inside an (a)-block: in
Django, a terminal `QuerySet`-evaluating method (`.get`, `.create`,
`.update`, `.delete`, `.save`, `.get_or_create`, `.bulk_create`, the final
link of a lazily chained `.filter()/.exclude()/.annotate()` sequence, or a
raw `.raw()` / `cursor.execute()` call); in TypeORM, a `repository.<method>`
or `manager.<method>` call, or a `QueryBuilder` chain ending in
`.getOne()` / `.getMany()` / `.execute()`. Each such call site is its own
transaction function — this is Django's and TypeORM's shared default
behavior (autocommit outside an explicit transaction, one statement per
implicit transaction), and matches the "one-shot transaction" the Abadi et
al. study itself defines (paper §2: "When operations are not combined
using such APIs, a transaction is sent to the database system consisting of
a single statement"). Its parameters are the variables feeding that one
call: its enclosing function's own arguments, plus any local value already
read earlier in the same enclosing function.

**Exclusions.** Database migrations, management/CLI commands, test code,
and seed/fixture generators are not enumerated — they are not part of the
compiled application's own transaction surface (`docs/concept.md` §1's
"complete set of queries, transaction functions" means what the running
application exposes, not its build or test tooling). A helper function
invoked only from inside an already-counted (a)-block or (b)-call is not
separately enumerated at that call site — only the outermost unit (a) or
(b) identifies is counted, to avoid counting one transaction's footprint
twice; the same helper, if also called from a second, independent location
outside any (a)-block, is enumerated separately at that second site. A
function or statement the front end cannot map to the IR's node kinds at
all (arbitrary external I/O, unbounded recursion, a construct it has no
translation for) is refused whole, per `docs/concept.md` §2.1's
unsupported-source rule — refused source is not enumerated as a transaction
function at all, and is tracked separately from "Unknown" footprint
coverage (`docs/experiments.md`, "Footprint categories").

**Two concrete examples per language**, each read directly from a pinned
repository at its frozen commit (§2), confirming the rule against real code
rather than a hypothetical:

- **Django, rule (a).** `zulip/zulip` @ `88e91fa` (full SHA in the
  manifest), `zerver/actions/create_user.py`:
  ```python
  @transaction.atomic(savepoint=False)
  def do_reactivate_user(user_profile: UserProfile, *, acting_user: Optional[UserProfile]) -> None:
      ...
      change_user_is_active(user_profile, True)
      RealmAuditLog.objects.create(...)
      ...
  ```
  The whole decorated function body is one transaction function,
  parameterized by `user_profile` and `acting_user`.
- **Django, rule (b).** `babybuddy/babybuddy` @ `ba46c49`, `core/views.py`,
  `TimerAddQuick.post`:
  ```python
  def post(self, request, *args, **kwargs):
      instance = models.Timer.objects.create(user=request.user)
      ...
      instance.save()
  ```
  No `transaction.atomic` anywhere in this method. `objects.create(...)`
  and the later `instance.save()` are two separate one-shot transaction
  functions, not one — Django autocommits each statement independently
  here, exactly the case rule (b) names.
- **TypeORM, rule (a).** `l3tnun/EPGStation` @ `c0e201b`,
  `src/model/db/RecordedDB.ts`, `restore(items)`:
  ```typescript
  const queryRunner = connection.createQueryRunner();
  await queryRunner.startTransaction();
  try {
      await queryRunner.manager.delete(Thumbnail, {});
      await queryRunner.manager.delete(VideoFile, {});
      await queryRunner.manager.delete(Recorded, {});
      for (const item of items) {
          await queryRunner.manager.insert(Recorded, item);
      }
      await queryRunner.commitTransaction();
  } catch (err) { ... await queryRunner.rollbackTransaction(); }
  ```
  Everything between `startTransaction()` and `commitTransaction()` /
  `rollbackTransaction()` is one transaction function, parameterized by
  `items`, regardless of its variable-length insert loop (bounded by
  `items.length`, satisfying `docs/concept.md` §2.1's bounded-iteration
  node kind).
- **TypeORM, rule (b).** Same file, `insertOnce(recorded)`:
  ```typescript
  public async insertOnce(recorded: Recorded): Promise<apid.RecordedId> {
      const queryBuilder = connection.createQueryBuilder().insert().into(Recorded).values(recorded);
      const insertedResult = await this.promieRetry.run(() => queryBuilder.execute());
      return insertedResult.identifiers[0].id;
  }
  ```
  One `.execute()` call, no explicit transaction wrapper — one one-shot
  transaction function, parameterized by `recorded`.

## 4. Trace source and weights for the applications

**No trace source exists for any of the 110 frozen applications, decided
now rather than discovered mid-extraction. Every application falls back to
the non-gating unweighted metric.**

Reason: the corpus is 110 pinned, static, point-in-time snapshots of
open-source repositories (§2) — there is no request log, APM trace, or
production traffic capture available for any of them, and the Abadi et al.
study itself (§4, confirmed by `docs/research/systematic-review.md` §4.1)
measured only static code structure, never call frequency. Synthesizing a
substitute frequency signal now — for example, weighting by how often each
endpoint's test suite exercises it, or by a hand-guessed usage profile —
would be exactly the kind of after-the-fact tuning the freeze protocol
exists to forbid, since it could not be fixed before classification without
first reading the application's code.

`docs/experiments.md`, "Gating numerator and verdict," already names the
honest fallback for exactly this situation: "An application with no trace
source, and the unweighted fallback metric (one per transaction function)
where no trace exists, are likewise reported but non-gating." E1 decides,
now, that this fallback applies to **every one of the 110 applications**:
each enumerated transaction function (§3) counts once, with no frequency
weight, when computing that application's reported (non-gating) coverage
share.

**Consequence for the gate, stated honestly rather than left implicit.**
`docs/experiments.md`'s Pass condition requires "the median coverage over
at least five traced applications is at least 70%." Under this freeze, zero
of the 110 applications are traced — the median is computed over an empty
set. E1's application-side Pass/Fail condition is therefore structurally
unreachable from this corpus alone as written: TPC-C's own weighted
coverage (§1) can still pass, fail, or land inconclusive on its own, but
the application corpus can only ever contribute the non-gating unweighted
figures `docs/experiments.md` already provides for exactly this case. This
is a consequence of freezing the inputs honestly, not a gap this document
papers over; resolving it — by instrumenting a trace, or by the operator
ruling that the unweighted figure stands in for the gate — is a decision
for #2's review or the operator, not one this freeze invents a trace to
avoid.
