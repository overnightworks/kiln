// S2: the explicit transactions the source text writes, in the exact frozen shapes.

import ts from "typescript";

import { ExtractorError } from "./extractor_error.mjs";
import { calledMethodName } from "./source_text.mjs";
import { START_TRANSACTION, endsOfStartedTransactions } from "./transaction_flow.mjs";

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
  const endsOfStarts = endsOfStartedTransactions(sourceText.sourceFile);
  for (const marker of markers) {
    const block = blockOfReport(marker, sourceText, endsOfStarts);
    if (block) blocks.push(block);
    else unmatched.push(marker);
  }
  return { blocks, unmatched };
}

function blockOfReport(marker, sourceText, endsOfStarts) {
  const calls = sourceText.callsReportedAt(marker, marker.name);
  if (calls.length === 1) return blockOf(calls[0], sourceText, endsOfStarts);
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

function blockOf(call, sourceText, endsOfStarts) {
  const name = calledMethodName(call);
  if (name === START_TRANSACTION) return startedBlock(call, sourceText, endsOfStarts);
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

/** The start up to where its transaction ends; a start no commit or rollback closes opens no unit. */
function startedBlock(start, sourceText, endsOfStarts) {
  const end = endsOfStarts.get(start);
  return end === undefined ? undefined : { position: sourceText.startOf(start), start: sourceText.startOf(start), end };
}
