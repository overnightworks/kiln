import assert from "node:assert/strict";
import { test } from "node:test";

import { assertOnlyFilesOutsideTheProject } from "./analyzer_errors.mjs";
import { parseAnalyzerJson } from "./analyzer_report.mjs";
import { analyzerArguments, isFallbackExcluded, selectionOf } from "./file_selection.mjs";
import { entityMessage, FILE, marker, reportOf, site, WHOLE_APPLICATION } from "./unit_fixtures.mjs";

const SOURCE = "function run(a) {\n  repo.find(a);\n  repo.save(a);\n}\n";

test("KILN-E1TO-01 only read, write and other method messages are sites; entities and markers are not", () => {
  const report = reportOf([
    site(SOURCE, "repo.find(a)", "find"),
    site(SOURCE, "repo.save(a)", "save", { methodType: "write" }),
    site(SOURCE, "repo.find(a)", "createQueryBuilder", { methodType: "other" }),
    marker(SOURCE, "repo.find(a)", "transaction"),
    entityMessage(),
  ]);
  assert.deepEqual(report.sites.map(({ method }) => method), ["find", "save", "createQueryBuilder"]);
  assert.deepEqual(report.markers.map(({ name }) => name), ["transaction"]);
});

test("KILN-E1TO-01 a site keeps the analyzer's position with the line counted from 1", () => {
  const [found] = reportOf([site(SOURCE, "repo.save(a)", "save", { methodType: "write" })]).sites;
  assert.deepEqual([found.start, found.end], [{ line: 3, column: 2 }, { line: 3, column: 14 }]);
});

test("KILN-E1TO-01 an unknown message type or method type fails loudly", () => {
  const unknownType = { ...entityMessage(), content: { type: "mystery" } };
  const unknownMethodType = site(SOURCE, "repo.find(a)", "find", { methodType: "astral" });
  for (const message of [unknownType, unknownMethodType]) {
    assert.throws(() => reportOf([message]), /unknown (message|method) type/);
  }
});

const TRACKED = [
  ["the shortest path wins, not the shallowest", ["client/tsconfig.json", "src/tsconfig.json", "package.json"], "src"],
  ["a tsconfig at the root is the root", ["src/tsconfig.json", "tsconfig.json"], ""],
  ["equal lengths are ordered lexically", ["bbb/tsconfig.json", "aaa/tsconfig.json"], "aaa"],
  ["only files named tsconfig.json count", ["a/tsconfig.build.json", "b/mytsconfig.json", "web/tsconfig.json"], "web"],
];

for (const [name, tracked, directory] of TRACKED) {
  test(`KILN-E1TO-02 ${name}`, () => {
    assert.deepEqual(selectionOf(tracked), { directory, fallback: false });
  });
}

test("KILN-E1TO-02 the frozen command passes the project directory and no include glob", () => {
  assert.deepEqual(analyzerArguments({ directory: "src", fallback: false }, "/corpus/app", "/out/messages.json"), [
    "/corpus/app/src",
    "--output",
    "/out/messages.json",
  ]);
});

test("KILN-E1TO-03 without a tsconfig the fallback command includes every .ts file of the root", () => {
  const selection = selectionOf(["src/index.ts", "package.json"]);
  assert.deepEqual(selection, { directory: "", fallback: true });
  assert.deepEqual(analyzerArguments(selection, "/corpus/app", "/out/m.json"), ["/corpus/app", "--include", "**/*.ts", "--output", "/out/m.json"]);
});

const FALLBACK_PATHS = [
  ["src/a.ts", false], ["node_modules/x/a.ts", true], ["dist/a.ts", true], ["packages/build/a.ts", true],
  ["test/a.ts", true], ["src/tests/a.ts", true], ["src/__tests__/a.ts", true], ["src/a.spec.ts", true],
  ["src/a.test.ts", true], ["src/contest.ts", false], ["src/distribution/a.ts", false], ["src/specs.ts", false],
]; // fmt: skip

for (const [relativePath, discarded] of FALLBACK_PATHS) {
  test(`KILN-E1TO-03 the fallback ${discarded ? "discards" : "keeps"} ${relativePath}`, () => {
    assert.equal(isFallbackExcluded(relativePath), discarded);
    const report = parseAnalyzerJson(JSON.stringify({ messages: [{ ...entityMessage(), filePath: relativePath, content: { type: "method", name: "find", methodType: "read", object: "r", objectTypes: [] } }] }), { directory: "", fallback: true });
    assert.equal(report.sites.length, discarded ? 0 : 1);
  });
}

test("KILN-E1TO-04 reports are located relative to the application root and reports leaving it are dropped", () => {
  const inside = site(SOURCE, "repo.find(a)", "find", { file: "model/db.ts" });
  const outside = site(SOURCE, "repo.find(a)", "find", { file: "../shared/db.ts" });
  const report = reportOf([inside, outside], { directory: "src", fallback: false });
  assert.deepEqual(report.sites.map(({ file }) => file), ["src/model/db.ts"]);
  assert.deepEqual(reportOf([inside], WHOLE_APPLICATION).sites.map(({ file }) => file), ["model/db.ts"]);
  assert.deepEqual(reportOf([site(SOURCE, "repo.find(a)", "find", { file: "/etc/db.ts" })]).sites, []);
});

const PROJECT_FILE_ERROR = `Parsing error: ESLint was configured to run on \`<tsconfigRootDir>/${FILE}\` using \`parserOptions.project\`:
- <tsconfigRootDir>/tsconfig.json
However, none of those TSConfigs include this file. Either:
- Create a new TSConfig that includes this file and include it in your parserOptions.project
`;

test("KILN-E1TO-06 error output naming only files outside the project is accepted", () => {
  assertOnlyFilesOutsideTheProject("");
  assertOnlyFilesOutsideTheProject(PROJECT_FILE_ERROR + PROJECT_FILE_ERROR);
});

test("KILN-E1TO-06 any other error output fails loudly, wherever it sits", () => {
  const syntaxError = "Parsing error: ';' expected.\n";
  for (const output of [syntaxError, `${PROJECT_FILE_ERROR}${syntaxError}`, `TypeError: boom\n${PROJECT_FILE_ERROR}`]) {
    assert.throws(() => assertOnlyFilesOutsideTheProject(output), /the analyzer reported errors/);
  }
});
