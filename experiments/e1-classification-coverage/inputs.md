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
(fetched 2026-10-15). Revision 5.11's own Document History (spec p. 3) is
not editorial-only: "11 February 2010, Revision 5.11, Updated TPC
Membership, Editorial change in Clause 1.3.1, Modified Clause 6.6.3.7,
Modified Clause 7.2.3.1, Modified/Added Clauses 0.1, 5.7.1, 8.1.1.2, and
9.2.9 to support TPC-Energy requirements." None of those touch Clause 5.2.3
("Mix of Transactions," the clause E1's weights below are frozen against) —
the same Document History table shows 5.2.3 was last modified in Revision
5.10 (17 April 2008) and left untouched by every revision since, including
5.11. 5.11 is nonetheless the current published revision, so it remains the
pinned version; no later revision, technical or editorial, exists to pin
instead.

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

**The rule is kept purely syntactic and reproducible over the source text.**
What a unit's footprint actually turns out to be — static, one-hop, or
unknown — is the classifier's own later job, checked against the source by
the lowering audit (`docs/experiments.md`, "Audits"); this rule only has to
say, unambiguously, what counts as a unit, never what that unit's footprint
is. It is stated as three rules, S1-S3, applied in that order.

**S1 — unit sites.** A unit site is a call expression in the application's
own source whose final method name is in the frozen list below (Django or
TypeORM), matched by name alone — regardless of the receiver's static or
runtime type, and regardless of a repository-local override of that name: a
local `save()` override does not change that `instance.save()` is a site,
exactly as if the frozen library method had been called directly, because
the rule never inspects what the named method's body does. An attribute
reference that is never called is not a site — for example, a bound method
passed as a callback, such as `Child.objects.count` passed unparenthesized
as an argument to `cache.get_or_set`: only the call expression itself,
parentheses included, is enumerated. A lazily evaluated constructor named in
the frozen Django list (`iterator()`, `raw()`) counts at its own call
expression, as a stated convention, consistent with how Django's own docs
describe each: `iterator()` "Evaluates the `QuerySet` (by performing the
query)" at the call
(<https://docs.djangoproject.com/en/stable/ref/models/querysets/#iterator>,
fetched 2026-10-15), and `raw()` "Takes a raw SQL query, executes it..."
also at the call (same page, quoted in full below). This freeze does not
need to adjudicate, for every Django version or database backend, exactly
when the underlying cursor opens or the first row is fetched; the call
expression is the one counted site regardless, and the later consumption of
whatever either call returns (iterating the generator or the `RawQuerySet`)
is never a second site.

**S2 — explicit transactions.** An explicit transaction is one unit,
regardless of how many S1 sites it absorbs. In Django: the body of a
function or method decorated with `@transaction.atomic(...)`, or the body of
a `with transaction.atomic(...):` block. In TypeORM: the body between
`queryRunner.startTransaction()` and its matching `commitTransaction()` /
`rollbackTransaction()`, or the callback passed to
`dataSource.transaction(...)` / `manager.transaction(...)` /
`connection.transaction(...)`. "Absorbs" means lexically: every S1 site
written in the block's own source text is part of that one unit. Helpers are
not expanded — depth 0, removed from this freeze's earlier depth-2 rule: a
transaction block never reaches into a helper function it calls to pull in
that helper's own sites. An S1 site that is textually inside a helper
function's own body, rather than inside the transaction's own lexical body,
is its own separate unit, attributed to the helper's definition — unless the
helper's own definition is itself lexically nested inside an explicit
transaction block (for example, a closure defined inside the `atomic`
block), in which case the block it is textually written inside absorbs it,
by the same lexical rule. A unit's parameters are the enclosing function's
own declared parameters — the IR's "function of its arguments"
(`docs/concept.md` §2.1) refers to these.

**S3 — autocommit sites.** Each S1 site that is not lexically inside an
explicit transaction (S2) is its own one-shot unit. This is Django's and
TypeORM's shared default behavior outside an explicit transaction —
autocommit, one statement per implicit transaction — and matches the
"one-shot transaction" the Abadi et al. study itself defines (paper §2:
"When operations are not combined using such APIs, a transaction is sent
to the database system consisting of a single statement"). A unit's
parameters are the variables feeding that one call: its enclosing function's
own arguments, plus any local value already read earlier in the same
enclosing function. A statement or expression containing several S1
sites — for example, two independent query-builder calls combined in one
`return` — yields that many units, one per site, never one unit per
statement. Conditional and nested sites (inside an `if`/`elif`, a ternary, a
comprehension, a loop body) count once per site, exactly as written in the
source, independent of whether, or how many times, that branch runs at
runtime: enumeration is structural, over the source text, never a runtime
trace.

**Frozen Django evaluation-site list** (Django's current ("stable") docs,
fetched 2026-10-15; the version selector on each page below resolves
"stable" to **Django 6.1**):

- Every `QuerySet`-evaluation trigger named by "When QuerySets are
  evaluated"
  (<https://docs.djangoproject.com/en/stable/ref/models/querysets/#when-querysets-are-evaluated>):
  iteration (`for`, including `async for`), slicing with a "step"
  parameter, pickling, `repr()`, `len()`, `list()`, and a boolean-context
  test (`bool()`, or an implicit truth test in `if`/`and`/`or`).
- Every method the same page's "Methods that do not return `QuerySet`s"
  section documents as evaluating immediately
  (<https://docs.djangoproject.com/en/stable/ref/models/querysets/>,
  section-level statement: "These methods do not use a cache... Rather,
  they query the database each time they're called"): `get()`, `create()`,
  `get_or_create()`, `update_or_create()`, `bulk_create()`, `bulk_update()`,
  `count()`, `in_bulk()`, `iterator()`, `latest()`, `earliest()`, `first()`,
  `last()`, `aggregate()`, `exists()`, `contains()`, `update()`, `delete()`
  (the `QuerySet` method), and `explain()` — excluding `as_manager()`, which
  that same section lists but which constructs a `Manager` class and
  touches no database.
- `raw()` / `Model.objects.raw()` and `cursor.execute()`: `raw()` is
  documented as always executing — "Takes a raw SQL query, executes it, and
  returns a `django.db.models.query.RawQuerySet` instance," and "`raw()`
  always triggers a new query"
  (<https://docs.djangoproject.com/en/stable/ref/models/querysets/>).
- `Model.save()` and `Model.delete()`
  (<https://docs.djangoproject.com/en/stable/ref/models/instances/>):
  "Django executes an `UPDATE`" when the instance's primary key is already
  set (an `INSERT` otherwise) for `save()`; "Issues an SQL `DELETE` for the
  object" for `delete()`.

**Frozen TypeORM evaluation-site list** (typeorm.io docs, fetched
2026-10-15):

- Repository/EntityManager find methods
  (<https://typeorm.io/docs/working-with-entity-manager/repository-api>):
  `find`, `findBy`, `findAndCount`, `findAndCountBy`, `findOne`, `findOneBy`,
  `findOneOrFail`, `findOneByOrFail`, `preload`.
- Count, aggregate, and existence: `count`, `countBy`, `sum`, `average`,
  `minimum`, `maximum`, `exists`, `existsBy`.
- Write and delete: `save`, `remove`, `insert`, `update`, `updateAll`,
  `upsert`, `increment`, `decrement`, `delete`, `deleteAll`, `softDelete`,
  `restore`, `softRemove`, `recover`, `clear`.
- Raw SQL: `query`.
- `QueryBuilder` execution methods
  (<https://typeorm.io/docs/query-builder/select-query-builder>): `getOne`
  ("To get a single result from the database... you must use `getOne`"),
  `getOneOrFail`, `getMany` ("To get multiple results from the database...
  use `getMany`"), `getManyAndCount` (not documented on that page; confirmed
  directly in source — `git clone` of `typeorm/typeorm`, HEAD
  `fe92394287516feeada6e9c308ebe22deca2458e`, fetched 2026-10-15,
  `packages/typeorm/src/query-builder/SelectQueryBuilder.ts:1862`:
  `async getManyAndCount(): Promise<[Entity[], number]>`), `getRawOne` and
  `getRawMany` ("To get raw data, you use `getRawOne` and `getRawMany`"),
  `getCount` ("You can get the count on the number of rows a query will
  return by using `getCount()`"), `stream`, and `execute()` on the
  insert/update/delete query builders.

**An ORM persistence method that wraps several SQL statements in its own
transaction is one unit — an ORM operation, not the SQL statements inside
it.** TypeORM's own docs state that `save()` "saves all given entities in a
single transaction" and that, by default, "all queries in [a] persistence
operation are wrapped into the transaction"
(<https://typeorm.io/docs/working-with-entity-manager/repository-api>). One
call to `save()`, `remove()`, or another persistence method TypeORM
documents this way is one evaluation site under this freeze, regardless of
how many `INSERT`/`UPDATE`/`DELETE` statements it issues underneath —
counting its internal SQL statements would conflate the ORM-level call
graph this enumeration measures with a statement-level count this freeze
does not measure.

**A refused function stays in the denominator.** A function or statement
the front end cannot map to the IR's node kinds at all (arbitrary external
I/O, unbounded recursion, a construct it has no translation for) is refused
for *translation*, per `docs/concept.md` §2.1's unsupported-source rule —
it is never compiled into the running program. Refusal is independent of
*enumeration*: S1-S3 above identify the unit from the source's own
transaction/call structure alone, before translation is attempted, so a
refused unit is enumerated like any other and stays in the denominator. It
classifies **Unknown** (`docs/experiments.md`, "Footprint categories": "an
unsupported function is never dropped from the denominator; it is folded
into this category's numerator"), and the front end's declared refusal
reason (`docs/concept.md` §2.1: it "refuses the whole transaction function,
with a declared reason") is reported alongside that classification,
separately from the aggregate coverage percentage — so a classifier cannot
raise its reported coverage by silently failing to extract what it would
have trouble with.

**Exclusions.** Database migrations, management/CLI commands, test code,
and seed/fixture generators are not enumerated — they are not part of the
compiled application's own transaction surface (`docs/concept.md` §1's
"complete set of queries, transaction functions" means what the running
application exposes, not its build or test tooling). Enumeration counts
each S1 site once, where it is written, never once per caller of whatever
function it sits in: a call expression appearing literally once in the
source is one site, whether the function around it is called from one
place or from ten. Under S2, a site inside an explicit transaction's own
lexical body is absorbed into that one transaction unit — reached a second
time from a second, independent transaction block only if the same call
expression is itself written a second time in that second block's own
body, never by re-walking into a helper the block merely calls (depth 0).
Under S3, a site that sits inside a helper function's own body, and that
helper is not itself lexically inside a transaction, is its own unit at the
helper's definition, counted once there regardless of how many independent,
non-transactional call sites invoke that helper — multiple callers never
multiply a site that is written once.

**Two concrete examples per language**, each read directly from a pinned
repository at its frozen commit (§2), confirming the rule against real code
rather than a hypothetical:

- **Django, S2.** `zulip/zulip` @ `88e91fa` (full SHA in the
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
- **Django, S3.** `babybuddy/babybuddy` @ `ba46c49`, `core/views.py`,
  `TimerAddQuick.post` — no `transaction.atomic` anywhere in this method,
  so S3 applies to every site in it; worked in full, with its resulting
  unit count, in §3.1 below.
- **TypeORM, S2.** `l3tnun/EPGStation` @ `c0e201b`,
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
- **TypeORM, S3.** Same file, `insertOnce(recorded)`:
  ```typescript
  public async insertOnce(recorded: Recorded): Promise<apid.RecordedId> {
      const queryBuilder = connection.createQueryBuilder().insert().into(Recorded).values(recorded);
      const insertedResult = await this.promieRetry.run(() => queryBuilder.execute());
      return insertedResult.identifiers[0].id;
  }
  ```
  One `.execute()` call, no explicit transaction wrapper — one one-shot
  transaction function, parameterized by `recorded`.

### 3.1 Worked examples: full unit counts against the frozen rule

Both examples below are read directly from the pinned commit; neither
invents a line, a call, or a count not present in that source.

**Django — `babybuddy/babybuddy` @ `ba46c49bea0694fd6b9bda728bcd281ef15f284c`,
`core/views.py`, `TimerAddQuick.post` (lines 456–466; confirmed with
`git show ba46c49bea0694fd6b9bda728bcd281ef15f284c:core/views.py`,
2026-10-15):**

```python
def post(self, request, *args, **kwargs):
    instance = models.Timer.objects.create(user=request.user)
    # Find child from child pk in POST
    child_id = request.POST.get("child", False)
    child = models.Child.objects.get(pk=child_id) if child_id else None
    if child:
        instance.child = child
    # Add child relationship if there is only Child instance.
    elif models.Child.count() == 1:
        instance.child = models.Child.objects.first()
    instance.save()
```

No `transaction.atomic` anywhere in this method, so S3 applies to every S1
site in it: each is its own unit, counted once per site regardless of its
conditional. **Five units** enumerate directly inside `post()`, by line:

1. `models.Timer.objects.create(user=request.user)` (line 457) — `create()`
   is in the frozen list's "do not return `QuerySet`s" group. One unit.
2. `models.Child.objects.get(pk=child_id)` (line 460) — the ternary's
   `.get()` call; conditional, counted once. One unit.
3. `models.Child.count()` (line 464, the `elif` condition) — `count` is
   itself in the frozen Django list. Under S1 this call is a site by name
   match alone, regardless of the fact that `Child.count` is a
   repository-local classmethod and not the manager's own `count()`: a
   local override of a frozen name does not change that the call is a
   site, exactly as S1 states for `save()`. One unit, conditional, counted
   once — not a traversal into the classmethod's body.
4. `models.Child.objects.first()` (line 465) — inside the `elif` branch;
   conditional, counted once. One unit.
5. `instance.save()` (line 466) — `Model.save()`, which here issues an
   `UPDATE` (the instance's primary key was already set by the preceding
   `create()`). One unit.

The classmethod `Child.count` itself (`core/models.py:218–221`, confirmed
with `git show ba46c49bea0694fd6b9bda728bcd281ef15f284c:core/models.py`,
2026-10-15) contributes **zero** further units:

```python
@classmethod
def count(cls):
    """Get a (cached) count of total number of Child instances."""
    return cache.get_or_set(cls.cache_key_count, Child.objects.count, None)
```

`Child.objects.count` here is passed unparenthesized as the second argument
to `cache.get_or_set` — an attribute reference used as a callback, never
itself called in this source — so under S1's callback exception it is not a
site, and `cache.get_or_set` is not a name in either frozen list. Nothing
inside `Child.count`'s own body is a unit.

**`Child.count()` has three callers in this pinned commit, not two** — all
three are one-shot S1 sites outside any explicit transaction, so S3
attributes a unit to each of them independently, at its own line, never
collapsed onto the helper's definition (there is no helper-side unit to
collapse onto, since `Child.count`'s body has none):

- `TimerAddQuick.post`, line 464 (counted above, unit 3).
- `Timer.title_with_child` (`core/models.py:632`): `Child.count() > 1`.
- `set_initial_values` (`core/forms.py:41`, confirmed with
  `git show ba46c49bea0694fd6b9bda728bcd281ef15f284c:core/forms.py`,
  2026-10-15): `elif models.Child.count() == 1:` — the third caller, absent
  from the previous draft's "two independent...locations" count.

**Total for `TimerAddQuick.post`: 5 units**, all five enumerated directly
inside `post()` at lines 457, 460, 464, 465, and 466 — none attributed to
the helper, because S1's callback exception leaves the helper's own body
with no site to attribute. The three independent callers of `Child.count()`
across the pinned commit are three separate S3 units at three separate
lines, not one shared unit multiplied or collapsed by caller count.

**TypeORM — `cool-team-official/cool-admin-midway` @
`edc13838439a0d6469abfe6263f51f2c987284bf`,
`src/modules/cloud/service/db.ts`, `CloudDBService.data` (confirmed with
`git show edc13838439a0d6469abfe6263f51f2c987284bf:src/modules/cloud/service/db.ts`,
2026-10-15):**

```typescript
async data(id, method, params: any = {}) {
  const db = await this.cloudDBEntity.findOneBy({ id });
  const repository = await this.getRepository(db?.className);
  if (method == 'add' || method == 'update') {
    await repository.save(params);
    return { id: params.id };
  }
  if (method == 'delete') {
    await repository.delete(params.ids);
  }
  if (method == 'clear') {
    await repository.clear();
  }
  if (method == 'page') {
    const { page = 1, size = this.coolConfig.crud.pageSize } = params;
    const find = repository
      .createQueryBuilder()
      .offset((page - 1) * size)
      .limit(size)
      .orderBy('createTime', 'DESC');
    return {
      list: await find.getMany(),
      pagination: {
        page: parseInt(page),
        size: parseInt(size),
        total: await find.getCount(),
      },
    };
  }
}
```

No explicit transaction anywhere in `data()`, so S3 applies to every S1
site found through the TypeORM frozen list; `createQueryBuilder()` itself
is not in that list — it only builds the query and executes nothing, so it
is not a site. Six units enumerate directly inside `data()`, each counted
once per conditional site:

1. `this.cloudDBEntity.findOneBy({ id })` (line 33).
2. `repository.save(params)` (line 36, the `add`/`update` branch) — one
   ORM-operation unit under the persistence-method rule above, regardless
   of how many `INSERT`/`UPDATE` statements it issues underneath.
3. `repository.delete(params.ids)` (line 42, the `delete` branch).
4. `repository.clear()` (line 45, the `clear` branch).
5. `find.getMany()` (line 55, the `page` branch — part of the review's
   cited span, lines 54–61).
6. `find.getCount()` (line 59, the `page` branch — the other half of the
   cited span): a second, independent execution against the same
   conditions, confirming the frozen rule directly — a statement with
   several executing calls (here, one `return` holding both `getMany()` and
   `getCount()`) yields several units, not one.

`this.getRepository(db?.className)` (line 34) is not itself a seventh S1
site: `getRepository` is not a name in the frozen TypeORM list, so the call
expression at line 34 does not match S1 regardless of what the method it
names does. Under S2/S3, `getRepository` is a helper, not lexically inside
any transaction, so S1 sites written inside its own body are each their own
unit at the helper's definition (lines 69–77 of the same file), by depth 0
— never by walking from `data()` into it through the line-34 call. That
helper's own body has exactly one such site: `this.cloudDBEntity.findOneBy({
className })` (line 70), which matches `findOneBy` in the frozen list. It
is written once in the source and is called from this one location in the
pinned commit, so it is one unit, attributed to the helper's definition —
plus one unit.

**Total: 7 units** — 6 enumerated directly inside `data()`, plus 1 at the
helper `getRepository`'s own definition (line 70). The review's specifically
cited lines 54–61 alone contribute 2 of those 7 (`getMany()` + `getCount()`)
— resolving the ambiguity the review raised: the frozen rule counts both
calls, never one call per statement.

## 4. Trace source and weights for the applications

**No trace source exists for any of the 110 frozen applications, decided
now rather than discovered mid-extraction. Every application's gating share
is therefore its unweighted coverage, not a frequency-weighted one.**

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

`docs/experiments.md`, "Gating numerator and verdict," names the honest
fallback for exactly this situation: one unit per enumerated transaction
function (§3), no frequency weight, labelled "unweighted — no trace source
exists." E1 decides, now, that this fallback applies to **every one of the
110 applications** and, because none of the 110 has a trace source, *is*
the application-side gating metric — not a non-gating side figure computed
alongside a trace-weighted one that never materializes.

**Consequence for the gate, decided now rather than left implicit.**
`docs/experiments.md`'s original Pass condition — "the median coverage over
at least five traced applications is at least 70%" — could never be
satisfied
under this freeze: zero of the 110 applications are traced, so that median
would be computed over an empty set on every run this corpus produces, not
merely an unlucky result for this particular corpus. Rather than freeze a
gate that fails by construction and resolve it only after a result exists,
this freeze decides now: the application half of the gate uses the
unweighted fallback above as its *gating* metric — the median, over the 110
frozen applications, of each application's own share of enumerated
transaction functions (§3) that classify static or one-hop, labelled
"unweighted — no trace source exists" — against the same 70%/50%
thresholds. TPC-C's own weighted coverage (§1) is unaffected and keeps
gating on its frozen mix. `docs/experiments.md`, "Gating numerator and
verdict," is amended in the same change to state this directly, replacing
the "at least five traced applications" condition it previously carried;
the two documents agree from this point on.
