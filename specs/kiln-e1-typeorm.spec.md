# `kiln-e1` TypeORM extractor

`experiments/e1-classification-coverage/extract/typeorm` turns the pinned
`@ctring/splinter-eslint` analyzer's output into the TypeORM unit records that
`crates/kiln-e1` reads. This file owns the observable behavior of that
extractor. It cites the frozen rule rather than restating it:
`experiments/e1-classification-coverage/inputs.md` §2 (manifest), §3 (S1-S5,
the pinned analyzer, its command and file selection, the Django clarification
of 16.10.2026 and the TypeORM clarification that follows it) and §3.1 (worked
examples); the record shape is `UnitRecord` of `specs/kiln-e1.spec.md`
(`KILN-E1-01`). A test claims a criterion by its name: a test named
`KILN-E1TO-04 ...` claims `KILN-E1TO-04`. Several tests may share a criterion.
Positions are the line (counted from 1) and column (counted from 0) of the
start of the reported expression.

## Sites and file selection

- [KILN-E1TO-01] A site is exactly one analyzer message of type `method` whose method type is read, write or other (the lists of §3), located at the call it reports; an `entity` message and a transaction marker are not sites, an unknown message type or method type fails loudly, and no site is added, removed or moved.
- [KILN-E1TO-02] The directory handed to the analyzer is that of the tracked `tsconfig.json` with the shortest path from the application root (fewest characters), ties broken by lexical order; the one frozen command is `splinter-eslint <that directory> --output <file>` without `--include`, run on the checkout at the manifest `commit_sha_full`.
- [KILN-E1TO-03] When no `tsconfig.json` is tracked, the frozen fallback command (`--include '**/*.ts'` on the application root) is used, and every message from a path with a directory component named `node_modules`, `dist`, `build`, `test`, `tests` or `__tests__`, or from a file named `*.spec.ts` or `*.test.ts`, is discarded before enumeration; nothing else is filtered.
- [KILN-E1TO-04] The analyzer reports paths relative to the directory it was given; every unit, site and marker is located by its path relative to the application root, and a report whose path lies outside the application root or outside the file selection (below the analyzed directory, or kept by the fallback) is not a site.
- [KILN-E1TO-05] Before the analyzer runs, the checkout must be at the manifest `commit_sha_full` with no tracked `.ts` file or `tsconfig.json` modified or deleted and no untracked or ignored `.ts` file inside the file selection (below `node_modules` is not inside it); otherwise the run fails loudly naming the difference.
- [KILN-E1TO-06] The analyzer exits 0 and reports a file it could not parse only on its error stream. A file that no `tsconfig.json` includes is outside the project and is the frozen selection at work: the run keeps the analyzer's error output verbatim in `<application>.analyzer.log` and goes on. Any other error output, and a non-zero exit, fails the run loudly, so no other file silently drops out of the denominator.

## Units

- [KILN-E1TO-07] `R.startTransaction()` (marker `startTransaction`) opens one S2 unit positioned at that call, parameterized by the declared parameters of the function it is written in; the unit ends at the last `R.commitTransaction()` or `R.rollbackTransaction()` with the same receiver text written directly in that function (not in a function nested in it) after the start and before the next `R.startTransaction()`, and it absorbs every site written between the start call and that last call.
- [KILN-E1TO-08] The callback (the last function or arrow function argument) of `X.transaction(...)` where `X` is, or ends in a property named, `dataSource`, `manager` or `connection`, is one S2 unit positioned at the call, parameterized like KILN-E1TO-07, and absorbs every site written in the callback's body; a site in another argument is not in the body.
- [KILN-E1TO-09] A transaction marker that opens no unit by KILN-E1TO-07 or -08 (a `@Transaction` method, a `transaction` call of another receiver or without a function argument, a start without a matching commit or rollback) opens no unit and its sites stay S3 or S5; each is written with file, line and column to the side output `<application>.unmatched-markers.jsonl`, and the run does not fail (clarification of §3, transaction markers).
- [KILN-E1TO-10] When explicit transactions are nested, the outermost one is the only unit and absorbs every site of its lexical body, including those of the nested ones.
- [KILN-E1TO-11] Helpers are not expanded: a site in the body of a function the transaction merely calls is its own unit, while a function defined inside the transaction body is part of it and its sites are absorbed.
- [KILN-E1TO-12] A site outside every explicit transaction and outside every query-builder chain is its own S3 unit positioned at the site, whose absorbed sites are that site alone; a statement holding several sites yields one unit per site.
- [KILN-E1TO-13] A site in a branch, ternary, loop body or callback is one unit as written, however often it runs.
- [KILN-E1TO-14] An S3 unit's parameters are the names the site's call expression reads (receiver and arguments) that are declared parameters of the innermost function around it, in declaration order, followed by those that are locals of that function bound before the site, in source order, each once; a name bound only by the site's own declaration or later, a name of an outer function or global, and a site outside any function contribute none (clarification of §3, S3 parameters).
- [KILN-E1TO-15] A query-builder chain (S5) is the reported `createQueryBuilder()` call together with every reported site on its receiver chain, that is the calls whose callee is a member access on the call before them, however many lines it spans; outside an explicit transaction it is one S5 unit positioned at the `createQueryBuilder()` site, absorbing all those sites, parameterized by the names read by the whole chain expression including methods the analyzer does not report.
- [KILN-E1TO-16] S5 never double-counts: a chain inside an explicit transaction contributes no unit of its own and all its sites are absorbed by the S2 unit; a chain outside one is never also S3 units of its sites.
- [KILN-E1TO-17] Only the written chain is a chain: a reported site called on a variable that holds a builder, or inside an argument of a chain method, is not part of that chain and is its own unit.
- [KILN-E1TO-18] A call the analyzer does not report (`getMany`, `execute`, `getRepository`) yields no unit.
- [KILN-E1TO-19] A reported site or marker is matched to its call by file, start, end and called name; if no call or more than one call fits, or the source text lacks the file, or the position is not inside the source text (line below 1 or beyond the last, column negative or beyond the line's width), the run fails loudly naming the file and position.

## S4 evidence

- [KILN-E1TO-20] No unit is removed for S4; each site is also reported with its called method, receiver expression and the analyzer's resolved receiver types, and a receiver resolved to `any`, `unknown` or `error` is marked unresolved, never as a resolved receiver.

## Output

- [KILN-E1TO-21] A unit is one JSON line with exactly `subject` (`{"application": "<owner/repo>"}`), `file`, `line`, `column`, `rule` (`S2`, `S3` or `S5`), `parameters` and `absorbed_sites` (each `file`, `line`, `column`), the file path relative to the application root.
- [KILN-E1TO-22] Units are ordered by file, line, column and rule, and absorbed sites by start and then end, whatever the order of the analyzer's messages, so a rerun over the same input is byte-identical.

## Worked examples

- [KILN-E1TO-23] From the analyzer output recorded for EPGStation at its pinned commit, `restore` yields 1 S2 unit parameterized by `items` that absorbs its 4 sites.
- [KILN-E1TO-24] From the same recording, `insertOnce` yields 1 S5 unit parameterized by `recorded` that absorbs its 2 reported sites.
- [KILN-E1TO-25] From the analyzer output recorded for cool-admin-midway at its pinned commit, `CloudDBService.data` yields its 5 units and `getRepository` its 1 (§3.1: 6 in all).

## Corpus

- [KILN-E1TO-26] Fetching an application clones its manifest row at `commit_sha_full` into the configured corpus directory; its own dependencies are installed there by a separate command and the pinned analyzer environment of §3 (`@ctring/splinter-eslint@1.0.9`, `typescript@5.4.3`) by another; an unknown application, a missing corpus directory setting, or a directory inside the repository is rejected.
