import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";

import {
  MANIFEST_PATH, analyzerInstallCommands, applicationRow, corpusDirectory, dependencyCommands, fetchCommands, readManifest,
  verifySnapshot,
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

test("KILN-E1TO-26 a missing corpus setting or one inside the repository is rejected", () => {
  assert.equal(corpusDirectory("/data/corpus", REPOSITORY_ROOT), CORPUS);
  assert.throws(() => corpusDirectory(undefined, REPOSITORY_ROOT), /set the corpus directory/);
  assert.throws(() => corpusDirectory("/work/kiln/corpus", REPOSITORY_ROOT), /inside the repository/);
  assert.throws(() => corpusDirectory("/work/kiln", REPOSITORY_ROOT), /inside the repository/);
  assert.equal(corpusDirectory("/work/kiln-corpus", REPOSITORY_ROOT), "/work/kiln-corpus");
});

let root;
let row;

const git = (...args) =>
  execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@example.com", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@example.com" },
  }).trim();

const write = (relative, text = "export {};\n") => {
  mkdirSync(path.dirname(path.join(root, relative)), { recursive: true });
  writeFileSync(path.join(root, relative), text);
};

before(() => {
  root = mkdtempSync(path.join(tmpdir(), "kiln-e1to-"));
  git("init", "--quiet");
  write("tsconfig.json", "{}\n");
  write("src/a.ts");
  write("client/tsconfig.json", "{}\n");
  write("tsconfig.base.json", '{"compilerOptions": {"strict": true}}\n');
  write("package-lock.json", "{}\n");
  write(".gitignore", "node_modules/\nignored.log\n");
  git("add", ".");
  git("commit", "--quiet", "-m", "pinned");
  row = { ...applicationRow(manifest, EPGSTATION), commitShaFull: git("rev-parse", "HEAD") };
});

after(() => rmSync(root, { recursive: true, force: true }));

test("KILN-E1TO-05 a checkout at the manifest commit with only the installation added is accepted", () => {
  write("node_modules/dep/index.d.ts");
  write("package-lock.json", '{"lockfileVersion": 3}\n');
  verifySnapshot(row, root);
  rmSync(path.join(root, "node_modules"), { recursive: true });
  git("checkout", "--", ".");
});

const CHANGES = [
  ["a modified tracked source file", () => write("src/a.ts", "export const changed = 1;\n"), "src/a.ts"],
  ["an untracked source file", () => write("src/b.ts"), "src/b.ts"],
  ["a modified tsconfig", () => write("tsconfig.json", '{"include": []}\n'), "tsconfig.json"],
  ["a modified tsconfig the project extends, outside the selected directory", () => write("tsconfig.base.json", '{"exclude": ["src"]}\n'), "tsconfig.base.json"],
  ["an untracked file outside the selected directory", () => write("client/untracked.ts"), "client/untracked.ts"],
  ["an ignored file outside node_modules", () => write("ignored.log"), "ignored.log"],
  ["a deleted tracked configuration file", () => rmSync(path.join(root, "tsconfig.base.json")), "tsconfig.base.json"],
];

for (const [name, change, file] of CHANGES) {
  test(`KILN-E1TO-05 ${name} is rejected naming the file`, () => {
    change();
    assert.throws(() => verifySnapshot(row, root), (error) => error.message.includes(file));
    git("checkout", "--", ".");
    git("clean", "--quiet", "--force", "-x", "--exclude=node_modules");
  });
}

test("KILN-E1TO-05 another commit is rejected", () => {
  assert.throws(() => verifySnapshot({ ...row, commitShaFull: "0".repeat(40) }, root), /not the manifest commit/);
});
