import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, afterEach, before, beforeEach, describe, test } from "node:test";

import { lockRecordFile, readLockRecord, writeLockRecord } from "./lock_record.mjs";
import { installDependencies, verifyBeforeInstallation, verifyInstalledCheckout } from "./snapshot.mjs";

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

const TRACKED_LOCK = '{"lockfileVersion": 2}\n';

before(() => {
  root = mkdtempSync(path.join(tmpdir(), "kiln-e1to-"));
  git("init", "--quiet");
  write("tsconfig.json", "{}\n");
  write("src/a.ts");
  write("client/tsconfig.json", "{}\n");
  write("tsconfig.base.json", '{"compilerOptions": {"strict": true}}\n');
  write("package-lock.json", TRACKED_LOCK);
  write("yarn.lock", "# yarn\n");
  write(".gitignore", "node_modules/\nignored.log\n");
  git("add", ".");
  git("commit", "--quiet", "-m", "pinned");
  row = { commitShaFull: git("rev-parse", "HEAD") };
});

after(() => rmSync(root, { recursive: true, force: true }));

afterEach(() => {
  git("reset", "--quiet", "--hard", row.commitShaFull);
  git("clean", "--quiet", "--force", "-x");
});

test("KILN-E1TO-05 a pristine checkout is accepted before the installation and records no generated lock", () => {
  verifyBeforeInstallation(row, root);
  verifyInstalledCheckout(row, root, []);
});

const CHANGES = [
  ["a modified tracked source file", () => write("src/a.ts", "export const changed = 1;\n"), "src/a.ts"],
  ["an untracked source file", () => write("src/b.ts"), "src/b.ts"],
  ["a modified tsconfig", () => write("tsconfig.json", '{"include": []}\n'), "tsconfig.json"],
  ["a modified tsconfig the project extends, outside the selected directory", () => write("tsconfig.base.json", '{"exclude": ["src"]}\n'), "tsconfig.base.json"],
  ["an untracked file outside the selected directory", () => write("client/untracked.ts"), "client/untracked.ts"],
  ["an ignored file outside node_modules", () => write("ignored.log"), "ignored.log"],
  ["a deleted tracked configuration file", () => rmSync(path.join(root, "tsconfig.base.json")), "tsconfig.base.json"],
  ["a tracked lockfile of another package manager that changed", () => write("yarn.lock", "# changed\n"), "yarn.lock"],
];

for (const [name, change, file] of CHANGES) {
  test(`KILN-E1TO-05 ${name} is rejected naming the file, before and after the installation`, () => {
    change();
    assert.throws(() => verifyBeforeInstallation(row, root), (error) => error.message.includes(file));
    assert.throws(() => verifyInstalledCheckout(row, root, []), (error) => error.message.includes(file));
  });
}

test("KILN-E1TO-05 another commit is rejected", () => {
  assert.throws(() => verifyBeforeInstallation({ commitShaFull: "0".repeat(40) }, root), /not the manifest commit/);
});

test("KILN-E1TO-28 a tracked lockfile modified before the installation fails the run naming it", () => {
  write("package-lock.json", '{"lockfileVersion": 3}\n');
  let installed = false;
  assert.throws(() => installDependencies(row, root, () => (installed = true)), /before the installation: package-lock\.json/);
  assert.equal(installed, false);
});

test("KILN-E1TO-28 a tracked lockfile the installer rewrites fails the run naming it", () => {
  const rewritingInstaller = () => write("package-lock.json", '{"lockfileVersion": 3}\n');
  assert.throws(() => installDependencies(row, root, rewritingInstaller), /after the installation: package-lock\.json/);
});

test("KILN-E1TO-28 a tracked lockfile the installer leaves byte-identical passes, node_modules included", () => {
  const untouchingInstaller = () => write("node_modules/dep/index.d.ts");
  assert.deepEqual(installDependencies(row, root, untouchingInstaller), []);
});

test("KILN-E1TO-28 a lockfile left over from an earlier installation is rejected before the next one", () => {
  git("rm", "--quiet", "package-lock.json");
  git("commit", "--quiet", "-m", "no lock");
  const withoutLock = { commitShaFull: git("rev-parse", "HEAD") };
  write("package-lock.json", "{}\n");
  assert.throws(() => verifyBeforeInstallation(withoutLock, root), /package-lock\.json/);
});

const LOCK_HASH = "f66e66d185a139532477c54bd6ad0f4c3cd04cb7803a3a2f23bccfa6e07df037";
const INSTALLED_AT = "2026-10-16T12:00:00.000Z";
const clockAtVerification = () => new Date(INSTALLED_AT);

function commitWithoutLock() {
  git("rm", "--quiet", "package-lock.json");
  git("commit", "--quiet", "-m", "no lock");
  return { commitShaFull: git("rev-parse", "HEAD") };
}

const generateLock = (text = '{"lockfileVersion": 3}\n') => () => write("package-lock.json", text);

test("KILN-E1TO-29 an application without a tracked lockfile gets the generated lock recorded with its hash and the clock at verification, not the file's modification time", () => {
  const withoutLock = commitWithoutLock();
  const generatingInstaller = () => {
    generateLock()();
    utimesSync(path.join(root, "package-lock.json"), new Date("2001-01-01T00:00:00Z"), new Date("2001-01-01T00:00:00Z"));
  };
  assert.deepEqual(installDependencies(withoutLock, root, generatingInstaller, clockAtVerification), [
    { file: "package-lock.json", sha256: LOCK_HASH, installedAt: INSTALLED_AT },
  ]);
});

test("KILN-E1TO-29 a generated lockfile that the application's ignore rules hide is recorded too", () => {
  git("rm", "--quiet", "package-lock.json");
  write(".gitignore", "node_modules/\npackage-lock.json\n");
  git("add", ".gitignore");
  git("commit", "--quiet", "-m", "ignore the lock");
  const ignoringLock = { commitShaFull: git("rev-parse", "HEAD") };
  const [lock] = installDependencies(ignoringLock, root, generateLock());
  assert.equal(lock.file, "package-lock.json");
});

describe("KILN-E1TO-29 the units step checks the lockfile against the installation record", () => {
  let outputDirectory;
  let withoutLock;
  let recordFile;

  beforeEach(() => {
    outputDirectory = mkdtempSync(path.join(tmpdir(), "kiln-e1to-out-"));
    withoutLock = { ...commitWithoutLock(), directoryName: "acme__shop" };
    recordFile = lockRecordFile(outputDirectory, withoutLock);
  });

  afterEach(() => rmSync(outputDirectory, { recursive: true, force: true }));

  const installAndRecord = () => writeLockRecord(recordFile, installDependencies(withoutLock, root, generateLock(), clockAtVerification));

  test("the lock that was installed passes, read back from the record file", () => {
    installAndRecord();
    assert.deepEqual(readLockRecord(recordFile), [{ file: "package-lock.json", sha256: LOCK_HASH, installedAt: INSTALLED_AT }]);
    verifyInstalledCheckout(withoutLock, root, readLockRecord(recordFile));
  });

  test("a lock substituted after the installation fails naming the file", () => {
    installAndRecord();
    generateLock('{"lockfileVersion": 3, "substituted": true}\n')();
    assert.throws(() => verifyInstalledCheckout(withoutLock, root, readLockRecord(recordFile)), /package-lock\.json is not the lock installed at 2026-10-16T12:00:00\.000Z/);
  });

  test("a lock that appeared without being recorded fails", () => {
    installAndRecord();
    write("npm-shrinkwrap.json", "{}\n");
    assert.throws(() => verifyInstalledCheckout(withoutLock, root, readLockRecord(recordFile)), /npm-shrinkwrap\.json has no installation record/);
  });

  test("a recorded lock that is gone fails", () => {
    installAndRecord();
    rmSync(path.join(root, "package-lock.json"));
    assert.throws(() => verifyInstalledCheckout(withoutLock, root, readLockRecord(recordFile)), /package-lock\.json was recorded but is gone/);
  });

  test("a missing record fails before anything is analyzed", () => {
    generateLock()();
    assert.throws(() => readLockRecord(recordFile), /no installation record at .*acme__shop\.generated-locks\.jsonl/);
  });

  test("a malformed record line fails", () => {
    writeFileSync(recordFile, '{"file":"package-lock.json","sha256":"abc","installed_at":"never"}\n');
    assert.throws(() => readLockRecord(recordFile), /malformed installation record/);
  });
});

test("KILN-E1TO-29 an application with a tracked lockfile has an empty record that passes", () => {
  const record = installDependencies(row, root, () => write("node_modules/dep/index.d.ts"));
  assert.deepEqual(record, []);
  verifyInstalledCheckout(row, root, record);
});
