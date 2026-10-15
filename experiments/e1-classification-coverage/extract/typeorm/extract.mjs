// Command line of the TypeORM unit extractor.
//
//   extract.mjs install-analyzer
//   extract.mjs fetch OWNER/REPO
//   extract.mjs install-dependencies OWNER/REPO
//   extract.mjs units OWNER/REPO --output-dir DIR
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
  dependencyCommands, fetchCommands, readManifest, trackedFiles, verifySnapshot,
} from "./corpus.mjs"; // fmt: skip
import { enumerateUnits } from "./enumeration.mjs";
import { selectionOf } from "./file_selection.mjs";
import { renderReceiverEvidence, renderUnits, renderUnmatchedMarkers } from "./rendering.mjs";

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
  if (command === "install-dependencies") return runCommands(dependencyCommands(row, corpus));
  if (command === "units" && values["output-dir"]) return writeUnits(row, corpus, values["output-dir"]);
  throw new Error("usage: extract.mjs install-analyzer | fetch|install-dependencies|units OWNER/REPO [--output-dir DIR]");
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

function writeUnits(row, corpus, outputDirectory) {
  mkdirSync(outputDirectory, { recursive: true });
  const stem = path.join(outputDirectory, row.directoryName);
  const root = applicationDirectory(row, corpus);
  const selection = selectionOf(trackedFiles(root));
  verifySnapshot(row, root);
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
}

main(process.argv.slice(2));
