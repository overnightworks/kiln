// The extractor's S2 units against a tiny oracle that applies the ruled definition literally to
// small generated programs: enumerate the paths, drop those that end in `throw`, run a stack per
// path, and match a start when no path leaves it open and some path pops it.

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

function generatedBlock(random, depth) {
  return Array.from({ length: 1 + random(3) }, () => {
    if (depth < MAX_DEPTH && random(4) === 0) {
      return { kind: "if", then: generatedBlock(random, depth + 1), otherwise: random(2) === 0 ? generatedBlock(random, depth + 1) : undefined };
    }
    return { kind: LEAF_KINDS[random(LEAF_KINDS.length)] };
  });
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
  const block = (statements) => {
    for (const statement of statements) {
      if (statement.kind !== "if") {
        emit(LEAF_TEXT[statement.kind], statement);
        continue;
      }
      emit("if (flag) {", statement);
      block(statement.then);
      if (statement.otherwise) {
        emit("} else {");
        block(statement.otherwise);
      }
      emit("}");
    }
  };
  block(program);
  lines.push("}", "");
  return lines.join("\n");
}

/** Every path through the statements as its events, with how it ends; paths that throw are gone. */
function pathsOf(statements) {
  return statements.reduce(
    (paths, statement) => paths.flatMap((path) => (path.ending === "end" ? pathsOfStatement(statement).map((next) => ({ events: [...path.events, ...next.events], ending: next.ending })) : [path])),
    [{ events: [], ending: "end" }],
  );
}

function pathsOfStatement(statement) {
  const onward = (events = []) => [{ events, ending: "end" }];
  if (statement.kind === "start") return onward([{ opens: statement.line }]);
  if (statement.kind === "commit" || statement.kind === "rollback") return onward([{ closes: statement.line }]);
  if (statement.kind === "write") return onward();
  if (statement.kind === "return") return [{ events: [], ending: "return" }];
  if (statement.kind === "throw") return [];
  return [...pathsOf(statement.then), ...(statement.otherwise ? pathsOf(statement.otherwise) : onward())];
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

function linesOf(statements, kind) {
  return statements.flatMap((statement) => [...(statement.kind === kind ? [statement.line] : []), ...linesOf(statement.then ?? [], kind), ...linesOf(statement.otherwise ?? [], kind)]);
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

test("KILN-E1TO-10 the extractor agrees with the definition applied literally on generated programs", () => {
  const random = randomSource(SEED);
  const seen = { matched: 0, unmatched: 0, programs: 0 };
  for (let attempt = 0; seen.programs < PROGRAM_COUNT; attempt += 1) {
    const program = generatedBlock(random, 1);
    const source = rendered(program);
    const expected = oracleOf(program);
    if (expected.pathCount > MAX_ORACLE_PATHS) continue;
    assert.deepEqual(extractedFrom(source), { units: expected.units, unmatched: expected.unmatched }, `generated program ${attempt}:\n${source}`);
    seen.programs += 1;
    seen.matched += expected.units.some(([rule]) => rule === "S2") ? 1 : 0;
    seen.unmatched += expected.unmatched.length > 0 ? 1 : 0;
  }
  assert.ok(seen.matched > 100 && seen.unmatched > 200, `the generated programs must exercise both outcomes: ${JSON.stringify(seen)}`);
});
