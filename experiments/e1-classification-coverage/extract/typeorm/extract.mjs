// Command line of the TypeORM unit extractor.
//
//   extract.mjs install-analyzer
//   extract.mjs fetch OWNER/REPO
//   extract.mjs install-dependencies OWNER/REPO --output-dir DIR
//       (verifies the checkout before and after, records the generated lockfiles in DIR)
//   extract.mjs units OWNER/REPO --output-dir DIR   (the same DIR: checks the recorded lockfiles)
//
// The corpus directory comes from `--corpus-dir` or `KILN_E1_CORPUS_DIR` and
// must lie outside the repository.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";

import { runAnalyzer } from "./analyzer_command.mjs";
import { parseAnalyzerJson } from "./analyzer_report.mjs";
import {
  MANIFEST_PATH, analyzerInstallCommands, analyzerScript, applicationDirectory, applicationRow, corpusDirectory,
  dependencyCommands, fetchCommands, readManifest,
} from "./corpus.mjs"; // fmt: skip
import { enumerateUnits } from "./enumeration.mjs";
import { selectionOf } from "./file_selection.mjs";
import { installScriptsOf } from "./install_scripts.mjs";
import { lockRecordFile, readLockRecord, writeLockRecord } from "./lock_record.mjs";
import { renderInstallScripts, renderReceiverEvidence, renderUnits, renderUnmatchedMarkers } from "./rendering.mjs";
import { installDependencies, trackedFiles, verifyInstalledCheckout } from "./snapshot.mjs";

function main(argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      "corpus-dir": { type: "string", default: process.env.KILN_E1_CORPUS_DIR },
      "output-dir": { type: "string" },
    },
  });
  const [command, application] = positionals;
  const corpus = corpusDirectory(values["corpus-dir"], repositoryRoot());
  if (command === "install-analyzer") return runCommands(analyzerInstallCommands(corpus));
  const row = applicationRow(readManifest(MANIFEST_PATH), application);
  if (command === "fetch") return runCommands(fetchCommands(row, corpus));
  const outputDirectory = values["output-dir"];
  if (command === "install-dependencies" && outputDirectory) return installAndRecord(row, corpus, outputDirectory);
  if (command === "units" && outputDirectory) return writeUnits(row, corpus, outputDirectory);
  throw new Error("usage: extract.mjs install-analyzer | fetch OWNER/REPO | install-dependencies|units OWNER/REPO --output-dir DIR");
}

function repositoryRoot() {
  const toplevel = execFileSync("git", ["rev-parse", "--show-toplevel"], { cwd: import.meta.dirname, encoding: "utf8" });
  return path.resolve(toplevel.trim());
}

function runCommands(commands) {
  for (const command of commands) {
    mkdirSync(command.directory, { recursive: true });
    execFileSync(command.program, command.arguments, { cwd: command.directory, stdio: "inherit" });
  }
}

function installAndRecord(row, corpus, outputDirectory) {
  const locks = installDependencies(row, applicationDirectory(row, corpus), () => runCommands(dependencyCommands(row, corpus)));
  mkdirSync(outputDirectory, { recursive: true });
  writeLockRecord(lockRecordFile(outputDirectory, row), locks);
}

function writeUnits(row, corpus, outputDirectory) {
  mkdirSync(outputDirectory, { recursive: true });
  const stem = path.join(outputDirectory, row.directoryName);
  const root = applicationDirectory(row, corpus);
  const selection = selectionOf(trackedFiles(root));
  verifyInstalledCheckout(row, root, readLockRecord(lockRecordFile(outputDirectory, row)));
  const messages = `${stem}.analyzer.json`;
  const analyzerLog = runAnalyzer({ script: analyzerScript(corpus), workingDirectory: corpus, selection, applicationRoot: root, output: path.resolve(messages) });
  writeFileSync(`${stem}.analyzer.log`, analyzerLog);
  const report = parseAnalyzerJson(readFileSync(messages, "utf8"), selection);
  const files = new Set([...report.sites, ...report.markers].map((located) => located.file));
  const sources = new Map([...files].sort().map((file) => [file, readFileSync(path.join(root, file), "utf8")]));
  const enumeration = enumerateUnits(row.ownerRepo, report, sources);
  writeFileSync(`${stem}.units.jsonl`, renderUnits(enumeration.units));
  writeFileSync(`${stem}.receivers.jsonl`, renderReceiverEvidence(enumeration.receiverEvidence));
  writeFileSync(`${stem}.unmatched-markers.jsonl`, renderUnmatchedMarkers(enumeration.unmatchedMarkers));
  writeFileSync(`${stem}.install-scripts.jsonl`, renderInstallScripts(installScriptsOf(root)));
}

main(process.argv.slice(2));
