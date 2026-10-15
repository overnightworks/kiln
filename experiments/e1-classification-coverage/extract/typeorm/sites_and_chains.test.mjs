import assert from "node:assert/strict";
import { test } from "node:test";

import { enumerationOf, marker, shape, site, unitsOf } from "./unit_fixtures.mjs";

const TWO_SITES = "function view(key) {\n  return [repository.findOneBy({ id: key }), counter.count()];\n}\n";

test("KILN-E1TO-12 a statement holding several sites yields one S3 unit per site, positioned at the site", () => {
  const messages = [
    site(TWO_SITES, "repository.findOneBy({ id: key })", "findOneBy"),
    site(TWO_SITES, "counter.count()", "count"),
  ];
  const units = unitsOf(TWO_SITES, messages);
  assert.deepEqual(units.map(shape), [
    ["S3", 2, ["key"], [2]],
    ["S3", 2, [], [2]],
  ]);
  assert.deepEqual(units.map(({ column }) => column), [10, 45]);
});

const BRANCHES = `\
async function view(flag, rows) {
  if (flag) {
    await repo.find();
  } else if (await repo.exist()) {
    await repo.clear();
  }
  const row = flag ? await repo.findOne() : null;
  for (const item of rows) {
    await repo.save(item);
  }
  await Promise.all(rows.map((r) => repo.remove(r)));
}
`;

test("KILN-E1TO-13 a site in a branch, ternary, loop or callback is one unit as written", () => {
  const sites = [
    ["repo.find()", "find"], ["repo.exist()", "exist"], ["repo.clear()", "clear"], ["repo.findOne()", "findOne"],
    ["repo.save(item)", "save"], ["repo.remove(r)", "remove"],
  ]; // fmt: skip
  const units = unitsOf(BRANCHES, sites.map(([needle, method]) => site(BRANCHES, needle, method)));
  assert.deepEqual(units.map((unit) => unit.line), [3, 4, 5, 7, 9, 11]);
});

const PARAMETERS = `\
class Views {
  async post(request, flag = null, { name, ...rest }, ...others) {
    const early = await load(request);
    const { picked } = early;
    const done = await repo.save(request, early, picked, late, name, rest, others, done, outerName, this.hidden.flag);
    const late = 5;
    for (const item of request.items) {
      await repo.remove(item);
    }
    try {
      await repo.insert(flag);
    } catch (caught) {
      await repo.update(caught);
    }
  }
}
const topLevel = repo.count(globalName);
`;

test("KILN-E1TO-14 S3 parameters are the names the call reads that are parameters, then locals bound before it", () => {
  const messages = [
    site(PARAMETERS, "repo.save(request, early, picked, late, name, rest, others, done, outerName, this.hidden.flag)", "save"),
    site(PARAMETERS, "repo.remove(item)", "remove"),
    site(PARAMETERS, "repo.insert(flag)", "insert"),
    site(PARAMETERS, "repo.update(caught)", "update"),
    site(PARAMETERS, "repo.count(globalName)", "count"),
  ];
  assert.deepEqual(unitsOf(PARAMETERS, messages).map(({ parameters }) => parameters), [
    ["request", "name", "rest", "others", "early", "picked"],
    ["item"],
    ["flag"],
    ["caught"],
    [],
  ]);
});

const CHAIN = `\
async function insertOnce(recorded: Recorded, unrelated: number) {
  const connection = await getConnection();
  const queryBuilder = connection.createQueryBuilder().insert().into(Recorded).values(recorded);
  return queryBuilder.execute();
}
`;

const CHAIN_NEEDLE = "connection.createQueryBuilder().insert().into(Recorded).values(recorded)";

function chainMessages(source = CHAIN) {
  return [
    site(source, "connection.createQueryBuilder()", "createQueryBuilder", { methodType: "other" }),
    site(source, "connection.createQueryBuilder().insert()", "insert", { methodType: "write" }),
  ];
}

test("KILN-E1TO-15 a query-builder chain is one S5 unit at createQueryBuilder() absorbing its reported sites", () => {
  assert.ok(CHAIN.includes(CHAIN_NEEDLE));
  assert.deepEqual(unitsOf(CHAIN, chainMessages()).map(shape), [["S5", 3, ["recorded", "connection"], [3, 3]]]);
});

const MULTILINE = `\
async function page(params, db) {
  const find = db
    .createQueryBuilder()
    .offset(params.page)
    .update()
    .orderBy("createTime");
  return [await find.getMany(), await find.getCount()];
}
`;

test("KILN-E1TO-15 a chain spread over lines is one unit, and terminals the analyzer does not report add none", () => {
  const messages = [
    site(MULTILINE, "db\n    .createQueryBuilder()", "createQueryBuilder", { methodType: "other" }),
    site(MULTILINE, "db\n    .createQueryBuilder()\n    .offset(params.page)\n    .update()", "update", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(MULTILINE, messages).map(shape), [["S5", 2, ["params", "db"], [2, 2]]]);
});

test("KILN-E1TO-16 a chain inside an explicit transaction contributes no unit and its sites are absorbed", () => {
  const source = "async function run(a) {\n  await dataSource.transaction(async (m) => m.createQueryBuilder().update().set(a).execute());\n  await repo.createQueryBuilder().delete().execute();\n}\n";
  const messages = [
    marker(source, "dataSource.transaction(async (m) => m.createQueryBuilder().update().set(a).execute())", "transaction"),
    site(source, "m.createQueryBuilder()", "createQueryBuilder", { methodType: "other" }),
    site(source, "m.createQueryBuilder().update()", "update", { methodType: "write" }),
    site(source, "repo.createQueryBuilder()", "createQueryBuilder", { methodType: "other" }),
    site(source, "repo.createQueryBuilder().delete()", "delete", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(source, messages).map(shape), [
    ["S2", 2, ["a"], [2, 2]],
    ["S5", 3, [], [3, 3]],
  ]);
});

test("KILN-E1TO-17 a site on a variable holding a builder, or in a chain argument, is not part of the chain", () => {
  const source = "function run(a) {\n  const qb = repo.createQueryBuilder();\n  qb.update();\n  repo.createQueryBuilder().where(() => other.find(a)).delete();\n}\n";
  const messages = [
    site(source, "repo.createQueryBuilder()", "createQueryBuilder", { methodType: "other" }),
    site(source, "qb.update()", "update", { methodType: "write" }),
    site(source, "repo.createQueryBuilder()", "createQueryBuilder", { methodType: "other", occurrence: 2 }),
    site(source, "other.find(a)", "find"),
    site(source, "repo.createQueryBuilder().where(() => other.find(a)).delete()", "delete", { methodType: "write" }),
  ];
  assert.deepEqual(unitsOf(source, messages).map(shape), [
    ["S5", 2, [], [2]],
    ["S3", 3, ["qb"], [3]],
    ["S5", 4, ["a"], [4, 4]],
    ["S3", 4, [], [4]],
  ]);
});

test("KILN-E1TO-18 a call the analyzer does not report yields no unit", () => {
  const source = "function run(a) {\n  return this.getRepository(a).getMany();\n}\n";
  assert.deepEqual(unitsOf(source, []), []);
});

const AMBIGUOUS_SOURCE = "function run(a) {\n  return repo.find(a);\n}\n";

test("KILN-E1TO-19 a report that names no call, a missing file or a position outside the text fails loudly", () => {
  const found = site(AMBIGUOUS_SOURCE, "repo.find(a)", "find");
  const wrongMethod = { ...found, content: { ...found.content, name: "findOne" } };
  const shifted = { ...found, fromColumn: found.fromColumn + 1 };
  const pastLastLine = { ...found, fromLine: 9, toLine: 9 };
  const pastLineEnd = { ...found, fromColumn: 99 };
  const negativeColumn = { ...found, fromColumn: -1 };
  for (const broken of [wrongMethod, shifted, pastLastLine, pastLineEnd, negativeColumn]) {
    assert.throws(() => enumerationOf(AMBIGUOUS_SOURCE, [broken]), /src\/db\.ts/);
  }
  assert.throws(() => enumerationOf(AMBIGUOUS_SOURCE, [{ ...found, filePath: "src/other.ts" }]), /src\/other\.ts: no source text/);
});

test("KILN-E1TO-19 two reports of one call fail loudly instead of being merged", () => {
  const found = site(AMBIGUOUS_SOURCE, "repo.find(a)", "find");
  assert.throws(() => enumerationOf(AMBIGUOUS_SOURCE, [found, found]), /two reports/);
});

test("KILN-E1TO-20 no unit is removed for S4 and each site carries its receiver evidence", () => {
  const source = "function run(a, request) {\n  repo.find(a);\n  request.body.get(a);\n  untyped.save(a);\n}\n";
  const messages = [
    site(source, "repo.find(a)", "find", { receiver: "repo", types: ["Repository<Item>", "Base"] }),
    site(source, "request.body.get(a)", "get", { receiver: "body", types: ["any"] }),
    site(source, "untyped.save(a)", "save", { receiver: "untyped", types: [] }),
  ];
  const enumeration = enumerationOf(source, messages);
  assert.equal(enumeration.units.length, 3);
  assert.deepEqual(
    enumeration.receiverEvidence.map(({ method, receiver, receiver_types, receiver_resolved }) => [method, receiver, receiver_types, receiver_resolved]),
    [["find", "repo", ["Repository<Item>", "Base"], true], ["get", "body", ["any"], false], ["save", "untyped", [], false]],
  );
});
