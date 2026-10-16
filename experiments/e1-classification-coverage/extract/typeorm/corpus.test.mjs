import assert from "node:assert/strict";
import { test } from "node:test";

import {
  MANIFEST_PATH, analyzerInstallCommands, applicationRow, corpusDirectory, dependencyCommands, fetchCommands, readManifest,
} from "./corpus.mjs"; // fmt: skip

const REPOSITORY_ROOT = "/work/kiln";
const CORPUS = "/data/corpus";
const manifest = readManifest(MANIFEST_PATH);
const EPGSTATION = "l3tnun/EPGStation";

test("KILN-E1TO-26 the manifest holds the 32 TypeORM applications and rejects a Django one", () => {
  const typeorm = [...manifest.values()].filter((row) => row.orm === "TypeORM");
  assert.equal(typeorm.length, 32);
  assert.throws(() => applicationRow(manifest, "zulip/zulip"), /not a TypeORM application/);
  assert.throws(() => applicationRow(manifest, "nobody/nothing"), /not a TypeORM application/);
});

test("KILN-E1TO-26 fetching clones the manifest commit into the corpus directory", () => {
  const row = applicationRow(manifest, EPGSTATION);
  const [init, fetch, checkout] = fetchCommands(row, CORPUS);
  assert.deepEqual(init.arguments, ["init", "--quiet", "/data/corpus/l3tnun__EPGStation"]);
  assert.deepEqual(fetch.arguments, ["fetch", "--depth", "1", row.repositoryUrl, "c0e201b83231dfed73e5bea1b2ec54866fcfbbba"]);
  assert.equal(fetch.directory, "/data/corpus/l3tnun__EPGStation");
  assert.deepEqual(checkout.arguments, ["checkout", "--quiet", "FETCH_HEAD"]);
});

test("KILN-E1TO-26 the analyzer environment is the pinned install of the rule, apart from the applications", () => {
  const [install] = analyzerInstallCommands(CORPUS);
  assert.deepEqual([install.program, ...install.arguments], ["npm", "install", "@ctring/splinter-eslint@1.0.9", "typescript@5.4.3"]);
  assert.equal(install.directory, "/data/corpus/splinter-eslint-tools");
  const [dependencies] = dependencyCommands(applicationRow(manifest, EPGSTATION), CORPUS);
  assert.equal(dependencies.directory, "/data/corpus/l3tnun__EPGStation");
});

test("KILN-E1TO-27 the application's dependencies are installed without running install scripts", () => {
  const [dependencies] = dependencyCommands(applicationRow(manifest, EPGSTATION), CORPUS);
  assert.deepEqual([dependencies.program, ...dependencies.arguments], ["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund"]);
});

test("KILN-E1TO-26 a missing corpus setting or one inside the repository is rejected", () => {
  assert.equal(corpusDirectory("/data/corpus", REPOSITORY_ROOT), CORPUS);
  assert.throws(() => corpusDirectory(undefined, REPOSITORY_ROOT), /set the corpus directory/);
  assert.throws(() => corpusDirectory("/work/kiln/corpus", REPOSITORY_ROOT), /inside the repository/);
  assert.throws(() => corpusDirectory("/work/kiln", REPOSITORY_ROOT), /inside the repository/);
  assert.equal(corpusDirectory("/work/kiln-corpus", REPOSITORY_ROOT), "/work/kiln-corpus");
});
