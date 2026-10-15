# `kiln-e1` Django extractor

`experiments/e1-classification-coverage/extract/django` turns the pinned
`splinter-mypy` analyzer's output into the Django unit records that
`crates/kiln-e1` reads. This file owns the observable behavior of that
extractor. It cites the frozen rule rather than restating it:
`experiments/e1-classification-coverage/inputs.md` §2 (manifest), §3 (S1-S4,
the pinned analyzer, its command and file selection) and §3.1 (worked
examples); the record shape is `UnitRecord` of `specs/kiln-e1.spec.md`
(`KILN-E1-01`). A test claims a criterion by its name prefix:
`test_kiln_e1dj_04_...` claims `KILN-E1DJ-04`. One criterion per test method;
several tests may share it.

## Sites and file selection

- [KILN-E1DJ-01] A site is exactly one analyzer message of type `method` whose method type is read, write or other, located at the call it reports; a model message and a transaction marker are not sites, and no site is added, removed or moved.
- [KILN-E1DJ-02] The analyzer is run with the one frozen command line of §3, unchanged for every application, on the application root checked out at its manifest `commit_sha_full`.
- [KILN-E1DJ-03] A message from a path with a directory component named `tests`, `test`, `migrations`, `node_modules`, `venv`, `.venv`, `docs` or `site-packages`, or from a file named `test_*.py`, `*_test.py` or `conftest.py`, is discarded before enumeration; every other message is kept.

## Units

- [KILN-E1DJ-04] A function decorated with `transaction.atomic` (bare or called) is one S2 unit positioned at its `def`, parameterized by its declared parameters, and absorbs every site written in its body, even when that is none.
- [KILN-E1DJ-05] A `with transaction.atomic(...)` block is one S2 unit positioned at the `with`, parameterized by the declared parameters of the function it is written in, and absorbs every site written in its body; a site in the `atomic(...)` call's own arguments is not in the body.
- [KILN-E1DJ-06] When explicit transactions are nested, the outermost one is the only unit and absorbs every site of its lexical body, including those of the nested ones.
- [KILN-E1DJ-07] Helpers are not expanded: a site in the body of a function the transaction merely calls is its own S3 unit, while a function defined inside the transaction body is part of it and its sites are absorbed.
- [KILN-E1DJ-08] A site outside every explicit transaction is its own S3 unit positioned at the site, whose absorbed sites are that site alone.
- [KILN-E1DJ-09] A statement holding several sites yields one S3 unit per site.
- [KILN-E1DJ-10] A site in a branch, ternary, comprehension or loop body is one unit as written, however often it runs.
- [KILN-E1DJ-11] An S3 unit's parameters are the declared parameters of its enclosing function followed by the names bound earlier in that function in source order, each once; a site outside any function has none.
- [KILN-E1DJ-12] A call the analyzer does not report (for example `super().get(...)`) yields no unit.
- [KILN-E1DJ-13] A transaction marker the source does not show as a decorator or `with` block, and a site whose file or position the source text does not contain, fail loudly naming the file.

## S4 evidence

- [KILN-E1DJ-14] No unit is removed for S4; each site is also reported with its called method, receiver expression and the analyzer's resolved receiver types, and a receiver the analyzer resolved to `Any` is marked unresolved, never as a resolved receiver.

## Output

- [KILN-E1DJ-15] A unit is one JSON line with exactly `subject` (`{"application": "<owner/repo>"}`), `file`, `line`, `column`, `rule` (`S2` or `S3`), `parameters` and `absorbed_sites` (each `file`, `line`, `column`), the file path relative to the application root.
- [KILN-E1DJ-16] Units are ordered by file, line, column and rule, and absorbed sites by line and column, whatever the order of the analyzer's messages, so a rerun over the same input is byte-identical.

## Worked examples

- [KILN-E1DJ-17] From the analyzer output recorded for babybuddy at its pinned commit, `TimerAddQuick.post` yields the 7 S3 units of §3.1.
- [KILN-E1DJ-18] From the analyzer output recorded for zulip at its pinned commit, `do_reactivate_user` yields 1 S2 unit.

## Corpus

- [KILN-E1DJ-19] Fetching an application clones its manifest row at `commit_sha_full` into the configured corpus directory and installs the pinned analyzer environment of §3 there; an unknown application, a missing corpus directory setting, or a directory inside the repository is rejected.
