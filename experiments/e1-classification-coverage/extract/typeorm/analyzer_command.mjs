// The edge that runs the one frozen `splinter-eslint` command.

import { spawnSync } from "node:child_process";

import { assertOnlyFilesOutsideTheProject } from "./analyzer_errors.mjs";
import { ExtractorError } from "./extractor_error.mjs";
import { analyzerArguments } from "./file_selection.mjs";

/** Returns the analyzer's error output, which is only ever files outside the project. */
export function runAnalyzer({ script, workingDirectory, selection, applicationRoot, output }) {
  const run = spawnSync("node", [script, ...analyzerArguments(selection, applicationRoot, output)], {
    cwd: workingDirectory,
    encoding: "utf8",
    maxBuffer: 1 << 28,
  });
  if (run.error) throw run.error;
  if (run.status !== 0) throw new ExtractorError(`the analyzer exited with status ${run.status}: ${run.stderr}`);
  assertOnlyFilesOutsideTheProject(run.stderr);
  return run.stderr;
}
