// The edge that runs the one frozen `splinter-eslint` command.

import { spawnSync } from "node:child_process";

import { ExtractorError } from "./extractor_error.mjs";
import { analyzerArguments } from "./file_selection.mjs";

/**
 * The analyzer reports a file it could not read only on its error stream and
 * still exits 0, so any error output means the denominator lost a file.
 */
export function runAnalyzer({ script, workingDirectory, selection, applicationRoot, output }) {
  const run = spawnSync("node", [script, ...analyzerArguments(selection, applicationRoot, output)], {
    cwd: workingDirectory,
    encoding: "utf8",
    maxBuffer: 1 << 28,
  });
  if (run.error) throw run.error;
  if (run.status !== 0) throw new ExtractorError(`the analyzer exited with status ${run.status}: ${run.stderr}`);
  if (run.stderr.trim() !== "") throw new ExtractorError(`the analyzer reported errors:\n${run.stderr}`);
}
