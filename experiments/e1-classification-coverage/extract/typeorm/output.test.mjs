import assert from "node:assert/strict";
import { test } from "node:test";

import { renderReceiverEvidence, renderUnits, renderUnmatchedMarkers } from "./rendering.mjs";
import { APPLICATION, enumerationOf, FILE, marker, site } from "./unit_fixtures.mjs";

const SOURCE = `\
async function run(a) {
  await repo.find(a);
  await repo.createQueryBuilder().update().set(a).execute();
  await dataSource.transaction(async (m) => m.save(a));
  await repo.find(a).then(() => repo.save(a));
}
`;

function messagesInAnyOrder() {
  return [
    site(SOURCE, "repo.find(a)", "find"),
    site(SOURCE, "repo.createQueryBuilder().update()", "update", { methodType: "write" }),
    site(SOURCE, "repo.createQueryBuilder()", "createQueryBuilder", { methodType: "other" }),
    marker(SOURCE, "dataSource.transaction(async (m) => m.save(a))", "transaction"),
    site(SOURCE, "m.save(a)", "save", { methodType: "write" }),
    site(SOURCE, "repo.find(a)", "find", { occurrence: 2 }),
    site(SOURCE, "repo.save(a)", "save", { methodType: "write" }),
  ];
}

test("KILN-E1TO-21 a unit is one JSON line with exactly the fields the reader deserializes", () => {
  const [first] = renderUnits(enumerationOf(SOURCE, messagesInAnyOrder()).units).split("\n");
  assert.deepEqual(JSON.parse(first), {
    subject: { application: APPLICATION },
    file: FILE,
    line: 2,
    column: 8,
    rule: "S3",
    parameters: ["a"],
    absorbed_sites: [{ file: FILE, line: 2, column: 8 }],
  });
  assert.deepEqual(Object.keys(JSON.parse(first)), ["subject", "file", "line", "column", "rule", "parameters", "absorbed_sites"]);
});

test("KILN-E1TO-22 units are ordered by position and rule and a rerun is byte-identical whatever the message order", () => {
  const messages = messagesInAnyOrder();
  const render = (ordered) => renderUnits(enumerationOf(SOURCE, ordered).units);
  const reference = render(messages);
  const rules = reference.trim().split("\n").map((line) => JSON.parse(line)).map(({ line, rule }) => `${line}${rule}`);
  assert.deepEqual(rules, ["2S3", "3S5", "4S2", "5S3", "5S3"]);
  for (const ordered of [[...messages].reverse(), [...messages.slice(3), ...messages.slice(0, 3)]]) {
    assert.equal(render(ordered), reference);
  }
});

test("KILN-E1TO-09 the side outputs are one JSON line per record", () => {
  const enumeration = enumerationOf(SOURCE, [site(SOURCE, "repo.find(a)", "find")]);
  assert.equal(renderUnmatchedMarkers([{ file: FILE, line: 3, column: 4 }]), `{"file":"${FILE}","line":3,"column":4}\n`);
  assert.equal(renderReceiverEvidence(enumeration.receiverEvidence).split("\n").length, 2);
  assert.equal(renderUnits([]), "");
});
