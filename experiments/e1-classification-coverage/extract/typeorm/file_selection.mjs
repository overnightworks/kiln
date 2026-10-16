// The frozen TypeORM file-selection rule of inputs.md section 3.

import path from "node:path/posix";

const TSCONFIG_NAME = "tsconfig.json";
const FALLBACK_EXCLUDED_DIRECTORIES = new Set(["node_modules", "dist", "build", "test", "tests", "__tests__"]);
const FALLBACK_EXCLUDED_FILE_SUFFIXES = [".spec.ts", ".test.ts"];
const FALLBACK_INCLUDE_GLOB = "**/*.ts";

/**
 * Where the analyzer runs: `directory` below the application root ("" for the
 * root itself), and whether the fallback command with its output discard applies.
 * @typedef {{ directory: string, fallback: boolean }} Selection
 */

/** The shortest tracked `tsconfig.json` path (fewest characters, then lexical) fixes the project. */
export function selectionOf(trackedFiles) {
  const tsconfigs = trackedFiles.filter((file) => path.basename(file) === TSCONFIG_NAME);
  if (tsconfigs.length === 0) return { directory: "", fallback: true };
  const nearest = tsconfigs.reduce((best, candidate) => (comesBefore(candidate, best) ? candidate : best));
  return { directory: path.dirname(nearest) === "." ? "" : path.dirname(nearest), fallback: false };
}

function comesBefore(candidate, best) {
  if (candidate.length !== best.length) return candidate.length < best.length;
  return candidate < best;
}

/** The arguments after the analyzer script: the frozen command of the selection. */
export function analyzerArguments(selection, applicationRoot, output) {
  const directory = path.join(applicationRoot, selection.directory);
  const include = selection.fallback ? ["--include", FALLBACK_INCLUDE_GLOB] : [];
  return [directory, ...include, "--output", output];
}

/** True for a path (relative to the application root) the fallback rule discards. */
export function isFallbackExcluded(relativePath) {
  const directories = path.dirname(relativePath).split("/");
  const inExcludedDirectory = directories.some((part) => FALLBACK_EXCLUDED_DIRECTORIES.has(part));
  return inExcludedDirectory || FALLBACK_EXCLUDED_FILE_SUFFIXES.some((suffix) => relativePath.endsWith(suffix));
}

/** True for a path (relative to the application root) the selection keeps. */
export function isSelected(selection, relativePath) {
  if (selection.fallback) return !isFallbackExcluded(relativePath);
  return selection.directory === "" || relativePath.startsWith(`${selection.directory}/`);
}
