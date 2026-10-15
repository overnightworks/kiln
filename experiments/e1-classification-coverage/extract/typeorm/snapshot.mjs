// The checkout the analyzer reads: exactly the manifest commit, before and after the installation.
// The installation may add `node_modules` and a lockfile the commit does not track, nothing else.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";

import { ExtractorError } from "./extractor_error.mjs";

const LOCKFILE_NAMES = new Set(["package-lock.json", "npm-shrinkwrap.json"]);
const UNTRACKED = "??";
const IGNORED = "!!";

/** @typedef {{ file: string, sha256: string }} GeneratedLock */

export function trackedFiles(applicationRoot) {
  return git(applicationRoot, "ls-files", "-z").split("\0").filter((file) => file !== "");
}

/** Lockfiles included: they drive which dependency versions, and so which types, the analyzer sees. */
export function verifyBeforeInstallation(row, applicationRoot) {
  const differences = differencesFromCommit(row, applicationRoot);
  if (differences.length > 0) throw differs(applicationRoot, "before the installation", differences);
}

/**
 * Every tracked file, every tracked lockfile among them, is byte-identical to the commit, and the
 * lockfiles the commit does not track are exactly the ones the installation recorded, unchanged.
 * @param {import("./lock_record.mjs").InstalledLock[]} record
 */
export function verifyInstalledCheckout(row, applicationRoot, record) {
  const problems = recordProblems(generatedLocks(row, applicationRoot), record);
  if (problems.length > 0) {
    throw new ExtractorError(`${applicationRoot} has generated lockfiles that differ from the installation record: ${problems.join("; ")}`);
  }
}

/**
 * Verifies the inputs, runs the installation, verifies that it left every tracked file as it was
 * and returns the lockfiles it generated, stamped with the clock at that verification.
 * @returns {import("./lock_record.mjs").InstalledLock[]}
 */
export function installDependencies(row, applicationRoot, install, now = () => new Date()) {
  verifyBeforeInstallation(row, applicationRoot);
  install();
  const locks = generatedLocks(row, applicationRoot);
  const installedAt = now().toISOString();
  return locks.map((lock) => ({ ...lock, installedAt }));
}

/** @returns {GeneratedLock[]} the untracked lockfiles present, ordered by file; any other difference fails */
function generatedLocks(row, applicationRoot) {
  const differences = differencesFromCommit(row, applicationRoot);
  const generated = differences.filter(isGeneratedLockfile);
  const strays = differences.filter((difference) => !generated.includes(difference));
  if (strays.length > 0) throw differs(applicationRoot, "after the installation", strays);
  return generated.map(({ file }) => ({ file, sha256: sha256Of(path.join(applicationRoot, file)) })).sort((first, second) => first.file.localeCompare(second.file));
}

function recordProblems(present, record) {
  const recorded = new Map(record.map((lock) => [lock.file, lock]));
  const presentFiles = new Set(present.map(({ file }) => file));
  const unrecorded = present.filter(({ file }) => !recorded.has(file)).map(({ file }) => `${file} has no installation record`);
  const changed = present.filter(({ file, sha256 }) => recorded.has(file) && recorded.get(file).sha256 !== sha256);
  const vanished = record.filter(({ file }) => !presentFiles.has(file)).map(({ file }) => `${file} was recorded but is gone`);
  return [...unrecorded, ...changed.map(({ file }) => `${file} is not the lock installed at ${recorded.get(file).installedAt}`), ...vanished];
}

function differencesFromCommit(row, applicationRoot) {
  const head = git(applicationRoot, "rev-parse", "HEAD").trim();
  if (head !== row.commitShaFull) {
    throw new ExtractorError(`${applicationRoot} is at ${head}, not the manifest commit ${row.commitShaFull}`);
  }
  const status = git(applicationRoot, "status", "--porcelain", "-z", "--no-renames", "--ignored", "--untracked-files=all");
  return status
    .split("\0")
    .filter((entry) => entry !== "")
    .map((entry) => ({ state: entry.slice(0, 2), file: entry.slice(3) }))
    .filter(({ file }) => !file.split("/").includes("node_modules"));
}

function isGeneratedLockfile({ state, file }) {
  return (state === UNTRACKED || state === IGNORED) && LOCKFILE_NAMES.has(path.posix.basename(file));
}

function sha256Of(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function differs(applicationRoot, moment, differences) {
  const files = differences.map(({ file }) => file).sort();
  return new ExtractorError(`${applicationRoot} differs from its commit ${moment}: ${files.join(", ")}`);
}

function git(directory, ...gitArguments) {
  return execFileSync("git", gitArguments, { cwd: directory, encoding: "utf8", maxBuffer: 1 << 28 });
}
