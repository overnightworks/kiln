// The extractor's S2 units against a tiny oracle that applies the ruled definition literally to
// small generated programs of the supported constructs (if/else, switch with fall-through and with
// or without default, try/catch/finally, return, throw): enumerate the paths, drop those that end
// in `throw`, run a stack per path, and match a start when no path leaves it open and some path
// pops it.

import assert from "node:assert/strict";
import { test } from "node:test";

import { enumerationOf, marker, shape, site } from "./unit_fixtures.mjs";

const PROGRAM_COUNT = 1000;
const SEED = 20261016;
const MAX_DEPTH = 3;
const MAX_ORACLE_PATHS = 500;

function randomSource(seed) {
  let state = seed;
  return (below) => {
    state = (state + 0x6d2b79f5) | 0;
    let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return Math.floor((((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296) * below);
  };
}

const LEAF_KINDS = ["start", "start", "start", "commit", "commit", "commit", "rollback", "write", "write", "write", "return", "throw"];

function generatedBlock(random, depth, minLength = 1) {
  return Array.from({ length: minLength + random(3) }, () => (depth < MAX_DEPTH && random(3) === 0 ? generatedComposite(random, depth + 1) : { kind: LEAF_KINDS[random(LEAF_KINDS.length)] }));
}

const maybe = (random, make) => (random(2) === 0 ? make() : undefined);

function generatedComposite(random, depth) {
  const kind = ["if", "switch", "try"][random(3)];
  if (kind === "if") return { kind, then: generatedBlock(random, depth), otherwise: maybe(random, () => generatedBlock(random, depth)) };
  if (kind === "switch") {
    const clauses = Array.from({ length: 1 + random(3) }, () => ({ body: generatedBlock(random, depth, 0), breaks: random(2) === 0 }));
    return { kind, clauses, defaultAt: maybe(random, () => random(clauses.length)) };
  }
  const caught = maybe(random, () => generatedBlock(random, depth));
  return { kind, body: generatedBlock(random, depth), caught, finalized: caught === undefined ? generatedBlock(random, depth) : maybe(random, () => generatedBlock(random, depth)) };
}

const LEAF_TEXT = {
  start: "await qr.startTransaction();",
  commit: "await qr.commitTransaction();",
  rollback: "await qr.rollbackTransaction();",
  write: "await qr.manager.save(first);",
  return: "return;",
  throw: "throw error;",
};

/** One statement per line; every statement learns its line. */
function rendered(program) {
  const lines = ["async function generated(first, flag) {"];
  const emit = (text, statement) => {
    lines.push(text);
    if (statement) statement.line = lines.length;
  };
  const block = (statements) => statements.forEach(statementText);
  const statementText = (statement) => {
    if (statement.kind === "if") {
      emit("if (flag) {", statement);
      block(statement.then);
      if (statement.otherwise) {
        emit("} else {");
        block(statement.otherwise);
      }
      emit("}");
    } else if (statement.kind === "switch") {
      emit("switch (flag) {", statement);
      statement.clauses.forEach((clause, index) => {
        emit(index === statement.defaultAt ? "default:" : `case ${index}:`);
        block(clause.body);
        if (clause.breaks) emit("break;");
      });
      emit("}");
    } else if (statement.kind === "try") {
      emit("try {", statement);
      block(statement.body);
      if (statement.caught) {
        emit("} catch (error) {");
        block(statement.caught);
      }
      if (statement.finalized) {
        emit("} finally {");
        block(statement.finalized);
      }
      emit("}");
    } else {
      emit(LEAF_TEXT[statement.kind], statement);
    }
  };
  block(program);
  lines.push("}", "");
  return lines.join("\n");
}

const followedBy = (path, nexts) => nexts.map((next) => ({ events: [...path.events, ...next.events], ending: next.ending }));

/** Every path through the statements as its events, with how it ends; paths that throw are gone. */
function pathsOf(statements) {
  return statements.reduce((paths, statement) => paths.flatMap((path) => (path.ending === "end" ? followedBy(path, pathsOfStatement(statement)) : [path])), [{ events: [], ending: "end" }]);
}

function pathsOfStatement(statement) {
  const onward = (events = []) => [{ events, ending: "end" }];
  if (statement.kind === "start") return onward([{ opens: statement.line }]);
  if (statement.kind === "commit" || statement.kind === "rollback") return onward([{ closes: statement.line }]);
  if (statement.kind === "write") return onward();
  if (statement.kind === "return") return [{ events: [], ending: "return" }];
  if (statement.kind === "throw") return [];
  if (statement.kind === "switch") return [...statement.clauses.flatMap((_, entry) => pathsFromClause(statement.clauses.slice(entry))), ...(statement.defaultAt === undefined ? onward() : [])];
  if (statement.kind === "try") return pathsOfTry(statement);
  return [...pathsOf(statement.then), ...(statement.otherwise ? pathsOf(statement.otherwise) : onward())];
}

/** Control enters at the first clause given and runs on into the next until a `break`, a `return` or the last clause. */
function pathsFromClause([clause, ...later]) {
  return pathsOf(clause.body).flatMap((path) => {
    if (path.ending !== "end") return [path];
    if (clause.breaks || later.length === 0) return [path];
    return followedBy(path, pathsFromClause(later));
  });
}

/** After a body that completes the code goes on with or without the catch; the finally follows every path that did not throw. */
function pathsOfTry({ body, caught, finalized }) {
  const afterCatch = pathsOf(body).flatMap((path) => (path.ending === "end" && caught ? [path, ...followedBy(path, pathsOf(caught))] : [path]));
  if (!finalized) return afterCatch;
  return afterCatch.flatMap((path) => followedBy(path, pathsOf(finalized)).map((closing) => ({ ...closing, ending: closing.ending === "end" ? path.ending : closing.ending })));
}

/** The lines of the starts that are matched, each with the line of the last terminator that pops it. */
function matchedStarts(program, startLines) {
  const left = new Set();
  const popped = new Map();
  for (const { events } of pathsOf(program)) {
    const open = [];
    for (const event of events) {
      if (event.opens) open.push(event.opens);
      else if (open.length > 0) {
        const start = open.pop();
        popped.set(start, Math.max(popped.get(start) ?? 0, event.closes));
      }
    }
    open.forEach((line) => left.add(line));
  }
  return { matched: new Map(startLines.filter((line) => popped.has(line) && !left.has(line)).map((line) => [line, popped.get(line)])), unmatched: startLines.filter((line) => !popped.has(line) || left.has(line)) };
}

const nestedBlocks = (statement) => [statement.then, statement.otherwise, statement.body, statement.caught, statement.finalized, ...(statement.clauses ?? []).map(({ body }) => body)].filter(Boolean);

function linesOf(statements, kind) {
  return statements.flatMap((statement) => [...(statement.kind === kind ? [statement.line] : []), ...nestedBlocks(statement).flatMap((nested) => linesOf(nested, kind))]);
}

/** Expected units as [rule, line, absorbed lines] and the lines of the unmatched starts. */
function oracleOf(program) {
  const { matched, unmatched } = matchedStarts(program, linesOf(program, "start").sort((first, second) => first - second));
  const blocks = [...matched].map(([start, end]) => ({ start, end }));
  const outermost = blocks.filter((block) => !blocks.some((other) => other !== block && other.start < block.start && block.start < other.end));
  const writes = linesOf(program, "write").sort((first, second) => first - second);
  const insideBlock = (block, line) => block.start < line && line < block.end;
  const units = [
    ...outermost.map((block) => ["S2", block.start, writes.filter((line) => insideBlock(block, line))]),
    ...writes.filter((line) => !outermost.some((block) => insideBlock(block, line))).map((line) => ["S3", line, [line]]),
  ].sort((first, second) => first[1] - second[1]);
  return { units, unmatched, pathCount: pathsOf(program).length };
}

function extractedFrom(source) {
  const starts = [...source.matchAll(/qr\.startTransaction\(\)/g)].map((_, index) => marker(source, "qr.startTransaction()", "startTransaction", { occurrence: index + 1 }));
  const writes = [...source.matchAll(/qr\.manager\.save\(first\)/g)].map((_, index) => site(source, "qr.manager.save(first)", "save", { methodType: "write", occurrence: index + 1 }));
  const { units, unmatchedMarkers } = enumerationOf(source, [...starts, ...writes]);
  return {
    units: units.map(shape).map(([rule, line, , absorbed]) => [rule, line, absorbed]),
    unmatched: unmatchedMarkers.map(({ line }) => line),
  };
}

const containsStatement = (statements, matches) => statements.some((statement) => matches(statement) || nestedBlocks(statement).some((nested) => containsStatement(nested, matches)));
const isSwitchWithoutDefault = (statement) => statement.kind === "switch" && statement.defaultAt === undefined;
const isFallThroughSwitch = (statement) => statement.kind === "switch" && statement.clauses.slice(0, -1).some(({ breaks }) => !breaks);
const isTryFinallyWithReturn = (statement) => statement.kind === "try" && statement.finalized !== undefined && containsStatement(statement.body, ({ kind }) => kind === "return");

test("KILN-E1TO-10 the extractor agrees with the definition applied literally on generated programs", () => {
  const random = randomSource(SEED);
  const seen = { matched: 0, unmatched: 0, programs: 0, matchedWithSwitchWithoutDefault: 0, matchedWithFallThrough: 0, matchedWithTryFinallyReturn: 0 };
  for (let attempt = 0; seen.programs < PROGRAM_COUNT; attempt += 1) {
    const program = generatedBlock(random, 1);
    const source = rendered(program);
    const expected = oracleOf(program);
    if (expected.pathCount > MAX_ORACLE_PATHS) continue;
    assert.deepEqual(extractedFrom(source), { units: expected.units, unmatched: expected.unmatched }, `generated program ${attempt}:\n${source}`);
    const hasUnit = expected.units.some(([rule]) => rule === "S2");
    seen.programs += 1;
    seen.matched += hasUnit ? 1 : 0;
    seen.unmatched += expected.unmatched.length > 0 ? 1 : 0;
    seen.matchedWithSwitchWithoutDefault += hasUnit && containsStatement(program, isSwitchWithoutDefault) ? 1 : 0;
    seen.matchedWithFallThrough += hasUnit && containsStatement(program, isFallThroughSwitch) ? 1 : 0;
    seen.matchedWithTryFinallyReturn += hasUnit && containsStatement(program, isTryFinallyWithReturn) ? 1 : 0;
  }
  assert.ok(seen.matched > 100 && seen.unmatched > 200, `the generated programs must exercise both outcomes: ${JSON.stringify(seen)}`);
  const exercised = [seen.matchedWithSwitchWithoutDefault, seen.matchedWithFallThrough, seen.matchedWithTryFinallyReturn];
  assert.ok(exercised.every((count) => count > 10), `the generated programs must match starts around switch and try/finally: ${JSON.stringify(seen)}`);
});
