// S2: the explicit transactions the source text writes, in the exact frozen shapes.

import ts from "typescript";

import { ExtractorError } from "./extractor_error.mjs";
import { inExclusiveBranches } from "./exclusive_branches.mjs";
import { isFunctionLike } from "./function_scopes.mjs";
import { calledMethodName } from "./source_text.mjs";

const START = "startTransaction";
const TERMINATORS = new Set(["commitTransaction", "rollbackTransaction"]);
const CALLBACK_TRANSACTION = "transaction";
const TRANSACTION_DECORATORS = new Set(["Transaction", "LazyTransaction"]);
const CALLBACK_RECEIVER_NAMES = new Set(["dataSource", "manager", "connection"]);

/**
 * @typedef {{ position: number, start: number, end: number }} TransactionBlock
 *   `position` is where the unit sits; the lexical body is [start, end).
 */

/**
 * A transaction report is a call of the reported name over the reported span, or
 * a method decorated `@Transaction` / `@LazyTransaction` over it. A report that
 * is neither has an invalid identity and fails the run.
 * @param {import("./source_text.mjs").SourceText} sourceText
 * @param {import("./analyzer_report.mjs").Marker[]} markers
 * @returns {{ blocks: TransactionBlock[], unmatched: import("./analyzer_report.mjs").Marker[] }}
 */
export function readTransactionBlocks(sourceText, markers) {
  const blocks = [];
  const unmatched = [];
  for (const marker of markers) {
    const block = blockOfReport(marker, sourceText);
    if (block) blocks.push(block);
    else unmatched.push(marker);
  }
  return { blocks, unmatched };
}

function blockOfReport(marker, sourceText) {
  const calls = sourceText.callsReportedAt(marker, marker.name);
  if (calls.length === 1) return blockOf(calls[0], sourceText);
  if (calls.length === 0 && isDecoratedMethodReport(marker, sourceText)) return undefined;
  const where = `${marker.start.line}:${marker.start.column}`;
  throw new ExtractorError(`${marker.file}: ${calls.length} calls of \`${marker.name}\` and no transaction-decorated method match the report at ${where}`);
}

function isDecoratedMethodReport(marker, sourceText) {
  const { start, end } = sourceText.spanOf(marker);
  return sourceText.classMethods.some((method) => {
    const spansReport = method.getEnd() === end && spanStarts(method, sourceText).includes(start);
    return spansReport && methodName(method) === marker.name && decoratorNames(method).some((name) => TRANSACTION_DECORATORS.has(name));
  });
}

/** Parsers differ on whether a method's span starts at its first decorator or after the last. */
function spanStarts(method, sourceText) {
  const firstToken = (method.modifiers ?? []).find((modifier) => !ts.isDecorator(modifier)) ?? method.name ?? method.getFirstToken(sourceText.sourceFile);
  return [sourceText.startOf(method), sourceText.startOf(firstToken)];
}

/** The name the analyzer reports: an identifier key, otherwise nothing. */
function methodName(method) {
  if (ts.isConstructorDeclaration(method)) return "constructor";
  return ts.isIdentifier(method.name) ? method.name.text : "";
}

function decoratorNames(method) {
  return (ts.getDecorators(method) ?? []).map(({ expression }) => {
    const called = ts.isCallExpression(expression) ? expression.expression : expression;
    return ts.isIdentifier(called) ? called.text : "";
  });
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

/**
 * The start up to the last commit or rollback on the same receiver within the same function, before
 * the next start that is not nested in this one. Following the control flow: starts in exclusive
 * branches of one conditional are one alternative opening, a start written while a transaction is
 * open on its path nests, and a commit or rollback closes the innermost open one.
 */
function startedBlock(start, sourceText) {
  const receiverOf = (call) => withoutWhitespace(call.expression.expression?.getText(sourceText.sourceFile) ?? "");
  const startOf = (call) => sourceText.startOf(call);
  const after = sourceText.calls
    .filter((call) => sameFunction(call, start) && receiverOf(call) === receiverOf(start) && startOf(call) >= start.getEnd())
    .sort((first, second) => startOf(first) - startOf(second));
  const openings = [[start]];
  let end;
  for (const call of after) {
    const name = calledMethodName(call);
    if (name === START) {
      if (openings.length === 0) break;
      const alternative = openings.at(-1).every((opened) => inExclusiveBranches(opened, call));
      if (alternative) openings.at(-1).push(call);
      else openings.push([call]);
    } else if (TERMINATORS.has(name)) {
      openings.pop();
      end = call.getEnd();
    }
  }
  return end === undefined ? undefined : { position: startOf(start), start: startOf(start), end };
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
