import assert from "node:assert/strict";
import { test } from "node:test";

import { enumerationOf, marker, shape, site, unitsOf } from "./unit_fixtures.mjs";

const START_COMMIT = `\
async function restore(items: Item[]) {
  const queryRunner = connection.createQueryRunner();
  await queryRunner.startTransaction();
  try {
    await queryRunner.manager.delete(Thumb, {});
    for (const item of items) {
      await queryRunner.manager.insert(Rec, item);
    }
    await queryRunner.commitTransaction();
  } catch (err) {
    await queryRunner.rollbackTransaction();
  }
  await audit.save(items);
}
`;

function startCommitMessages(source) {
  return [
    marker(source, "queryRunner.startTransaction()", "startTransaction"),
    site(source, "queryRunner.manager.delete(Thumb, {})", "delete", { methodType: "write" }),
    site(source, "queryRunner.manager.insert(Rec, item)", "insert", { methodType: "write" }),
    site(source, "audit.save(items)", "save", { methodType: "write" }),
  ];
}

test("KILN-E1TO-07 startTransaction opens one S2 unit up to the last commit or rollback, parameterized by the function", () => {
  const units = unitsOf(START_COMMIT, startCommitMessages(START_COMMIT));
  assert.deepEqual(units.map(shape), [
    ["S2", 3, ["items"], [5, 7]],
    ["S3", 13, ["items"], [13]],
  ]);
  assert.equal(units[0].column, 8);
});

const TWO_STARTS = `\
async function twice(a: number) {
  await qr.startTransaction();
  await qr.manager.save(a);
  await qr.commitTransaction();
  await other.save(a);
  await qr.startTransaction();
  await qr.manager.save(a);
  await qr.rollbackTransaction();
}
`;

test("KILN-E1TO-07 a second start of the same receiver opens its own unit and sites between the two stay outside", () => {
  const messages = [
    marker(TWO_STARTS, "qr.startTransaction()", "startTransaction"),
    marker(TWO_STARTS, "qr.startTransaction()", "startTransaction", { occurrence: 2 }),
    site(TWO_STARTS, "qr.manager.save(a)", "save", { methodType: "write" }),
    site(TWO_STARTS, "other.save(a)", "save", { methodType: "write" }),
    site(TWO_STARTS, "qr.manager.save(a)", "save", { methodType: "write", occurrence: 2 }),
  ];
  assert.deepEqual(unitsOf(TWO_STARTS, messages).map(shape), [
    ["S2", 2, ["a"], [3]],
    ["S3", 5, ["a"], [5]],
    ["S2", 6, ["a"], [7]],
  ]);
});

const CALLBACKS = `\
async function pay(order: Order, amount: number) {
  const ledger = await getLedger();
  const first = dataSource.transaction("SERIALIZABLE", async (manager) => manager.save(order, ledger.insert(amount)));
  await this.connection.transaction(async (m) => m.delete(Order, order));
  await sequelize.transaction(async (t) => t.save(order));
  await manager.transaction(namedWork);
}
`;

const FIRST_TRANSACTION = 'dataSource.transaction("SERIALIZABLE", async (manager) => manager.save(order, ledger.insert(amount)))';

test("KILN-E1TO-08 the callback of dataSource, connection or manager .transaction is one S2 unit, other receivers and callbacks by name are not", () => {
  const messages = [
    marker(CALLBACKS, FIRST_TRANSACTION, "transaction"),
    marker(CALLBACKS, "this.connection.transaction(async (m) => m.delete(Order, order))", "transaction"),
    marker(CALLBACKS, "sequelize.transaction(async (t) => t.save(order))", "transaction"),
    marker(CALLBACKS, "manager.transaction(namedWork)", "transaction"),
    site(CALLBACKS, "manager.save(order, ledger.insert(amount))", "save", { methodType: "write" }),
    site(CALLBACKS, "ledger.insert(amount)", "insert", { methodType: "write" }),
    site(CALLBACKS, "m.delete(Order, order)", "delete", { methodType: "write" }),
    site(CALLBACKS, "t.save(order)", "save", { methodType: "write" }),
  ];
  const enumeration = enumerationOf(CALLBACKS, messages);
  assert.deepEqual(enumeration.units.map(shape), [
    ["S2", 3, ["order", "amount"], [3, 3]],
    ["S2", 4, ["order", "amount"], [4]],
    ["S3", 5, ["t"], [5]],
  ]);
  assert.deepEqual(enumeration.unmatchedMarkers.map(({ line }) => line), [5, 6]);
});

test("KILN-E1TO-08 a site in an argument of the transaction call other than the callback is not in its body", () => {
  const source = "function run(a) {\n  dataSource.transaction(level(repo.count(a)), async (m) => m.save(a));\n}\n";
  const messages = [
    marker(source, "dataSource.transaction(level(repo.count(a)), async (m) => m.save(a))", "transaction"),
    site(source, "repo.count(a)", "count"),
    site(source, "m.save(a)", "save", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(source, messages).map(shape), [
    ["S2", 2, ["a"], [2]],
    ["S3", 2, ["a"], [2]],
  ]);
});

test("KILN-E1TO-09 a marker that opens no unit is recorded, does not fail the run and leaves its sites outside", () => {
  const source = `\
class Pay {
  @Transaction()
  async decorated(a) {
    await repo.save(a);
  }

  async unfinished(b) {
    await qr.startTransaction();
    await repo.save(b);
  }
}
`;
  const messages = [
    marker(source, "@Transaction()\n  async decorated(a) {\n    await repo.save(a);\n  }", "decorated"),
    marker(source, "qr.startTransaction()", "startTransaction"),
    site(source, "repo.save(a)", "save", { methodType: "write" }),
    site(source, "repo.save(b)", "save", { methodType: "write" }),
  ];
  const enumeration = enumerationOf(source, messages);
  assert.deepEqual(enumeration.units.map(shape), [
    ["S3", 4, ["a"], [4]],
    ["S3", 9, ["b"], [9]],
  ]);
  assert.deepEqual(enumeration.unmatchedMarkers, [
    { file: "src/db.ts", line: 2, column: 2 },
    { file: "src/db.ts", line: 8, column: 10 },
  ]);
});

const NESTED = `\
async function outer(a) {
  await dataSource.transaction(async (m) => {
    await m.save(a);
    await qr.startTransaction();
    await m.insert(a);
    await dataSource.transaction(async (n) => n.delete(a));
    await qr.commitTransaction();
  });
  await helper(a);
}
`;

test("KILN-E1TO-10 nested explicit transactions are one unit owned by the outermost", () => {
  const outerCall = NESTED.slice(NESTED.indexOf("dataSource.transaction(async (m)"), NESTED.indexOf("\n  await helper") - 1);
  const messages = [
    marker(NESTED, outerCall, "transaction"),
    marker(NESTED, "qr.startTransaction()", "startTransaction"),
    marker(NESTED, "dataSource.transaction(async (n) => n.delete(a))", "transaction"),
    site(NESTED, "m.save(a)", "save", { methodType: "write" }),
    site(NESTED, "m.insert(a)", "insert", { methodType: "write" }),
    site(NESTED, "n.delete(a)", "delete", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(NESTED, messages).map(shape), [["S2", 2, ["a"], [3, 5, 6]]]);
});

const HELPERS = `\
async function helper(id) {
  return repo.findOneBy({ id });
}

async function pay(id) {
  await dataSource.transaction(async (m) => {
    await helper(id);
    const inner = async () => m.find(id);
    await inner();
  });
}
`;

test("KILN-E1TO-11 a helper's own site is its own unit while a function defined inside the transaction is part of it", () => {
  const messages = [
    marker(HELPERS, "dataSource.transaction(async (m) => {\n    await helper(id);\n    const inner = async () => m.find(id);\n    await inner();\n  })", "transaction"),
    site(HELPERS, "repo.findOneBy({ id })", "findOneBy"),
    site(HELPERS, "m.find(id)", "find"),
  ];
  assert.deepEqual(unitsOf(HELPERS, messages).map(shape), [
    ["S3", 2, ["id"], [2]],
    ["S2", 6, ["id"], [8]],
  ]);
});

const NESTED_STARTS = `\
async function outer(a) {
  await qr.startTransaction();
  await qr.manager.save(a);
  await qr.startTransaction();
  await qr.manager.insert(a);
  await qr.commitTransaction();
  await qr.manager.delete(a);
  await qr.commitTransaction();
}
`;

test("KILN-E1TO-10 a start written inside an open transaction of the same receiver nests and the outermost owns every site", () => {
  const messages = [
    marker(NESTED_STARTS, "qr.startTransaction()", "startTransaction"),
    marker(NESTED_STARTS, "qr.startTransaction()", "startTransaction", { occurrence: 2 }),
    site(NESTED_STARTS, "qr.manager.save(a)", "save", { methodType: "write" }),
    site(NESTED_STARTS, "qr.manager.insert(a)", "insert", { methodType: "write" }),
    site(NESTED_STARTS, "qr.manager.delete(a)", "delete", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(NESTED_STARTS, messages).map(shape), [["S2", 2, ["a"], [3, 5, 7]]]);
});

test("KILN-E1TO-10 sequential transactions of one receiver stay separate units", () => {
  const messages = [
    marker(TWO_STARTS, "qr.startTransaction()", "startTransaction"),
    marker(TWO_STARTS, "qr.startTransaction()", "startTransaction", { occurrence: 2 }),
    site(TWO_STARTS, "qr.manager.save(a)", "save", { methodType: "write" }),
    site(TWO_STARTS, "qr.manager.save(a)", "save", { methodType: "write", occurrence: 2 }),
  ];
  assert.deepEqual(unitsOf(TWO_STARTS, messages).map(shape), [
    ["S2", 2, ["a"], [3]],
    ["S2", 6, ["a"], [7]],
  ]);
});

test("KILN-E1TO-10 a commit and a rollback after a closed transaction still belong to it, up to the next start", () => {
  const messages = [
    marker(START_COMMIT, "queryRunner.startTransaction()", "startTransaction"),
    site(START_COMMIT, "queryRunner.manager.insert(Rec, item)", "insert", { methodType: "write" }),
    site(START_COMMIT, "audit.save(items)", "save", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(START_COMMIT, messages).map(shape), [
    ["S2", 3, ["items"], [7]],
    ["S3", 13, ["items"], [13]],
  ]);
});

const DECORATED = `\
class Pay {
  @LazyTransaction({ isolation: "READ COMMITTED" })
  async decorated(a) {
    await repo.save(a);
  }

  @Cached()
  async plain(a) {
    await repo.save(a);
  }
}
`;

const DECORATED_METHOD = '@LazyTransaction({ isolation: "READ COMMITTED" })\n  async decorated(a) {\n    await repo.save(a);\n  }';
const PLAIN_METHOD = "@Cached()\n  async plain(a) {\n    await repo.save(a);\n  }";

test("KILN-E1TO-19 a method decorated @Transaction or @LazyTransaction is a legitimate marker, whether its span starts at the decorator or after it", () => {
  const atDecorator = marker(DECORATED, DECORATED_METHOD, "decorated");
  const afterDecorator = { ...atDecorator, fromLine: 2, fromColumn: 2 };
  for (const report of [atDecorator, afterDecorator]) {
    const { unmatchedMarkers } = enumerationOf(DECORATED, [report]);
    assert.deepEqual(unmatchedMarkers, [{ file: "src/db.ts", line: report.fromLine + 1, column: report.fromColumn }]);
  }
});

const shortenedEnd = (report) => ({ ...report, toColumn: report.toColumn - 1 });
const MALFORMED_REPORTS = [
  ["a start call whose end column is one short", TWO_STARTS, shortenedEnd(marker(TWO_STARTS, "qr.startTransaction()", "startTransaction"))],
  ["a method that is not decorated @Transaction", DECORATED, marker(DECORATED, PLAIN_METHOD, "plain")],
  ["a decorated method under another name", DECORATED, marker(DECORATED, DECORATED_METHOD, "other")],
  ["a span over no call and no method", DECORATED, marker(DECORATED, "repo", "transaction")],
];

for (const [name, source, report] of MALFORMED_REPORTS) {
  test(`KILN-E1TO-19 a transaction report over ${name} fails loudly instead of changing ownership`, () => {
    assert.throws(() => unitsOf(source, [report]), /no transaction-decorated method match the report at/);
  });
}
