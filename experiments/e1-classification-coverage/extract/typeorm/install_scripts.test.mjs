import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";

import { installScriptsOf } from "./install_scripts.mjs";
import { renderInstallScripts } from "./rendering.mjs";

let root;

function writePackage(relative, manifest, files = []) {
  const directory = path.join(root, relative);
  mkdirSync(directory, { recursive: true });
  writeFileSync(path.join(directory, "package.json"), JSON.stringify(manifest));
  for (const file of files) writeFileSync(path.join(directory, file), "");
}

before(() => {
  root = mkdtempSync(path.join(tmpdir(), "kiln-e1to-scripts-"));
  writePackage(".", { name: "app", version: "1.0.0", scripts: { prepare: "husky install", test: "jest", postinstall: "patch-package" } });
  writePackage("node_modules/native", { name: "native", version: "2.0.0", scripts: { install: "node-gyp-build" } });
  writePackage("node_modules/@scope/generated", { name: "@scope/generated", version: "3.1.0", scripts: { preinstall: "a", postinstall: "b" } });
  writePackage("node_modules/native/node_modules/nested", { name: "nested", version: "0.1.0", scripts: { postinstall: "c" } });
  writePackage("node_modules/gyp-only", { name: "gyp-only", version: "4.0.0" }, ["binding.gyp"]);
  writePackage("node_modules/gyp-with-install", { name: "gyp-with-install", version: "4.1.0", scripts: { install: "custom" } }, ["binding.gyp"]);
  writePackage("node_modules/quiet", { name: "quiet", version: "5.0.0", scripts: { prepare: "tsc", test: "mocha", build: "tsc" } });
  writePackage("node_modules/no-scripts", { name: "no-scripts", version: "6.0.0" });
  writePackage("node_modules/.hidden", { name: "hidden", version: "7.0.0", scripts: { install: "x" } });
  writePackage("elsewhere/linked", { name: "linked", version: "8.0.0", scripts: { postinstall: "d" } });
  symlinkSync(path.join(root, "elsewhere/linked"), path.join(root, "node_modules/linked"));
});

after(() => rmSync(root, { recursive: true, force: true }));

test("KILN-E1TO-27 the application and each installed package declaring an install script are recorded, ordered by path", () => {
  assert.deepEqual(installScriptsOf(root), [
    { package: "app", version: "1.0.0", path: ".", scripts: ["postinstall", "prepare"] },
    { package: "@scope/generated", version: "3.1.0", path: "node_modules/@scope/generated", scripts: ["preinstall", "postinstall"] },
    { package: "gyp-only", version: "4.0.0", path: "node_modules/gyp-only", scripts: ["install"] },
    { package: "gyp-with-install", version: "4.1.0", path: "node_modules/gyp-with-install", scripts: ["install"] },
    { package: "linked", version: "8.0.0", path: "node_modules/linked", scripts: ["postinstall"] },
    { package: "native", version: "2.0.0", path: "node_modules/native", scripts: ["install"] },
    { package: "nested", version: "0.1.0", path: "node_modules/native/node_modules/nested", scripts: ["postinstall"] },
  ]);
});

test("KILN-E1TO-27 a tree without install scripts records nothing and the record is one JSON line per package", () => {
  const empty = mkdtempSync(path.join(tmpdir(), "kiln-e1to-scripts-empty-"));
  try {
    writeFileSync(path.join(empty, "package.json"), JSON.stringify({ name: "bare", scripts: { start: "node ." } }));
    assert.deepEqual(installScriptsOf(empty), []);
  } finally {
    rmSync(empty, { recursive: true, force: true });
  }
  const lines = renderInstallScripts(installScriptsOf(root)).trim().split("\n");
  assert.equal(lines.length, 7);
  assert.deepEqual(JSON.parse(lines[4]), { package: "linked", version: "8.0.0", path: "node_modules/linked", scripts: ["postinstall"] });
});

test("KILN-E1TO-27 an unreadable package manifest fails loudly naming the file", () => {
  const broken = mkdtempSync(path.join(tmpdir(), "kiln-e1to-scripts-broken-"));
  try {
    mkdirSync(path.join(broken, "node_modules/bad"), { recursive: true });
    writeFileSync(path.join(broken, "node_modules/bad/package.json"), "{not json");
    assert.throws(() => installScriptsOf(broken), /node_modules\/bad\/package\.json: cannot read the package manifest/);
  } finally {
    rmSync(broken, { recursive: true, force: true });
  }
});
