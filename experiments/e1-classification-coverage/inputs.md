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

**The rule is reproducible over the source text.** What a unit's footprint
actually turns out to be — static, one-hop, or unknown — is the classifier's
own later job, checked against the source by the lowering audit
(`docs/experiments.md`, "Audits"); this rule only has to say, unambiguously,
what counts as a unit, never what that unit's footprint is. It is stated as
five rules, S1-S5, applied in that order. S1-S3 decide what counts as a unit,
over the source text; S4 decides which of S1's reported candidates are not
database operations at all — a classification question answered later by
the lowering, never by this enumeration rule; S5 fixes a structural gap in
the TypeORM analyzer's own candidate set. S1 (below) is mediated through a
pinned external analyzer rather than hand-matched against a list this
document maintains — S2, S3, and S5 stay directly syntactic over the source
text, unchanged by that; S4 is resolved by the classifier's lowering, not by
this document.

**S1 — unit sites.** A unit site is an ORM operation reported by the
111-application study's own published analyzers (`splinter-mypy` for Django,
`@ctring/splinter-eslint` for TypeORM — both named below with their pinned
versions and exact commands), run unmodified against the application's
source at its frozen commit (§2). A site is exactly one of that analyzer's
own `method`-typed output messages, located at the call expression it
reports; no site is added, removed, or reattributed by hand. This replaces
this freeze's earlier independently-authored name list, retired after the
first review found it conflated dictionary and builtin calls with ORM calls
(`request.POST.get(...)` was being counted as if it were `QuerySet.get()`):
using the study's own tool in place of a list re-derived from documentation
removes that class of disagreement by construction, because there is no
longer a second, hand-maintained list to disagree with the first. Neither
analyzer emits a distinct "operation seen but unrecognized" message for a
call that matches no name in its lists — an unmatched call is simply absent
from its output, not present with an "unknown" tag — so a call outside both
frozen lists below is not a site at all, exactly as under the previous rule;
what the classifier cannot resolve about a site that *is* reported still
falls to the existing "Unknown" footprint category
(`docs/experiments.md`, "Footprint categories"), unaffected by this change.
An attribute reference that is never called is still not a site, for the
same reason as before — neither analyzer inspects a non-call expression —
for example, `Child.objects.count` passed unparenthesized as an argument to
`cache.get_or_set` produces no message under either analyzer, verified
directly for this exact Django case in §3.1 below.

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
by the same lexical rule. When one explicit transaction block is itself
lexically nested inside another — Django's `@transaction.atomic` opening a
savepoint inside an outer `atomic` block, or a nested
`dataSource.transaction(...)` callback — the **outermost** explicit
transaction owns every S1 site in its lexical body, including every site
textually inside the nested transaction; the nested transaction contributes
no unit of its own. This is the same lexical-absorption rule applied one
level up, stated explicitly because otherwise two transaction blocks could
each claim the same site, breaking "each site counted once." A unit's
parameters are the enclosing function's own declared parameters — the IR's
"function of its arguments" (`docs/concept.md` §2.1) refers to these.

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

**S4 — non-database candidates.** The analyzers (S1) define the candidate
set, not the final denominator. S4 requires affirmative evidence: a
candidate S1 reports leaves the denominator only when the lowering shows
*positively* that it performs no database access — its receiver resolves to
a concrete non-ORM type, and that type's called method has no path to an
ORM operation, for example Django's `QueryDict.get` resolved via the
framework's own documented type for the `HttpRequest` view argument
(`request.GET`/`request.POST` are `QueryDict`, and `QueryDict.get` is a
dict-like lookup TypeORM's and Django's own ORMs never implement). A
candidate whose receiver mypy cannot resolve to a concrete type — reported
as `Any` — or whose called method could reach the ORM, stays in the
denominator; an unresolvable one classifies **Unknown**, the same as any
other candidate the classifier cannot determine. An unannotated Django
`request` parameter that mypy infers as `Any` is exactly the unresolved
case, not the affirmative one: `Any` is the absence of a resolved type, not
evidence that the resolved type is non-ORM, so a candidate reported this way
does not, by itself, qualify for S4 exclusion — see the worked
`TimerAddQuick.post` example in §3.1 for what evidence would be needed to
decide sites 2 and 7 either way. This is a classification outcome decided by
the lowering (`docs/concept.md` §2.1), never a second hand-maintained name
list applied on top of S1's output. Each application's report states the
count of S4 exclusions next to its coverage percentage, together with the
affirmative evidence recorded for each one (the excluded site, its resolved
receiver type, and the called method's documented absence of an ORM path),
so a dropped non-database candidate never silently shrinks the denominator
unreported or unverifiable.

**S4 audit sample.** The lowering audit's sample (`docs/experiments.md`,
"Audits") is drawn, per application, by a frozen rule: a seeded uniform
random sample, without replacement, of `max(20, 10%)` of that application's
included units and, separately, `max(20, 10%)` of that application's
excluded S4 candidates — or every excluded candidate, if the application
has fewer than 20. The generator is Python's `random.Random(seed)`, seed
`17` (this item's own number), calling `.sample(population, k)` once per
application per population (included units, excluded candidates), with
`population` ordered by each site's `(file_path, line, column)` as reported
by the analyzer, so the sample is reproducible from the analyzer's own
output alone. This proves each exclusion against the source, rather than
merely asserting it, and the included-unit sample proves the included side
of the same audit at the same time.

**S5 — TypeORM QueryBuilder chains.** A TypeORM `QueryBuilder` chain —
`repository.createQueryBuilder()` followed by any sequence of builder
methods and ending in a terminal execution call (`getOne`, `getMany`,
`getCount`, `getRawOne`, `getRawMany`, `stream`, `execute`, or no terminal
at all) — is one unit, represented by its `createQueryBuilder()` site (S1,
"other"). S5 absorbs **every** analyzer-reported site in that same chain,
not only the terminal: an intermediate builder method that happens to match
one of the analyzer's own four name lists (for example `.insert()`, matched
under "write" the same as a direct `repository.insert(...)` call) is still
part of the one chain unit, never a second, separate S1 site. This closes
two structural gaps at once: no terminal method is in any of the analyzer's
four lists (§3.1's `CloudDBService.data`, the `getMany`/`getCount` pair), so
without S5 a chain's terminal call would be invisible to enumeration
entirely; and an intermediate method the analyzer *does* report would
otherwise double-count the same chain as two units instead of one. Neither
gap adds a second unit — S5 confirms that whatever the analyzer reports
anywhere along the chain, terminal or intermediate, recognized or not,
collapses into the one unit at `createQueryBuilder()`.

**S5's precedence against S2 and S3.** S2 takes precedence over S5
lexically: a chain written inside an explicit transaction's own lexical
body contributes no unit of its own — every site in it, `createQueryBuilder()`
included, is absorbed into that transaction's one S2 unit, the same as any
other S1 site inside it. Outside an explicit transaction, S5 takes
precedence over S3: a chain is one S5 unit, not one S3 unit per site it
contains — S3's "each S1 site is its own one-shot unit" default applies
only to a site that is not part of any S5 chain.

**Django analyzer: `splinter-mypy`.**
`github.com/umd-dslam/splinter-mypy` @
`f8057cae6c2faae19e7cf97f3026a4fbebd4f9cc` (HEAD as of 2026-10-15; the
repository carries no tagged release, so HEAD is the pinned version). It
runs mypy's own build — a real type-check, not a lint pass — over the
target source with `django-stubs` installed as plain type stubs (the
Django mypy plugin that rewrites field types from `models.py` is *not*
registered) and walks the resulting typed AST: for every call expression
`expr.method(...)`, it looks up mypy's statically inferred type of `expr`,
skips the call if that type's string representation starts with
`builtins`, `collections`, `os`, or `hashlib`, and otherwise reports it if
`method` is in one of three fixed Python lists in its own source
(`splinter/analyzer.py:41-77`, pinned commit):

- read: `filter, all, get, exclude, remove, add, aggregate, first, last, count, distinct, values, values_list, latest, earliest, dates, datetimes, exists, extra`
- write: `save, delete, create, update, update_or_create, get_or_create, bulk_create, bulk_update`
- other: `raw, execute`

A decorator resolved by its fully-qualified import to
`django.db.transaction.atomic`, or a call resolved the same way, is reported
as a transaction marker (`splinter/analyzer.py:287-394`); the receiver's
resolved type(s) are recorded alongside each report but never gate inclusion
beyond the four-prefix exclusion above. In particular: `iterator()` and
`list()`/`len()`/`bool()`/`repr()` are not in any list and are never
reported (`iterator()` is absent from all three lists above, contrary to
this freeze's previous, independently-authored list); a Django request or
view parameter is normally unannotated, so mypy infers it as `Any`, which
the four-prefix exclusion does not catch — `request.POST.get(...)` and
`request.GET.get(...)` are therefore, in practice, reported as sites,
verified directly in §3.1 below.

Pinned environment and command (installed only under this task's
`scratchpad/e1-tools/`, nothing system-wide): a Python 3.11 virtual
environment (`uv venv`), `pip install -r requirements.txt` from the pinned
commit unedited (exact versions: `mypy==1.9.0`, `django==5.0.3`,
`django-stubs==4.2.7`, `django-stubs-ext==4.2.7`,
`django-filter-stubs==0.1.3`, `djangorestframework-stubs==3.14.5`, plus
their own pinned transitive stub packages), then
`pip install --no-deps -e <checkout of f8057cae6c2faae19e7cf97f3026a4fbebd4f9cc>`.

**Django file-selection rule, frozen identically for every one of the 78
Django applications, no per-application choice left:** every tracked
`*.py` file, excluding any path with a directory component named `tests`,
`test`, `migrations`, `node_modules`, `venv`, `.venv`, `docs`, or
`site-packages`, and excluding any file named `test_*.py`, `*_test.py`, or
`conftest.py`. The one command line below realizes that rule, unchanged
across all 78 applications; `<application-root>` is not a discretionary
choice but a mechanical binding — the repository checked out at the
`commit_sha_full` its own row of `corpus-manifest.csv` names, one
invocation per row:

```
python -m splinter <application-root> \
  --exclude '**/migrations/**' \
  --exclude '**/tests/**' \
  --exclude '**/test/**' \
  --exclude '**/node_modules/**' \
  --exclude '**/venv/**' \
  --exclude '**/.venv/**' \
  --exclude '**/docs/**' \
  --exclude '**/site-packages/**' \
  --exclude '**/test_*.py' \
  --exclude '**/*_test.py' \
  --exclude '**/conftest.py' \
  --output messages.json
```

No output filtering beyond S2-S5 is applied to this command's `messages.json`:
enumeration reads every reported message, classified by S2-S5 as above,
with no additional hand-picked inclusion or exclusion of any reported site.

**TypeORM analyzer: `@ctring/splinter-eslint` + `@ctring/eslint-plugin-typeorm`.**
Published to npm only (no public source repository exists for either
package); pinned at the exact resolved versions
`@ctring/splinter-eslint@1.0.9` and its dependency
`@ctring/eslint-plugin-typeorm@1.0.1` (both the `latest` dist-tag as of
2026-10-15, verified against the npm registry metadata and against each
package's own extracted tarball source). It runs ESLint with
`@typescript-eslint/parser`'s full TypeScript type-checker service,
resolved from the target repository's own `tsconfig.json`, and for every
call expression `expr.method(...)` reports it if `method` is in one of
four fixed TypeScript lists in the plugin's own source
(`src/rules/find-api.ts`, pinned version):

- read: `countBy, sum, average, minimum, maximum, findBy, findAndCountBy, findOneBy, findOneByOrFail, count, exist, find, findAndCount, findOne, findOneOrFail, findByIds, findOneByIds`
- write: `clear, create, insert, merge, preload, save, softRemove, recover, remove, upsert, update, delete, increment, decrement, softDelete, restore`
- other: `createQueryBuilder, query`
- transaction: `transaction, startTransaction` (plus a method decorated
  `@Transaction`/`@LazyTransaction`, reported directly as a transaction
  marker)

Unlike the Django analyzer, this plugin performs **no receiver-type
exclusion at all**: it matches by method name alone on a member-access call
of any receiver type, and only *records* the type checker's resolved
receiver type(s) as report metadata, never as a filter. Two consequences
contradict this freeze's previous, independently-authored "Frozen TypeORM
evaluation-site list," which is why that hand list is retired in favor of
the one actually reproduced above: none of the four lists names any
`QueryBuilder` terminal execution method (`getOne`, `getMany`,
`getManyAndCount`, `getRawOne`, `getRawMany`, `getCount`, `stream`) or
`execute()` — the pinned analyzer has no concept of them at all, so none is
ever reported — while `createQueryBuilder` itself *is* named, under
"other," and is reported every time it is called. Both are verified
directly in §3.1 below.

Pinned environment and command (installed only under this task's
`scratchpad/e1-tools/`, nothing system-wide):
`npm install @ctring/splinter-eslint@1.0.9 typescript@5.4.3` in a dedicated
directory. The explicit `typescript@5.4.3` pin is necessary, not cosmetic:
installing `@ctring/splinter-eslint@1.0.9` alone, with no fixed `typescript`
version anywhere in its own dependency tree, let npm resolve the bare
`typescript` dependency to `7.0.2` (TypeScript's native-compiler v7 line),
which `@typescript-eslint/parser@7.3.1` cannot parse — observed firsthand:
`TypeError: Cannot read properties of undefined (reading 'BarBarToken')`;
`5.4.3` matches the range the package's own (unpublished alongside the npm
package, but present in its `devDependencies`) build was written against.
The target application must have its own dependencies installed in place
(an ordinary `npm install` inside its own clone) so the TypeScript checker
resolves real types, rather than `any`, for most calls. The root passed
must contain, or be an ancestor of, the project's own `tsconfig.json`,
because `parserOptions.project` is resolved as `**/tsconfig.json` under
that root: pointing the root directly at a subdirectory without the
project's `tsconfig.json` breaks parser services for files outside that
scope — observed firsthand against `cool-team-official/cool-admin-midway`'s
`src/` directory: `Error: ... requires parserServices ... You must
therefore provide a value for the "parserOptions.project" property`.

**TypeORM file-selection rule, frozen identically for every one of the 32
TypeORM applications, no per-application choice left:** the project of the
`tsconfig.json` nearest the repository root — among every tracked
`tsconfig.json` in the repository, the one with the shortest path from the
root, ties broken by lexical order of that path. When one exists (every
pinned TypeORM application in this corpus has at least one), its directory
*is* the root passed to the analyzer, and no `--include` glob is passed at
all: "the project" means whatever that `tsconfig.json`'s own `include`/
`files`/`exclude` fields select, exactly as `@typescript-eslint/parser`
resolves it to build the type-checker service — not a second, hand-built
glob layered on top of the project's own file list.

```
node <splinter-eslint>/dist/index.js <directory of the repository's
  shortest-path, lexically-first tsconfig.json> --output messages.json
```

If no `tsconfig.json` is tracked anywhere in the repository (not observed
in this corpus, but frozen now rather than decided ad hoc if it occurs):
every tracked `*.ts` file, excluding any path with a directory component
named `node_modules`, `dist`, `build`, `test`, `tests`, or `__tests__`, and
excluding any file named `*.spec.ts` or `*.test.ts`.

```
node <splinter-eslint>/dist/index.js <repository root> \
  --include '**/*.ts' \
  --output messages.json
```

realized by discarding, before enumeration, every message in this
fallback command's `messages.json` whose `filePath` falls under an
excluded directory or matches an excluded filename above — the mechanism
"Exclusions" below already names for a path a glob cannot exclude
cleanly, since `splinter-eslint` exposes only `--include`, never
`--exclude`. No other output filtering is applied: the nearest-`tsconfig.json`
command's `messages.json` is read in full, and the fallback command's is
read in full after only that one path/filename discard — classification
beyond that is S2-S5 alone.

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
*enumeration*: S1-S3 and S5 above identify the unit from the source's own
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

**Exclusions.** Database migrations, test code, vendored or virtualenv
directories, and documentation directories are not enumerated, by the one
frozen file-selection rule above (Django's `--exclude` globs; TypeORM's
nearest-`tsconfig.json` project, or its fallback `--include` glob plus
output discard) — the same rule for every application of that ORM, with no
per-application variant and no further hand-picked inclusion or exclusion.
This is not the whole of "the running application's own transaction
surface" in the abstract (`docs/concept.md` §1) — a management/CLI command
or a seed/fixture generator that does not happen to sit under one of the
named excluded directories is enumerated like any other source file, since
the frozen rule is a fixed directory/filename list, never a per-file
judgment call about what a file is for. Under S1, the exclusion is applied
either at invocation (the glob passed to the analyzer) or, where a path
cannot be excluded by a glob cleanly, by discarding any analyzer output
message whose `filePath` falls under an excluded path before enumeration;
either way it is applied to the analyzer's raw output, never by
hand-picking which of its reported sites to keep — no output filtering
beyond S2-S5 is applied otherwise. Enumeration counts
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
- **TypeORM, S5.** Same file, `insertOnce(recorded)`:
  ```typescript
  public async insertOnce(recorded: Recorded): Promise<apid.RecordedId> {
      const queryBuilder = connection.createQueryBuilder().insert().into(Recorded).values(recorded);
      const insertedResult = await this.promieRetry.run(() => queryBuilder.execute());
      return insertedResult.identifiers[0].id;
  }
  ```
  Two S1 sites by the frozen TypeORM lists above — `createQueryBuilder()`
  ("other") and `.insert()` ("write") — `.into()`, `.values()`, and
  `.execute()` match no name in any of the four lists (this corrects the
  previous draft's claim of one `.execute()` site and no
  `createQueryBuilder()` site, the opposite of what the pinned analyzer's
  own name lists say; this one is read directly off those lists rather than
  re-run, since the task this freeze was revised under named only the other
  two repositories for an actual tool run). `.insert()` is a builder method
  chained directly off `createQueryBuilder()`'s own return value, and the
  chain terminates in `queryBuilder.execute()` — no explicit transaction
  wrapper, so S5 applies, not S3: both reported sites are the same chain,
  and collapse into **one** unit, represented by the `createQueryBuilder()`
  site, parameterized by `recorded`. This is one unit, not two: treating
  `.insert()` as its own one-shot S3 unit, as an earlier draft did, is
  exactly the double-count S5 exists to prevent.

**Clarification 16.10.2026 (Django enumeration).** Three readings of the
text above, ruled so the extractor applies them mechanically. They add no rule; none states or implies a count.

- **S3 parameters.** Reads "the variables feeding that one call: its enclosing
  function's own arguments, plus any local value already read earlier". The
  call is the unit of S3, so only names the site's call expression references
  (its receiver and arguments) count: those that are the enclosing function's
  declared parameters, then those that are locals bound before the site. A
  name bound only by the site's own statement or later, a global, and a name
  the call does not reference are not parameters. Binding every earlier name
  would make an unrelated assignment change a unit's parameters.
- **Application boundary.** Reads "run unmodified against the application's
  source" and "discarding any analyzer output message whose `filePath` falls
  under an excluded path before enumeration". The `--exclude` globs only
  keep the analyzer from starting at a file; it still follows imports into
  files outside the application root and into files the selection excludes.
  A report located outside the application root, or in a file the frozen
  selection excludes, is not a site: applying the one selection rule to the
  analyzer's output is not the "additional" filtering the paragraph after
  the command forbids, since it picks no site by hand.
- **Transaction markers.** Reads S1's "`method`-typed output messages" with
  the three fixed lists (read, write, other) and S2's two Django forms. A
  message of method type `transaction` is not in the three lists, so it is
  not a site. It opens an S2 unit only when it is the decorator of a
  function or an item of a `with` statement; a marker that is neither (for
  example `ExitStack.enter_context(atomic())`) opens no S2 unit, and the
  sites around it stay S3 by S3's default. Each such marker is recorded with
  its location in a side output of the run and never aborts it, so the
  uncounted transaction stays visible rather than silent.

**Clarification 16.10.2026 (TypeORM enumeration).** Readings of the text above
for TypeScript, ruled so the extractor applies them mechanically. They add no
rule; none states or implies a count. Where the Django clarification above
applies unchanged (S3 parameters, application boundary, transaction markers),
it is applied to TypeScript.

- **Nearest `tsconfig.json`.** "The shortest path from the root" is the path
  with the fewest characters; equal lengths are ordered lexically. It is not
  the shallowest path.
- **S3 parameters.** "The enclosing function" is the innermost function, method,
  constructor, accessor or arrow function whose body holds the call. Parameters
  are the names that function declares (destructured names and rest included),
  then its `const`/`let`/`var`, loop and `catch` variables bound before the
  site, as the Django clarification orders them; a name of an outer function is
  not one. For an S5 chain the call is the whole written chain. §3.1's
  "parameterized by `recorded`" for `insertOnce` names only the function
  argument; `connection`, a local bound earlier in the function and read by
  the call, is also a parameter under the S3-parameter reading above, so the
  unit's parameters are `recorded` and `connection`. This changes the
  parameters, not the example's unit count.
- **S5 chain.** A chain is one written receiver chain: the `createQueryBuilder()`
  call and every call whose callee is a member access on the result of the call
  before it, up to the last such call. A reported call on a variable that
  holds a builder, or inside an argument of a chain method, is not in the
  chain. A chain holding several `createQueryBuilder()` calls belongs to the
  first one written.
- **S2 forms.** `X.startTransaction()` is matched by enumerating the control-flow
  paths of its function (the file body is one more function), statements in
  order, and running a stack of open starts per receiver text on each path:
  a start pushes, a `X.commitTransaction()` or `X.rollbackTransaction()` with
  the same receiver text written directly in the same function pops the
  innermost open start (one with nothing open is stray and changes nothing),
  and a `return`, or the end of the function, ends the path. A start is
  matched when every path that reaches it pops it, and its transaction ends
  at the last terminator that pops it on any path; a start that no path
  reaches is unmatched, and so is one that some path leaves open. An
  unmatched start opens no unit. The arms
  of an `if` (an absent `else` is the arm that skips it) and of a ternary are
  separate paths; a `switch` is entered at any of its clauses, which run on
  into the following ones until a `break`, `return` or `throw`, and it can
  also be skipped when it has no `default`.
  `try`, `catch` and `finally` run in sequence: after a body that completes,
  the code goes on either without the catch or through it, and `finally`
  runs after every path that did not throw. A path that ends in `throw` is
  dropped and decides nothing, a stated limitation: judging it needs a model
  of which `catch` or `finally` receives the exception, which this walk does
  not carry. A start made while another is open on the same path nests in it
  and the outermost owns the nested one's sites; starts in mutually exclusive
  arms are on different paths and so are one alternative opening, not a
  nesting. The definition covers exactly these constructs. A function has
  unsupported control flow, and every start in it is an unmatched marker with
  the reason "unsupported control flow", when it has a start, a commit or a
  rollback inside a loop (body, condition, initializer, update, or the
  iterated expression), a `break` or `continue` that names a label, or a
  `return` inside a loop that an enclosing `finally` holding a commit or
  rollback follows; a loop with none of these is walked as a path that may
  skip it. Paths that did the same to every transaction count once, and a
  function with more than 4096 distinct paths is not enumerated: every start
  in it is an unmatched marker with the reason "path cap exceeded". Both
  reasons are recorded in the side output.
  `X.transaction(...)` opens a unit when `X` is, or ends in a property named,
  `dataSource`, `manager` or `connection`, and its body is the last function
  or arrow function argument. A method decorated `@Transaction` or
  `@LazyTransaction`, and any other `transaction` call, is a marker that opens
  no unit, recorded as the Django clarification states. A transaction report
  that is neither a call over the reported span nor such a decorated method
  is not a legitimate unsupported form and fails the run.
- **Dependency installation (recorded deviation).** §3's plain `npm install`
  of an application is run as `npm install --ignore-scripts --no-audit
  --no-fund`. Reason: lifecycle scripts would execute arbitrary code from the
  dependency trees of 32 third-party applications on the machine, and type
  resolution needs package contents, not their build output. Possible bias:
  types that a dependency's install script would generate stay unresolved,
  which can hide sites the analyzer would otherwise report. The reach of the
  deviation is recorded, not assumed: each application's run writes
  `<application>.install-scripts.jsonl` naming the application and every
  installed package that declares a `preinstall`, `install` or `postinstall`
  script (the application also `prepare`), read from the package manifests
  without running anything.
- **Installation inputs.** Before the installation the whole checkout,
  lockfiles included, is verified against the manifest commit (nothing
  modified, deleted, untracked or ignored apart from `node_modules`). After
  `npm install --ignore-scripts` every tracked lockfile must be byte-identical
  to the commit, otherwise the run fails and names it; `npm ci` is not used.
  An application that tracks no lockfile gets the lock the installation
  generated recorded by the installation step, once the checkout is verified,
  with its SHA-256 and the clock at that verification (never a file's
  modification time) in `<application>.generated-locks.jsonl`, as evidence of
  what resolved. The analysis step validates the lockfile it finds against that
  record and fails on a missing record or any mismatch.

### 3.1 Worked examples: full unit counts against the frozen rule

Both examples below were recounted by actually running the two pinned
analyzers above against the pinned commit — not by hand-matching the source
against a list — using the exact pinned versions and commands given above.
Every call below is the analyzer's own report, read back from its output
JSON; the two cases where the analyzer did *not* report a call this
document's earlier draft expected are called out explicitly, with the
reason verified in the analyzer's own source, not asserted.

**Django — `babybuddy/babybuddy` @ `ba46c49bea0694fd6b9bda728bcd281ef15f284c`,
`core/views.py`, `TimerAddQuick.post` (the complete method, lines 456–470;
confirmed with
`git show ba46c49bea0694fd6b9bda728bcd281ef15f284c:core/views.py`,
2026-10-15 — the earlier draft's excerpt stopped at line 466, which is why
it missed two of the method's own sites):**

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
    self.url = request.GET.get(
        "next", reverse("core:timer-detail", args={instance.id})
    )
    return super(TimerAddQuick, self).get(request, *args, **kwargs)
```

No `transaction.atomic` anywhere in this method, so S3 applies to every S1
site in it. Running `python -m splinter` (pinned command above) against
this commit reports exactly **seven** sites inside `post()`, each its own
S3 unit:

1. `models.Timer.objects.create(user=request.user)` (line 457) — `create`,
   write; receiver type `django.db.models.manager.Manager[core.models.Timer]`.
2. `request.POST.get("child", False)` (line 459) — `get`, read; receiver
   type resolved by mypy as `Any` (the `request` parameter carries no type
   annotation), which the analyzer's four-prefix exclusion does not catch
   — so this *is* reported, contrary to the previous draft's claim that a
   hand-written name-only rule alone was the source of this
   miscount: the published, type-aware tool reports it too, for a
   specific, verified reason (an unannotated receiver resolves to `Any`),
   not because it is coarse in the way the first review's finding assumed.
3. `models.Child.objects.get(pk=child_id)` (line 460) — the ternary's
   `.get()` call; `get`, read; receiver type
   `django.db.models.manager.Manager[core.models.Child]`.
4. `models.Child.count()` (line 464, the `elif` condition) — `count`,
   read; receiver type resolves to the constructor signature of `Child`
   itself (`def (*args: Any, **kwargs: Any) -> core.models.Child`), because
   `models.Child` here is the class, not an instance — matched by name
   alone, exactly as S1 predicts for a repository-local override: `Child`
   defines its own `count()` classmethod (below), and the analyzer does not
   know or care that it is not `Manager.count()`.
5. `models.Child.objects.first()` (line 465) — `first`, read.
6. `instance.save()` (line 466) — `save`, write; receiver type
   `core.models.Timer`.
7. `request.GET.get("next", reverse(...))` (lines 467–469) — `get`, read;
   receiver type `Any`, for the same reason as site 2. This is the second
   site the earlier draft's truncated excerpt omitted.

**Not reported: `super(TimerAddQuick, self).get(request, *args, **kwargs)`
(line 470).** This is not a coarse miss of the kind the review's finding
described — it is structural. mypy's own semantic analysis represents a
call through Python's `super()` proxy by attaching the called method's name
directly onto a `SuperExpr` node, rather than wrapping it in the
`MemberExpr` node an ordinary `obj.method()` call produces. `splinter-mypy`'s
dispatch only inspects calls whose callee is a `MemberExpr`
(`splinter/analyzer.py:321`), so a `super()` call's callee never reaches
that branch, regardless of what name follows `.get`. Verified directly:
no message exists anywhere in the analyzer's output at line 470.

The classmethod `Child.count` itself (`core/models.py:218–221`) contributes
**zero** further units, also verified directly in the output:

```python
@classmethod
def count(cls):
    """Get a (cached) count of total number of Child instances."""
    return cache.get_or_set(cls.cache_key_count, Child.objects.count, None)
```

`cache.get_or_set` is not a name in any of the three Django lists, so this
call is not a site; `Child.objects.count` is passed unparenthesized as an
argument — never itself a call expression — so it produces no message
either, confirmed by the absence of any entry for these lines in the
analyzer's output.

**`Child.count()` has three production callers in this pinned commit,
all three present in the analyzer's output as three independent S3
units**, never collapsed onto the helper's definition (which has no site of
its own to collapse onto):

- `TimerAddQuick.post`, line 464 (counted above, site 4).
- `Timer.title_with_child` (`core/models.py:632`): `Child.count() > 1`.
- `set_initial_values` (`core/forms.py:41`): `elif models.Child.count() == 1:`.

(`Child.count()` also appears four times in `core/tests/tests_models.py`,
also present in the raw output — excluded from the unit count under this
document's "Exclusions," since test code is not enumerated.)

**Total for `TimerAddQuick.post`: 7 units**, all seven enumerated directly
inside `post()`, by the analyzer's own report — two more than this
document's previous draft claimed (which missed the two `request.GET`/
`request.POST` sites) and one fewer than the first review's manual count of
eight (which credited the `super().get()` call as a site; the pinned
analyzer does not and cannot see it, for the structural reason given above).

**Sites 2 and 7 are S4 candidates, not S4 exclusions.** `request.POST.get(...)`
and `request.GET.get(...)` are reported by `splinter-mypy` only because mypy
infers the unannotated `request` parameter as `Any` (above) — this is the
unresolved-receiver case S4 now names explicitly as *not* affirmative
evidence, independent of what `HttpRequest.GET`/`.POST` actually is at
runtime (Django's `QueryDict`, a dict-like lookup with no ORM path). Whether
sites 2 and 7 are ultimately excluded under S4 therefore turns on evidence
this document does not have and does not assert: the lowering would have to
establish the receiver's concrete type some other way than mypy's local
inference of this unannotated parameter — for example, by resolving
`request`'s type from the Django view-class method signature it overrides
(`django.views.generic.base.View.post` / the `TemplateResponseMixin`
contract passes it the first positional argument typed `HttpRequest` in
`django-stubs`), and then confirming `QueryDict.get` has no path to the ORM
in that stub's own method signature — and record that chain as the
exclusion's evidence. Until the lowering produces that affirmative evidence,
sites 2 and 7 stay in the denominator, classified **Unknown** if the
classifier cannot resolve their footprint by some other means. Of the 7
analyzer-reported candidates, sites 1, 3, 4, 5, and 6 are database
operations with no S4 question at all (their receivers resolve to a Manager
or model instance); sites 2 and 7 are S4 candidates whose exclusion is
undecided here, left to the lowering and its audit.

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
site. Running `splinter-eslint` (pinned command above, after `npm install`
in the target repository so the TypeScript checker resolves real types)
against this commit reports exactly **five** sites inside `data()`, each
its own S3 unit:

1. `this.cloudDBEntity.findOneBy({ id })` (line 33) — `findOneBy`, read;
   receiver type `Repository<CloudDBEntity>`.
2. `repository.save(params)` (line 36, the `add`/`update` branch) — `save`,
   write — one ORM-operation unit under the persistence-method rule above,
   regardless of how many `INSERT`/`UPDATE` statements it issues
   underneath.
3. `repository.delete(params.ids)` (line 42, the `delete` branch) —
   `delete`, write.
4. `repository.clear()` (line 45, the `clear` branch) — `clear`, write.
5. `repository.createQueryBuilder()` (lines 49–50, the `page` branch) —
   `createQueryBuilder`, "other." **This is reported**, which contradicts
   this document's previous draft, which excluded `createQueryBuilder()` on
   the stated ground that "it only builds the query and executes nothing."
   The pinned analyzer makes no such distinction: `createQueryBuilder` is
   one of its two "other" names, matched the same way `query` is, and is
   reported every time it is called — verified directly in the output.

**Not reported: `find.getMany()` and `find.getCount()`** (the review's
specifically disputed pair, part of the same `return` in the `page`
branch). Neither `getMany` nor `getCount` is a name in any of the plugin's
four lists (read/write/other/transaction, reproduced in full above) — the
pinned analyzer has no concept of a `QueryBuilder` terminal execution
method at all, so these two calls are structurally invisible to it, the
same way `super().get()` is invisible to the Django analyzer. This is the
opposite correction from what either side of the review's dispute assumed:
the fix is not "both calls count" or "one call per statement" — it is that
the published tool recognizes neither call by that name, while it does
recognize `createQueryBuilder()`, which the previous manual rule had
specifically excluded.

**S5 applies to this chain.** `createQueryBuilder()` (site 5) opens a
chain — `.offset(...).limit(...).orderBy(...)`, terminating in
`.getMany()` and `.getCount()` — that S5 represents as one unit by its
`createQueryBuilder()` site, whatever terminal methods follow. `getMany()`
and `getCount()` are not separate units omitted from the count, and not a
second site waiting to be added: they are this same chain's terminals,
already represented by site 5. Under S5, the count below is the chain's
correct unit count by rule, not merely an artifact of the analyzer's
silence on two names.

`this.getRepository(db?.className)` (line 34) is not itself a site:
`getRepository` is not a name in any of the four lists, confirmed by its
absence from the output. Under S2/S3, `getRepository` is a helper, not
lexically inside any transaction, so an S1 site written inside its own body
is its own unit at the helper's definition, by depth 0 — never by walking
from `data()` into it through the line-34 call. That helper's own body
(`async getRepository(className: string)`, lines 68–76 of the same file)
has exactly one such site, present in the output: `this.cloudDBEntity.findOneBy({
className })` (lines 70–72) — `findOneBy`, read. It is written once in the
source and is one unit, attributed to the helper's definition — plus one
unit.

**Total: 6 units** — 5 enumerated directly inside `data()`, plus 1 at the
helper `getRepository`'s own definition. This is one fewer than this
document's previous draft's total of 7, but by the opposite mechanism from
either the previous draft or the review's dispute: the pinned analyzer
drops both `getMany()` and `getCount()` from the count (neither is a
recognized name) while adding `createQueryBuilder()` back in (which the
previous draft had excluded) — net, one site removed from the `page`
branch, not two added or zero changed.

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

## 5. Amendment 16.10.2026: branch, index, hop, lowering and refusal rules

Decided before the classifier, the TPC-C lowering or the application
lowering exists and before any classification result exists, from an
independent plan review of open questions Q1-Q4 (#44). They make the
category definitions in `docs/experiments.md` ("Footprint categories")
operational and add one qualification to the application-side verdict
("Gating numerator and verdict"); a classifier builder applies them without
further judgement. Each rule states its bias direction on the coverage
number. No rule below is a result: none states or implies a category or
coverage number for TPC-C or any application.

**Q1. Branch composition.** *Changes:* what is classified. *Grounding:* the
frozen text classifies a *function's* footprint ("the complete set of rows,
ranges, or keys the function can touch"), so the unit is the whole function.

1. A function is classified over its complete body, every feasible branch.
2. A branch selected by an argument keeps the function static when each
   branch's footprint is static.
3. A branch selected by data counts the selector's dependency (the read that
   decides the branch) as well as each continuation's footprint.
4. Replacing a branch's footprint by a whole-table superset does not erase
   navigation: a read whose result determines a further access still counts
   under Q3 on that branch.
5. TPC-C keeps its five units and weights (§1). Payment's and Order-Status's
   by-name customer paths are part of the one gating classification of
   their unit. A by-id-only row may be reported for sensitivity and never
   gates. This corrects an earlier reading that made by-name sensitivity-only.

*Bias:* conservative against classifying by the easiest variant. *TPC-C:*
this moves Payment and Order-Status from the by-id variant to the complete
function, which can move a unit out of the numerator; the conservative
choice is taken.

**Q2. Secondary indexes.** *Changes:* which lookups are identifiable.
*Grounding:* "index or foreign key the compiler can identify ahead of time"
names an index, not a unique one.

1. A secondary index qualifies whether or not it is unique, if the schema of
   the application or of TPC-C declares it. An index is never inferred or
   added after a result is seen, and a unique constraint alone is not an
   index. The lowering uses only declared indexes and does not substitute a
   primary-key range for one.
2. A range over such an index bounded by arguments and the schema is static.
3. Resolving that range's contents into base-row keys not previously known
   is one hop (Q3), provided it is the function's only qualifying
   resolution.
4. An index-only read and a lookup followed by base-row accesses are
   distinct evidence; the classifier's evidence names which one applies.
   Ordering and median-selection semantics of the range are preserved by
   the lowering, else the function is refused (Q4).
5. A predicate on a secondary attribute with no declared index stays
   Unknown.
6. `kiln-ir` gains declared secondary indexes and indexed range reads in
   #45. Refusing index lookups for want of IR support would depress coverage
   through an implementation limit rather than the thesis, so the extension
   precedes the lowering.

*Bias:* admitting non-unique indexes follows the frozen text and raises
coverage; the declared-only restriction and the semantics requirement bound
it. *TPC-C:* this can move Payment's by-name path from Unknown to One-hop;
the conservative choice is to promote only when rule 3 holds on the whole
function (Q1), and Order-Status's further navigation is not assumed promoted.

**Q3. Hop counting.** *Changes:* what the single hop of "One-hop" counts.
*Grounding:* "exactly one data-dependent read whose result determines a
further access".

1. A function is one-hop when, on its most demanding feasible path (Q1),
   exactly one logical read has a result that determines further access
   targets through an identifiable index or foreign key (Q2).
2. A read whose result only affects returned values or control flow without
   changing any access target is not a resolution.
3. Several independent resolving reads exceed one even if they could run in
   parallel.
4. One resolving result may determine any number of later accesses without
   adding a hop.
5. A key for an insert derived from a read contributes that function's one
   resolution; the inserted row need not exist beforehand. This holds
   assuming all remaining behaviour qualifies.
6. A minimum, maximum or selection computed over one argument-bounded range
   of an identifiable index is computation, not a further read; using its
   result as an access key is one resolution. An arbitrary secondary scan is
   not covered by this rule.
7. Two or more chained resolutions are not One-hop. Such a function is
   Domain-bounded if provably confined to one argument-selected partition
   (`docs/experiments.md`'s definition), otherwise Unknown.
8. Splitting or combining independent reads syntactically (one query versus
   several, helper calls) does not change the count.

*Bias:* strict cardinality lowers coverage relative to counting parallel
rounds; rule 5 keeps writes from biasing upward. *TPC-C:* no unit's category
is stated here; the classifier assigns it by these rules. Recognising one
minimum or one lookup inside a unit that chains two or more resolutions
(rule 7) does not make it One-hop.

**Q4. Lowering fidelity and the audit oracle.** *Changes:* what a lowering
may omit and where audit expectations come from. *Grounding:* the
source-to-IR boundary of `docs/concept.md` §2.1 and the lowering audit.

1. A lowering may cover only recognised idioms, but never drops a helper's
   effects, failures, responses or constraints. A function with anything
   not translated is refused in full and stays in the denominator as
   Unknown (the frozen rule), and counts toward R as `docs/experiments.md`,
   "Gating numerator and verdict", defines it.
2. The lowering audit's expected outcomes are derived independently from the
   function's source and the framework's semantics, never from the IR under
   test; an expectation generated from that IR checks only consistency and
   does not satisfy the audit.
3. Both frozen seed-17 audit populations (§3), the affirmative S4 evidence
   and the separate access-log containment audit are unchanged. Controlled
   source execution is permitted as audit evidence, not required.

*Bias:* refusals depress coverage; a weak oracle could inflate it, hence
rule 2.

**Refusal-qualified falsification.** *Changes:* when an application-side
Fail counts as falsification. *Grounding:* "A unit landing strictly between
..." and "an unsupported function is never dropped from the denominator"
keep every denominator; this adds a reading, not a new threshold.

The definitions of L and U, the median(U) threshold and the required
report wording are owned by `docs/experiments.md` ("Gating numerator and
verdict"); this section keeps only the rationale. Refusals never leave a
denominator, so both L and U are always reported.

*Bias:* a Fail that refusals may have caused cannot falsify the thesis;
a Pass is untouched. Realised in the `kiln-e1 verdict` command's
application half in #2 slice 6.
