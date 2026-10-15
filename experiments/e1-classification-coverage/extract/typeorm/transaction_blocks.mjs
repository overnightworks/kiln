// S2: the explicit transactions the source text writes, in the exact frozen shapes.

import ts from "typescript";

import { isFunctionLike } from "./function_scopes.mjs";
import { calledMethodName } from "./source_text.mjs";

const START = "startTransaction";
const TERMINATORS = new Set(["commitTransaction", "rollbackTransaction"]);
const CALLBACK_TRANSACTION = "transaction";
const CALLBACK_RECEIVER_NAMES = new Set(["dataSource", "manager", "connection"]);

/**
 * @typedef {{ position: number, start: number, end: number }} TransactionBlock
 *   `position` is where the unit sits; the lexical body is [start, end).
 */

/**
 * @param {import("./source_text.mjs").SourceText} sourceText
 * @param {import("./analyzer_report.mjs").Marker[]} markers
 * @returns {{ blocks: TransactionBlock[], unmatched: import("./analyzer_report.mjs").Marker[] }}
 */
export function readTransactionBlocks(sourceText, markers) {
  const blocks = [];
  const unmatched = [];
  for (const marker of markers) {
    const block = sourceText.callsReportedAt(marker, marker.name).map((call) => blockOf(call, sourceText))[0];
    if (block) blocks.push(block);
    else unmatched.push(marker);
  }
  return { blocks, unmatched };
}

function blockOf(call, sourceText) {
  const name = calledMethodName(call);
  if (name === START) return startedBlock(call, sourceText);
  if (name === CALLBACK_TRANSACTION) return callbackBlock(call, sourceText);
  return undefined;
}

function callbackBlock(call, sourceText) {
  const receiver = call.expression.expression;
  const callback = [...call.arguments].reverse().find((argument) => ts.isArrowFunction(argument) || ts.isFunctionExpression(argument));
  if (!callback || !CALLBACK_RECEIVER_NAMES.has(finalName(receiver))) return undefined;
  return { position: sourceText.startOf(call), start: sourceText.startOf(callback.body), end: callback.body.getEnd() };
}

/** `dataSource` and `this.dataSource` both end in the name `dataSource`. */
function finalName(expression) {
  if (ts.isIdentifier(expression)) return expression.text;
  return ts.isPropertyAccessExpression(expression) ? expression.name.text : undefined;
}

/** The start up to the last commit or rollback on the same receiver, within the same function. */
function startedBlock(start, sourceText) {
  const receiverText = withoutWhitespace(start.expression.expression.getText(sourceText.sourceFile));
  const sameReceiver = (call) => withoutWhitespace(call.expression.expression?.getText(sourceText.sourceFile) ?? "") === receiverText;
  const inFunction = sourceText.calls.filter((call) => sameFunction(call, start) && sameReceiver(call));
  const startedAt = start.getEnd();
  const nextStart = inFunction.find((call) => calledMethodName(call) === START && call.getStart(sourceText.sourceFile) >= startedAt);
  const limit = nextStart ? nextStart.getStart(sourceText.sourceFile) : Infinity;
  const terminators = inFunction.filter(
    (call) => TERMINATORS.has(calledMethodName(call)) && call.getStart(sourceText.sourceFile) >= startedAt && call.getEnd() <= limit,
  );
  if (terminators.length === 0) return undefined;
  return {
    position: sourceText.startOf(start),
    start: sourceText.startOf(start),
    end: Math.max(...terminators.map((call) => call.getEnd())),
  };
}

function withoutWhitespace(text) {
  return text.replace(/\s+/g, "");
}

/** Both calls are written in the same innermost function (or both at top level). */
function sameFunction(first, second) {
  return enclosingFunction(first) === enclosingFunction(second);
}

function enclosingFunction(node) {
  for (let current = node.parent; current !== undefined; current = current.parent) {
    if (isFunctionLike(current)) return current;
  }
  return undefined;
}
