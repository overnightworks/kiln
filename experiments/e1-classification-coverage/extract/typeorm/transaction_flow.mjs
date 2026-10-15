// S2 control flow: which started transactions are closed on every path, following the syntax
// tree of one source text. Per function, statements run in order; the arms of a conditional
// each run from the state before it and join afterwards (see transaction_flow_state.mjs).

import ts from "typescript";

import { isFunctionLike } from "./function_scopes.mjs";
import { calledMethodName } from "./source_text.mjs";
import {
  NEW_FLOW,
  afterLoop,
  endOfFunction,
  endsOfStarts,
  hidingOpenTransactions,
  joined,
  leftByReturnOrThrow,
  opened,
  resumed,
  terminated,
} from "./transaction_flow_state.mjs";

export const START_TRANSACTION = "startTransaction";
const TERMINATORS = new Set(["commitTransaction", "rollbackTransaction"]);
const LEAVES_THE_CLAUSE = [ts.isBreakStatement, ts.isContinueStatement, ts.isReturnStatement, ts.isThrowStatement];

/**
 * A start is matched when every path from it to the end of its function closes it; its
 * transaction then ends at the last commit or rollback that closes it on any path.
 * @param {ts.SourceFile} sourceFile
 * @returns {Map<ts.CallExpression, number | undefined>} per start call, where its transaction
 *   ends, or `undefined` when it is unmatched
 */
export function endsOfStartedTransactions(sourceFile) {
  const walk = new FlowWalk(sourceFile);
  return new Map([sourceFile, ...functionsOf(sourceFile)].flatMap((root) => [...walk.run(root)]));
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

class FlowWalk {
  constructor(sourceFile) {
    this.sourceFile = sourceFile;
  }

  /** A function (or the file) is walked on its own; the functions nested in it are not part of it. */
  run(root) {
    return endsOfStarts(endOfFunction(this.#children(root, NEW_FLOW)));
  }

  #node(node, flow) {
    if (isFunctionLike(node)) return flow;
    if (ts.isCallExpression(node)) return this.#call(node, this.#children(node, flow));
    if (ts.isReturnStatement(node) || ts.isThrowStatement(node)) return leftByReturnOrThrow(this.#children(node, flow));
    if (ts.isIfStatement(node)) return this.#conditional(node.expression, [node.thenStatement, node.elseStatement], flow);
    if (ts.isConditionalExpression(node)) return this.#conditional(node.condition, [node.whenTrue, node.whenFalse], flow);
    if (ts.isSwitchStatement(node)) return this.#switch(node, flow);
    if (ts.isIterationStatement(node, false)) return afterLoop(flow, this.#children(node, hidingOpenTransactions(flow)));
    if (ts.isTryStatement(node)) return this.#try(node, flow);
    return this.#children(node, flow);
  }

  #children(node, flow) {
    let current = flow;
    ts.forEachChild(node, (child) => {
      current = this.#node(child, current);
    });
    return current;
  }

  /** An arm that is not written (no `else`) is the path that skips the conditional. */
  #conditional(condition, arms, flow) {
    const entered = this.#node(condition, flow);
    return joined(entered, arms.map((arm) => (arm === undefined ? entered : this.#node(arm, entered))));
  }

  /** Clauses that can fall through run in sequence as one arm; a switch without `default` has the skipping arm too. */
  #switch(node, flow) {
    const entered = this.#node(node.expression, flow);
    const { clauses } = node.caseBlock;
    const arms = fallThroughGroups(clauses).map((group) =>
      group.reduce((current, clause) => clause.statements.reduce((inner, statement) => this.#node(statement, inner), current), entered),
    );
    if (!clauses.some(ts.isDefaultClause)) arms.push(entered);
    return joined(entered, arms);
  }

  /** `try`, `catch` and `finally` run in sequence; the code after them is reached unless both the body and the catch leave. */
  #try(node, flow) {
    const tried = this.#node(node.tryBlock, flow);
    const caught = node.catchClause ? this.#node(node.catchClause, resumed(tried)) : tried;
    const afterBody = node.catchClause ? { ...caught, exited: tried.exited && caught.exited } : tried;
    if (!node.finallyBlock) return afterBody;
    const finished = this.#node(node.finallyBlock, resumed(afterBody));
    return { ...finished, exited: afterBody.exited || finished.exited };
  }

  #call(call, flow) {
    const name = calledMethodName(call);
    const receiver = (call.expression.expression?.getText(this.sourceFile) ?? "").replace(/\s+/g, "");
    if (name === START_TRANSACTION) return opened(flow, receiver, call);
    if (TERMINATORS.has(name)) return terminated(flow, receiver, call.getEnd());
    return flow;
  }
}

/** Runs of clauses where each but the last can fall through into the next. */
function fallThroughGroups(clauses) {
  const groups = [[]];
  for (const clause of clauses) {
    groups.at(-1).push(clause);
    if (leavesTheClause(clause)) groups.push([]);
  }
  return groups.filter((group) => group.length > 0);
}

function leavesTheClause(clause) {
  let last = clause.statements.at(-1);
  while (last !== undefined && ts.isBlock(last)) last = last.statements.at(-1);
  return last !== undefined && LEAVES_THE_CLAUSE.some((is) => is(last));
}
