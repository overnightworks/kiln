// S2 matching is defined for straight-line code, if/else, ternaries, `switch`, `try`/`catch`/`finally`,
// `return` and `throw`. A function that mixes transactions with a loop or a labelled jump is outside
// that definition and is recognised here, before any path is enumerated.

import ts from "typescript";

import { isFunctionLike } from "./function_scopes.mjs";
import { TERMINATORS, isTransactionCall } from "./transaction_paths.mjs";
import { calledMethodName } from "./source_text.mjs";

/**
 * Whether the function (or the file body) written in `root` has any of:
 * - a transaction start or terminator inside a loop (body, condition, initializer, update);
 * - a `break` or `continue` that names a label;
 * - a `return` inside a loop that an enclosing `finally` holding a terminator would follow.
 * Functions nested in `root` are their own roots and not looked into.
 * @param {ts.Node} root
 */
export function hasUnsupportedControlFlow(root) {
  return unsupportedBelow(root, { inLoop: false, followedByTerminator: false });
}

function unsupportedBelow(parent, context) {
  return ts.forEachChild(parent, (node) => unsupportedIn(node, context)) === true;
}

function unsupportedIn(node, context) {
  if (isFunctionLike(node)) return false;
  if (context.inLoop && isTransactionCall(node)) return true;
  if (isLabelledJump(node)) return true;
  if (context.inLoop && context.followedByTerminator && ts.isReturnStatement(node)) return true;
  if (ts.isTryStatement(node)) return unsupportedInTry(node, context);
  return unsupportedBelow(node, { ...context, inLoop: context.inLoop || ts.isIterationStatement(node, false) });
}

/** The `finally` runs after the body and the `catch`, not after itself. */
function unsupportedInTry(node, context) {
  const guarded = { ...context, followedByTerminator: context.followedByTerminator || (node.finallyBlock !== undefined && holdsTerminator(node.finallyBlock)) };
  return [[node.tryBlock, guarded], [node.catchClause, guarded], [node.finallyBlock, context]].some(([part, partContext]) => part !== undefined && unsupportedIn(part, partContext));
}

const isLabelledJump = (node) => (ts.isBreakStatement(node) || ts.isContinueStatement(node)) && node.label !== undefined;

function holdsTerminator(node) {
  if (isFunctionLike(node)) return false;
  if (ts.isCallExpression(node) && TERMINATORS.has(calledMethodName(node))) return true;
  return ts.forEachChild(node, holdsTerminator) === true;
}
