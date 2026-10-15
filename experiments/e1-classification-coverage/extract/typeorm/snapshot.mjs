// The checkout the analyzer reads: exactly the manifest commit, before and after the installation.
// The installation may add `node_modules` and a lockfile the commit does not track, nothing else.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, statSync } from "node:fs";
import path from "node:path";

import { ExtractorError } from "./extractor_error.mjs";

const LOCKFILE_NAMES = new Set(["package-lock.json", "npm-shrinkwrap.json"]);
const UNTRACKED = "??";
const IGNORED = "!!";

/** @typedef {{ file: string, sha256: string, installedAt: string }} GeneratedLock */

export function trackedFiles(applicationRoot) {
  return git(applicationRoot, "ls-files", "-z").split("\0").filter((file) => file !== "");
}

/** Lockfiles included: they drive which dependency versions, and so which types, the analyzer sees. */
export function verifyBeforeInstallation(row, applicationRoot) {
  const differences = differencesFromCommit(row, applicationRoot);
  if (differences.length > 0) throw differs(applicationRoot, "before the installation", differences);
}

/**
 * Every tracked file, every tracked lockfile among them, is byte-identical to the commit; a lockfile
 * the commit does not track and the installation generated is returned as evidence of what resolved.
 * @returns {GeneratedLock[]}
 */
export function verifyInstalledCheckout(row, applicationRoot) {
  const differences = differencesFromCommit(row, applicationRoot);
  const generated = differences.filter(isGeneratedLockfile);
  const strays = differences.filter((difference) => !generated.includes(difference));
  if (strays.length > 0) throw differs(applicationRoot, "after the installation", strays);
  return generated.map(({ file }) => generatedLock(applicationRoot, file)).sort((first, second) => first.file.localeCompare(second.file));
}

/** Verifies the inputs, runs the installation, then verifies that it left every tracked file as it was. */
export function installDependencies(row, applicationRoot, install) {
  verifyBeforeInstallation(row, applicationRoot);
  install();
  return verifyInstalledCheckout(row, applicationRoot);
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

function generatedLock(applicationRoot, file) {
  const absolute = path.join(applicationRoot, file);
  const sha256 = createHash("sha256").update(readFileSync(absolute)).digest("hex");
  return { file, sha256, installedAt: statSync(absolute).mtime.toISOString() };
}

function differs(applicationRoot, moment, differences) {
  const files = differences.map(({ file }) => file).sort();
  return new ExtractorError(`${applicationRoot} differs from its commit ${moment}: ${files.join(", ")}`);
}

function git(directory, ...gitArguments) {
  return execFileSync("git", gitArguments, { cwd: directory, encoding: "utf8", maxBuffer: 1 << 28 });
}
