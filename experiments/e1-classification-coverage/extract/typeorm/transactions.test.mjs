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
