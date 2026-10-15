// S2 control flow: which started transactions are closed, following the syntax
// tree of one source text. Per function, statements run in order; the arms of a
// conditional each run from a copy of the state before it and join afterwards.

import ts from "typescript";

import { isFunctionLike } from "./function_scopes.mjs";
import { calledMethodName } from "./source_text.mjs";

export const START_TRANSACTION = "startTransaction";
const TERMINATORS = new Set(["commitTransaction", "rollbackTransaction"]);
const LEAVES_THE_CLAUSE = [ts.isBreakStatement, ts.isContinueStatement, ts.isReturnStatement, ts.isThrowStatement];

/**
 * One transaction as written: the start calls that alternatively open it, and where its last
 * commit or rollback ends. A terminator across a loop boundary leaves it unmatched.
 * @typedef {{ starts: ts.CallExpression[], end: number | undefined, crossedLoop: boolean }} Opening
 */

/**
 * The state of one receiver: the open transactions (innermost last), the ones its latest
 * terminator closed (a later terminator extends them), and how many are hidden by a loop.
 * @typedef {{ open: Opening[], closed: Opening[], hidden: number }} ReceiverState
 */

const NOTHING_OPEN = { open: [], closed: [], hidden: 0 };

/**
 * @param {ts.SourceFile} sourceFile
 * @returns {Map<ts.CallExpression, number | undefined>} per start call, where its transaction
 *   ends, or `undefined` when no commit or rollback closes it
 */
export function endsOfStartedTransactions(sourceFile) {
  const walk = new FlowWalk(sourceFile);
  for (const root of [sourceFile, ...functionsOf(sourceFile)]) walk.run(root);
  return walk.ends();
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

/** @typedef {ReadonlyMap<string, ReceiverState>} Flow the state per receiver text */

class FlowWalk {
  /** @type {Opening[]} */
  #openings = [];

  constructor(sourceFile) {
    this.sourceFile = sourceFile;
  }

  /** A function (or the file) is walked on its own; the functions nested in it are not part of it. */
  run(root) {
    this.#children(root, new Map());
  }

  ends() {
    return new Map(this.#openings.flatMap((opening) => opening.starts.map((start) => [start, opening.crossedLoop ? undefined : opening.end])));
  }

  #node(node, flow) {
    if (isFunctionLike(node)) return flow;
    if (ts.isCallExpression(node)) return this.#call(node, this.#children(node, flow));
    if (ts.isIfStatement(node)) return this.#conditional(node.expression, [node.thenStatement, node.elseStatement], flow);
    if (ts.isConditionalExpression(node)) return this.#conditional(node.condition, [node.whenTrue, node.whenFalse], flow);
    if (ts.isSwitchStatement(node)) return this.#switch(node, flow);
    if (ts.isIterationStatement(node, false)) return this.#loop(node, flow);
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
    return this.#join(entered, arms.map((arm) => (arm === undefined ? entered : this.#node(arm, entered))));
  }

  /** Clauses that can fall through run in sequence as one arm; a switch without `default` has the skipping arm too. */
  #switch(node, flow) {
    const entered = this.#node(node.expression, flow);
    const { clauses } = node.caseBlock;
    const arms = fallThroughGroups(clauses).map((group) =>
      group.reduce((current, clause) => clause.statements.reduce((inner, statement) => this.#node(statement, inner), current), entered),
    );
    if (!clauses.some(ts.isDefaultClause)) arms.push(entered);
    return this.#join(entered, arms);
  }

  /** A loop body is walked once and sees no transaction from outside: its terminators cannot close one. */
  #loop(node, flow) {
    const hidden = new Map([...flow].map(([receiver, state]) => [receiver, { open: state.open, closed: [], hidden: state.open.length }]));
    this.#children(node, hidden);
    return flow;
  }

  #call(call, flow) {
    const name = calledMethodName(call);
    const receiver = (call.expression.expression?.getText(this.sourceFile) ?? "").replace(/\s+/g, "");
    if (name === START_TRANSACTION) return this.#open(call, receiver, flow);
    if (TERMINATORS.has(name)) return this.#close(call, receiver, flow);
    return flow;
  }

  #open(call, receiver, flow) {
    const opening = { starts: [call], end: undefined, crossedLoop: false };
    this.#openings.push(opening);
    const state = flow.get(receiver) ?? NOTHING_OPEN;
    return new Map(flow).set(receiver, { ...state, open: [...state.open, opening], closed: [] });
  }

  #close(call, receiver, flow) {
    const state = flow.get(receiver) ?? NOTHING_OPEN;
    const end = call.getEnd();
    if (state.open.length > state.hidden) {
      const innermost = state.open.at(-1);
      innermost.end = Math.max(innermost.end ?? end, end);
      return new Map(flow).set(receiver, { ...state, open: state.open.slice(0, -1), closed: [innermost] });
    }
    if (state.open.length > 0) state.open.at(-1).crossedLoop = true;
    for (const closed of state.closed) closed.end = Math.max(closed.end ?? end, end);
    return flow;
  }

  /**
   * Transactions open before the conditional stay open while any arm leaves them open. One opened
   * inside the arms continues as a single alternative opening when every arm leaves one open at that
   * depth; otherwise it is open on only some paths and stays unmatched.
   */
  #join(entered, arms) {
    const receivers = new Set([entered, ...arms].flatMap((flow) => [...flow.keys()]));
    return new Map([...receivers].map((receiver) => [receiver, joinedState(entered.get(receiver) ?? NOTHING_OPEN, arms.map((arm) => arm.get(receiver) ?? NOTHING_OPEN))]));
  }
}

function joinedState(entered, arms) {
  const before = new Set(entered.open);
  const keptDepth = (arm) => {
    const firstNew = arm.open.findIndex((opening) => !before.has(opening));
    return firstNew < 0 ? arm.open.length : firstNew;
  };
  const longest = arms.reduce((best, arm) => (keptDepth(arm) > keptDepth(best) ? arm : best));
  const tails = arms.map((arm) => arm.open.slice(keptDepth(arm)));
  const alternativeDepth = Math.min(...tails.map((tail) => tail.length));
  const alternatives = Array.from({ length: alternativeDepth }, (_, level) => oneOpening(tails.map((tail) => tail[level])));
  const closed = [...new Set(arms.flatMap((arm) => arm.closed))];
  return { open: [...longest.open.slice(0, keptDepth(longest)), ...alternatives], closed, hidden: entered.hidden };
}

/** The first opening takes over the starts of the others, so all of them end where it ends. */
function oneOpening([first, ...others]) {
  for (const other of others) {
    first.starts.push(...other.starts);
    first.crossedLoop ||= other.crossedLoop;
    other.starts = [];
  }
  return first;
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
