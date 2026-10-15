import assert from "node:assert/strict";
import { test } from "node:test";

import { enumerationOf, marker, shape, site } from "./unit_fixtures.mjs";

const START = "qr.startTransaction()";
const WRITE = /qr\.manager\.(save|insert)\((\w+)\)/g;

/** Every start of `qr` is a transaction marker and every `qr.manager` write a site. */
function enumerationOfFlow(source) {
  const starts = [...source.matchAll(/qr\.startTransaction\(\)/g)].map((_, index) => marker(source, START, "startTransaction", { occurrence: index + 1 }));
  const seen = new Map();
  const writes = [...source.matchAll(WRITE)].map(([needle, method]) => {
    seen.set(needle, (seen.get(needle) ?? 0) + 1);
    return site(source, needle, method, { methodType: "write", occurrence: seen.get(needle) });
  });
  return enumerationOf(source, [...starts, ...writes]);
}

const PARAMETERS = ["first", "second", "flag"];

const FLOWS = [
  [
    "a start left open in one arm is unmatched while the other arm's transaction is a unit of its own",
    `\
async function either(first, second, flag) {
  if (flag) {
    await qr.startTransaction();
    await qr.manager.save(first);
  } else {
    await qr.startTransaction();
    await qr.manager.insert(second);
    await qr.commitTransaction();
  }
}
`,
    [["S3", 4, ["first"], [4]], ["S2", 6, PARAMETERS, [7]]],
    [3],
  ],
  [
    "an outer transaction owns both arms even when each arm closes an inner and the outer",
    `\
async function outer(first, second, flag) {
  await qr.startTransaction();
  if (flag) {
    await qr.startTransaction();
    await qr.manager.save(first);
    await qr.commitTransaction();
    await qr.commitTransaction();
  } else {
    await qr.startTransaction();
    await qr.manager.insert(second);
    await qr.commitTransaction();
    await qr.commitTransaction();
  }
}
`,
    [["S2", 2, PARAMETERS, [5, 10]]],
    [],
  ],
  [
    "starts in switch clauses that fall through run in sequence and nest",
    `\
async function fallThrough(first, second, flag) {
  switch (flag) {
    case 1:
      await qr.startTransaction();
    case 2:
      await qr.startTransaction();
    default:
      await qr.manager.save(first);
  }
  await qr.commitTransaction();
  await qr.startTransaction();
  await qr.manager.insert(second);
  await qr.commitTransaction();
  await qr.commitTransaction();
}
`,
    [["S2", 4, PARAMETERS, [8, 12]]],
    [],
  ],
  [
    "a clause whose block ends in break leaves the switch, so the next clause is an alternative",
    `\
async function braces(first, second, flag) {
  switch (flag) {
    case 1: {
      await qr.startTransaction();
      break;
    }
    default:
      await qr.startTransaction();
  }
  await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S2", 4, PARAMETERS, [10]]],
    [],
  ],
  [
    "a start under a condition without an else is open on only some paths and unmatched",
    `\
async function maybe(first, second, flag) {
  if (flag) await qr.startTransaction();
  await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S3", 3, ["first"], [3]]],
    [2],
  ],
  [
    "a switch without default leaves a start open on only some paths unmatched",
    `\
async function cases(first, second, flag) {
  switch (flag) {
    case 1:
      await qr.startTransaction();
      break;
  }
  await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S3", 7, ["first"], [7]]],
    [4],
  ],
  [
    "a transaction closed in every arm stays closed for what follows",
    `\
async function closed(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  if (flag) await qr.commitTransaction();
  else await qr.rollbackTransaction();
  await qr.manager.insert(second);
}
`,
    [["S2", 2, PARAMETERS, [3]], ["S3", 6, ["second"], [6]]],
    [],
  ],
  [
    "a start in a loop whose commit lies after the loop is unmatched",
    `\
async function loopStart(first, second, flag) {
  for (const item of [first]) {
    await qr.startTransaction();
    await qr.manager.save(item);
  }
  await qr.commitTransaction();
}
`,
    [["S3", 4, ["item"], [4]]],
    [3],
  ],
  [
    "a start before a loop whose commit lies in the loop is unmatched",
    `\
async function loopCommit(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  while (flag) {
    await qr.commitTransaction();
  }
}
`,
    [["S3", 3, ["first"], [3]]],
    [2],
  ],
  [
    "a start and its commit inside one loop body are a transaction",
    `\
async function loopBoth(first, second, flag) {
  for (const item of [first]) {
    await qr.startTransaction();
    await qr.manager.save(item);
    await qr.commitTransaction();
  }
}
`,
    [["S2", 3, PARAMETERS, [4]]],
    [],
  ],
];

for (const [name, source, units, unmatchedLines] of FLOWS) {
  test(`KILN-E1TO-10 ${name}`, () => {
    const enumeration = enumerationOfFlow(source);
    assert.deepEqual(enumeration.units.map(shape), units);
    assert.deepEqual(enumeration.unmatchedMarkers.map(({ line }) => line), unmatchedLines);
  });
}
