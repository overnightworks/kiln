import assert from "node:assert/strict";
import { test } from "node:test";

import ts from "typescript";

import { MAX_PATHS_PER_FUNCTION, PATH_CAP_EXCEEDED, UNSUPPORTED_CONTROL_FLOW, closingsOfStartedTransactions } from "./transaction_flow.mjs";
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
    "a start under a condition without an else is matched when every path that reaches it closes it",
    `\
async function maybe(first, second, flag) {
  if (flag) await qr.startTransaction();
  await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S2", 2, PARAMETERS, [3]]],
    [],
  ],
  [
    "a start under a condition without an else and without a commit is unmatched",
    `\
async function leaks(first, second, flag) {
  if (flag) await qr.startTransaction();
  await qr.manager.save(first);
}
`,
    [["S3", 3, ["first"], [3]]],
    [2],
  ],
  [
    "a start in a switch without default is matched when every path that reaches it closes it",
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
    [["S2", 4, PARAMETERS, [7]]],
    [],
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
    "a transaction one arm leaves open is unmatched even though the other arm commits",
    `\
async function partial(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  if (flag) await qr.commitTransaction();
  else await qr.manager.insert(second);
}
`,
    [["S3", 3, ["first"], [3]], ["S3", 5, ["second"], [5]]],
    [2],
  ],
  [
    "a start whose commit lies under a condition of its own arm is unmatched",
    `\
async function guarded(first, second, flag) {
  if (flag) {
    await qr.startTransaction();
    await qr.manager.save(first);
    if (second) await qr.commitTransaction();
  }
}
`,
    [["S3", 4, ["first"], [4]]],
    [3],
  ],
  [
    "a transaction every arm commits is one unit absorbing the sites before and inside the arms",
    `\
async function bothCommit(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  if (flag) {
    await qr.manager.insert(second);
    await qr.commitTransaction();
  } else {
    await qr.manager.save(second);
    await qr.commitTransaction();
  }
}
`,
    [["S2", 2, PARAMETERS, [3, 5, 8]]],
    [],
  ],
  [
    "an arm that rolls back and returns does not stop the later commit from closing the transaction",
    `\
async function earlyReturn(first, second, flag) {
  await qr.startTransaction();
  if (flag) {
    await qr.rollbackTransaction();
    return;
  }
  await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S2", 2, PARAMETERS, [7]]],
    [],
  ],
  [
    "an arm that returns while the transaction is still open leaves the start unmatched",
    `\
async function leakingReturn(first, second, flag) {
  await qr.startTransaction();
  if (flag) return;
  await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S3", 4, ["first"], [4]]],
    [2],
  ],
  [
    "a catch that rolls back and rethrows leaves the code after the try reachable",
    `\
async function rethrow(first, second, flag) {
  try {
    await qr.startTransaction();
    await qr.manager.save(first);
    await qr.commitTransaction();
  } catch (error) {
    await qr.rollbackTransaction();
    throw error;
  }
  await qr.startTransaction();
  await qr.manager.insert(second);
  await qr.commitTransaction();
}
`,
    [["S2", 3, PARAMETERS, [4]], ["S2", 10, PARAMETERS, [11]]],
    [],
  ],
  [
    "a discarded inner opening does not let a later commit close the outer transaction: the outer start is unmatched and the inner one, popped on every path, owns what follows it",
    `\
async function innerGuarded(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  await qr.startTransaction();
  if (flag) await qr.commitTransaction();
  await qr.manager.insert(second);
  await qr.commitTransaction();
}
`,
    [["S3", 3, ["first"], [3]], ["S2", 4, PARAMETERS, [6]]],
    [2],
  ],
  [
    "a start made on only one path inside an open transaction leaves the outer start unmatched",
    `\
async function innerConditional(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  if (flag) await qr.startTransaction();
  await qr.manager.insert(second);
  await qr.commitTransaction();
}
`,
    [["S3", 3, ["first"], [3]], ["S2", 4, PARAMETERS, [5]]],
    [2],
  ],
  [
    "a thrown arm that continues past the join does not decide the start",
    `\
async function throwsThenCommits(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  if (flag) throw first;
  await qr.commitTransaction();
  return;
}
`,
    [["S2", 2, PARAMETERS, [3]]],
    [],
  ],
  [
    "a thrown arm beside an arm that commits and returns does not decide the start either",
    `\
async function throwsOrCommitsAndReturns(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  if (flag) throw first;
  else {
    await qr.commitTransaction();
    return;
  }
}
`,
    [["S2", 2, PARAMETERS, [3]]],
    [],
  ],
  [
    "a finally that commits closes the transaction of a try body that returns",
    `\
async function finallyCommits(first, second, flag) {
  await qr.startTransaction();
  try {
    await qr.manager.save(first);
    return;
  } finally {
    await qr.commitTransaction();
  }
}
`,
    [["S2", 2, PARAMETERS, [4]]],
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

/** Functions whose control flow the path definition does not cover: every start in them is unmatched with that reason. */
const UNSUPPORTED = [
  [
    "a start in a loop whose commit lies after the loop",
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
    "a start before a loop whose commit lies in the loop",
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
    "a start and its commit inside one loop body",
    `\
async function loopBoth(first, second, flag) {
  for (const item of [first]) {
    await qr.startTransaction();
    await qr.manager.save(item);
    await qr.commitTransaction();
  }
}
`,
    [["S3", 4, ["item"], [4]]],
    [3],
  ],
  [
    "a break out of a loop body before the commit",
    `\
async function loopBreak(first, second, flag) {
  for (const item of [first]) {
    await qr.startTransaction();
    await qr.manager.save(item);
    if (flag) break;
    await qr.commitTransaction();
  }
}
`,
    [["S3", 4, ["item"], [4]]],
    [3],
  ],
  [
    "a continue in a switch clause inside a loop with a start",
    `\
async function switchContinue(first, second, flag) {
  for (const item of [first]) {
    await qr.startTransaction();
    switch (flag) {
      case 1:
        continue;
      default:
        await qr.manager.save(item);
    }
    await qr.commitTransaction();
  }
}
`,
    [["S3", 8, ["item"], [8]]],
    [3],
  ],
  [
    "a return in a loop inside a try whose finally commits, with the start in the loop",
    `\
async function loopReturnFinally(first, second, flag) {
  try {
    while (flag) {
      await qr.startTransaction();
      return;
    }
  } finally {
    await qr.commitTransaction();
  }
}
`,
    [],
    [4],
  ],
  [
    "a return in a loop inside a try whose finally commits, with the start before the try",
    `\
async function returnFinally(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  try {
    while (flag) {
      return;
    }
  } finally {
    await qr.commitTransaction();
  }
}
`,
    [["S3", 3, ["first"], [3]]],
    [2],
  ],
  [
    "a loop that opens a second transaction while an outer one is open, so the final commit is not the outer one's",
    `\
async function outerThenLoop(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  while (flag) {
    await qr.startTransaction();
  }
  await qr.commitTransaction();
}
`,
    [["S3", 3, ["first"], [3]]],
    [2, 5],
  ],
  [
    "a do-while whose condition commits after a continue",
    `\
async function doWhileCommit(first, second, flag) {
  await qr.manager.save(first);
  do {
    await qr.startTransaction();
    continue;
  } while (await qr.commitTransaction());
}
`,
    [["S3", 2, ["first"], [2]]],
    [4],
  ],
  [
    "a labelled break past a start and its commit",
    `\
async function labelled(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  done: {
    break done;
  }
  await qr.commitTransaction();
}
`,
    [["S3", 3, ["first"], [3]]],
    [2],
  ],
  [
    "a labelled continue in a function with a start",
    `\
async function labelledContinue(first, second, flag) {
  await qr.startTransaction();
  outer: for (const item of [first]) {
    for (const other of [second]) {
      if (flag) continue outer;
    }
  }
  await qr.manager.save(second);
  await qr.commitTransaction();
}
`,
    [["S3", 8, ["second"], [8]]],
    [2],
  ],
  [
    "a loop in the file body beside a start",
    `\
await qr.startTransaction();
for (const item of [first]) {
  await qr.manager.save(item);
  await qr.commitTransaction();
}
`,
    [["S3", 3, [], [3]]],
    [1],
  ],
];

for (const [name, source, units, unmatchedLines] of UNSUPPORTED) {
  test(`KILN-E1TO-10 unsupported control flow: ${name} leaves every start in the function unmatched`, () => {
    const enumeration = enumerationOfFlow(source);
    assert.deepEqual(enumeration.units.map(shape), units);
    assert.deepEqual(enumeration.unmatchedMarkers.map(({ line }) => line), unmatchedLines);
    assert.deepEqual([...new Set(enumeration.unmatchedMarkers.map(({ reason }) => reason))], [UNSUPPORTED_CONTROL_FLOW]);
  });
}

/** Loops and labels that cannot touch a transaction, or whose jump stays inside the supported constructs, are still enumerated. */
const SUPPORTED_NEAR_MISSES = [
  [
    "a loop and a break in it that hold no transaction call",
    `\
async function loopWithoutTransactionCalls(first, second, flag) {
  await qr.startTransaction();
  for (const item of [first]) {
    await qr.manager.save(item);
    if (flag) break;
  }
  await qr.commitTransaction();
}
`,
    [["S2", 2, PARAMETERS, [4]]],
    [],
  ],
  [
    "a return in a loop inside a try whose finally has no terminator",
    `\
async function finallyWithoutTerminator(first, second, flag) {
  await qr.startTransaction();
  await qr.manager.save(first);
  await qr.commitTransaction();
  try {
    while (flag) {
      return;
    }
  } finally {
    audit.log(first);
  }
}
`,
    [["S2", 2, PARAMETERS, [3]]],
    [],
  ],
  [
    "a function nested in a loop keeps its own control flow",
    `\
async function outerLoop(first, second, flag) {
  for (const item of [first]) {
    run(async () => {
      await qr.startTransaction();
      await qr.manager.save(item);
      await qr.commitTransaction();
    });
  }
}
`,
    [["S2", 4, [], [5]]],
    [],
  ],
  [
    "a label that no jump names",
    `\
async function unusedLabel(first, second, flag) {
  await qr.startTransaction();
  here: await qr.manager.save(first);
  await qr.commitTransaction();
}
`,
    [["S2", 2, PARAMETERS, [3]]],
    [],
  ],
];

for (const [name, source, units, unmatchedLines] of SUPPORTED_NEAR_MISSES) {
  test(`KILN-E1TO-10 ${name} is still matched by the path definition`, () => {
    const enumeration = enumerationOfFlow(source);
    assert.deepEqual(enumeration.units.map(shape), units);
    assert.deepEqual(enumeration.unmatchedMarkers.map(({ line }) => line), unmatchedLines);
  });
}

/** A function with `count` independent conditional starts has 2^count distinct paths. */
const conditionalStarts = (count) =>
  `async function many(first, flag) {\n${"  if (flag) await qr.startTransaction();\n".repeat(count)}  await qr.manager.save(first);\n}\n\nasync function few(first) {\n  await qr.startTransaction();\n  await qr.manager.insert(first);\n  await qr.commitTransaction();\n}\n`;

test("KILN-E1TO-10 a function with more distinct paths than the cap leaves every start in it unmatched with the reason, and other functions are unaffected", () => {
  const starts = Math.log2(MAX_PATHS_PER_FUNCTION) + 1;
  const source = conditionalStarts(starts);
  const enumeration = enumerationOfFlow(source);
  const lines = Array.from({ length: starts }, (_, index) => index + 2);
  assert.deepEqual(enumeration.unmatchedMarkers.map(({ line }) => line), lines);
  assert.deepEqual([...new Set(enumeration.unmatchedMarkers.map(({ reason }) => reason))], [PATH_CAP_EXCEEDED]);
  assert.deepEqual(enumeration.units.map(shape).map(([rule, line]) => `${rule}${line}`), [`S3${starts + 2}`, `S2${starts + 6}`]);
});

test("KILN-E1TO-10 a function at the cap is enumerated and one path more is not", () => {
  const sourceFile = ts.createSourceFile("cap.ts", conditionalStarts(3), ts.ScriptTarget.Latest, true);
  const reasonsAt = (maxPaths) => [...closingsOfStartedTransactions(sourceFile, { maxPaths }).values()].filter((closing) => closing.unmatched === PATH_CAP_EXCEEDED).length;
  assert.equal(reasonsAt(8), 0);
  assert.equal(reasonsAt(7), 3);
});
