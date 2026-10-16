// The installation record: the lockfiles `npm install` generated for an application whose commit
// tracks none, with their hash and the instant the installation was verified. It lives next to the
// outputs; the `units` step only reads it, to check the lockfile it analyzes is the one installed.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

import { ExtractorError } from "./extractor_error.mjs";

const SHA256_HEX = /^[0-9a-f]{64}$/;

/** @typedef {{ file: string, sha256: string, installedAt: string }} InstalledLock */

export const lockRecordFile = (outputDirectory, row) => path.join(outputDirectory, `${row.directoryName}.generated-locks.jsonl`);

/** @param {InstalledLock[]} locks */
export function writeLockRecord(recordFile, locks) {
  const lines = locks.map(({ file, sha256, installedAt }) => `${JSON.stringify({ file, sha256, installed_at: installedAt })}\n`);
  writeFileSync(recordFile, lines.join(""));
}

/** @returns {InstalledLock[]} */
export function readLockRecord(recordFile) {
  if (!existsSync(recordFile)) throw new ExtractorError(`no installation record at ${recordFile}; run install-dependencies with the same output directory`);
  return readFileSync(recordFile, "utf8")
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => installedLock(line, recordFile));
}

function installedLock(line, recordFile) {
  const fields = parsed(line);
  const { file, sha256, installed_at: installedAt } = fields ?? {};
  const valid = typeof file === "string" && SHA256_HEX.test(sha256) && Number.isFinite(Date.parse(installedAt));
  if (!valid) throw new ExtractorError(`${recordFile}: malformed installation record line ${line}`);
  return { file, sha256, installedAt };
}

function parsed(line) {
  try {
    return JSON.parse(line);
  } catch {
    return undefined;
  }
}
