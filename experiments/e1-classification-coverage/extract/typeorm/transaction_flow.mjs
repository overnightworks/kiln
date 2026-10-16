// S2 control flow: which started transactions every path closes. Each function (and the file
// body) is walked on its own as an enumeration of its paths (transaction_paths.mjs), unless it has
// control flow the definition does not cover (unsupported_control_flow.mjs); a start is matched
// when no path leaves it open and some path closes it.

import ts from "typescript";

import { isFunctionLike } from "./function_scopes.mjs";
import { enumeratePaths, ownStarts } from "./transaction_paths.mjs";
import { openAtEnd } from "./transaction_path_state.mjs";
import { hasUnsupportedControlFlow } from "./unsupported_control_flow.mjs";

export { START_TRANSACTION } from "./transaction_paths.mjs";

/** The most distinct paths one function may have; its starts are all unmatched beyond that. */
export const MAX_PATHS_PER_FUNCTION = 4096;

export const NOT_CLOSED_ON_EVERY_PATH = "not closed on every path";
export const PATH_CAP_EXCEEDED = "path cap exceeded";
export const UNSUPPORTED_CONTROL_FLOW = "unsupported control flow";

/**
 * @typedef {{ end: number } | { unmatched: string }} Closing
 *   where the start's transaction ends, or why the start is unmatched
 */

/**
 * A start is matched when no path from it to the end of its function leaves it open and a commit
 * or rollback closes it on some path; its transaction ends at the last terminator that closes it
 * on any path. A start that no path reaches is unmatched, as is every start of a function with
 * unsupported control flow or more paths than the cap.
 * @param {ts.SourceFile} sourceFile
 * @returns {Map<ts.CallExpression, Closing>} per start call
 */
export function closingsOfStartedTransactions(sourceFile, { maxPaths = MAX_PATHS_PER_FUNCTION } = {}) {
  const roots = [sourceFile, ...functionsOf(sourceFile)];
  return new Map(roots.flatMap((root) => [...closingsIn(sourceFile, root, maxPaths)]));
}

function functionsOf(sourceFile) {
  const functions = [];
  const visit = (node) => {
    if (isFunctionLike(node)) functions.push(node);
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return functions;
}

function closingsIn(sourceFile, root, maxPaths) {
  const starts = ownStarts(root);
  if (starts.length === 0) return [];
  if (hasUnsupportedControlFlow(root)) return starts.map((start) => [start, { unmatched: UNSUPPORTED_CONTROL_FLOW }]);
  const paths = enumeratePaths(sourceFile, root, maxPaths);
  if (paths === undefined) return starts.map((start) => [start, { unmatched: PATH_CAP_EXCEEDED }]);
  const left = new Set(paths.flatMap(({ state }) => openAtEnd(state)));
  const ends = new Map();
  for (const { state } of paths) {
    for (const [start, end] of state.pops) ends.set(start, Math.max(ends.get(start) ?? end, end));
  }
  return starts.map((start) => [start, left.has(start) || !ends.has(start) ? { unmatched: NOT_CLOSED_ON_EVERY_PATH } : { end: ends.get(start) }]);
}
