// S2 control flow as explicit path enumeration: every path through one function (or the file
// body), statements in order, branches taken one way at a time. A path that ends in `throw` is
// dropped (a stated limitation). Paths that did the same to every transaction are one path, and a
// function with more distinct paths than the cap is not enumerated at all. Loops and labelled
// jumps are not modelled: a function whose transactions they could touch is rejected beforehand
// (unsupported_control_flow.mjs), so a loop here holds no transaction call and leaves the state as it was.

import ts from "typescript";

import { isFunctionLike } from "./function_scopes.mjs";
import { calledMethodName } from "./source_text.mjs";
import { START_OF_PATH, keyOf, started, terminated } from "./transaction_path_state.mjs";

export const START_TRANSACTION = "startTransaction";
export const TERMINATORS = new Set(["commitTransaction", "rollbackTransaction"]);

/** How a path ends: it runs on, or leaves its function (`return`) or its innermost loop or `switch`. */
const Exit = Object.freeze({ NORMAL: "normal", RETURN: "return", BREAK: "break", CONTINUE: "continue" });

/** @typedef {{ exit: string, state: import("./transaction_path_state.mjs").PathState }} Path */

class PathCapExceeded extends Error {}

/**
 * @param {ts.SourceFile} sourceFile
 * @param {ts.Node} root a function, or the file itself
 * @param {number} maxPaths
 * @returns {Path[] | undefined} the paths that do not end in `throw`, or `undefined` when the
 *   function has more than `maxPaths` distinct paths
 */
export function enumeratePaths(sourceFile, root, maxPaths) {
  try {
    return new PathWalk(sourceFile, maxPaths).children(root, START_OF_PATH);
  } catch (error) {
    if (error instanceof PathCapExceeded) return undefined;
    throw error;
  }
}

const onward = (state) => ({ exit: Exit.NORMAL, state });
const leaving = (exit, state) => ({ exit, state });

class PathWalk {
  #matters = new WeakMap();

  constructor(sourceFile, maxPaths) {
    this.sourceFile = sourceFile;
    this.maxPaths = maxPaths;
  }

  /** The paths through a node's children in order, from one state. */
  children(node, state) {
    let paths = [onward(state)];
    ts.forEachChild(node, (child) => {
      paths = this.#then(paths, (next) => this.#node(child, next));
    });
    return paths;
  }

  /** Paths still running go on through `step`; the others keep their ending. */
  #then(paths, step) {
    return this.#bounded(paths.flatMap((path) => (path.exit === Exit.NORMAL ? step(path.state) : [path])));
  }

  #bounded(paths) {
    const distinct = paths.length > 1 ? [...new Map(paths.map((path) => [keyOf(path), path])).values()] : paths;
    if (distinct.length > this.maxPaths) throw new PathCapExceeded();
    return distinct;
  }

  #node(node, state) {
    if (!this.#concerns(node)) return [onward(state)];
    if (ts.isCallExpression(node)) return this.#then(this.children(node, state), (next) => [onward(this.#effectOf(node, next))]);
    if (ts.isReturnStatement(node)) return this.#then(this.children(node, state), (next) => [leaving(Exit.RETURN, next)]);
    if (ts.isThrowStatement(node)) return [];
    if (ts.isBreakStatement(node) || ts.isContinueStatement(node)) return [leaving(ts.isBreakStatement(node) ? Exit.BREAK : Exit.CONTINUE, state)];
    if (ts.isIfStatement(node)) return this.#conditional(node.expression, [node.thenStatement, node.elseStatement], state);
    if (ts.isConditionalExpression(node)) return this.#conditional(node.condition, [node.whenTrue, node.whenFalse], state);
    if (ts.isSwitchStatement(node)) return this.#switch(node, state);
    if (ts.isIterationStatement(node, false)) return this.#loop(node, state);
    if (ts.isTryStatement(node)) return this.#try(node, state);
    return this.children(node, state);
  }

  /** An arm that is not written (no `else`) is the path that skips the conditional. */
  #conditional(condition, arms, state) {
    return this.#then(this.#node(condition, state), (entered) =>
      arms.flatMap((arm) => (arm === undefined ? [onward(entered)] : this.#node(arm, entered))),
    );
  }

  /** Control enters at any clause and runs on into the following ones; without `default` the switch can also be skipped. */
  #switch(node, state) {
    const { clauses } = node.caseBlock;
    return this.#then(this.#node(node.expression, state), (entered) => {
      const arms = clauses.flatMap((_, entry) => this.#clausesFrom(clauses.slice(entry), entered));
      return this.#bounded(clauses.some(ts.isDefaultClause) ? arms : [...arms, onward(entered)]);
    });
  }

  /** The statements of the clauses in sequence, until a path leaves the switch (`break`) or the function or loop. */
  #clausesFrom(clauses, state) {
    const statements = clauses.flatMap((clause) => [...clause.statements]);
    const paths = statements.reduce((current, statement) => this.#then(current, (next) => this.#node(statement, next)), [onward(state)]);
    return paths.map((path) => (path.exit === Exit.BREAK ? onward(path.state) : path));
  }

  /** The loop holds no transaction call, so running it leaves the state as it was; it may run zero times (not so a `do`). */
  #loop(node, state) {
    const afterBody = this.children(node, state).map((path) => (path.exit === Exit.RETURN ? path : onward(path.state)));
    return this.#bounded([...(ts.isDoStatement(node) ? [] : [onward(state)]), ...afterBody]);
  }

  /**
   * `try`, `catch` and `finally` run in sequence: after a body that completes, the code goes on
   * either without the catch or through it; `finally` runs after every path that did not throw.
   */
  #try(node, state) {
    const body = this.#node(node.tryBlock, state);
    const caught = node.catchClause ? body.flatMap((path) => (path.exit === Exit.NORMAL ? [path, ...this.#node(node.catchClause, path.state)] : [path])) : body;
    if (!node.finallyBlock) return this.#bounded(caught);
    return this.#bounded(caught.flatMap((path) => this.#node(node.finallyBlock, path.state).map((closing) => (closing.exit === Exit.NORMAL ? { ...closing, exit: path.exit } : closing))));
  }

  #effectOf(call, state) {
    const name = calledMethodName(call);
    const receiver = (call.expression.expression?.getText(this.sourceFile) ?? "").replace(/\s+/g, "");
    if (name === START_TRANSACTION) return started(state, receiver, call);
    if (TERMINATORS.has(name)) return terminated(state, receiver, call.getEnd());
    return state;
  }

  /** Whether the node can change a transaction or end a path; the rest of the tree is one step. */
  #concerns(node) {
    if (isFunctionLike(node)) return false;
    if (!this.#matters.has(node)) {
      let concerns = isTransactionCall(node) || [ts.isReturnStatement, ts.isThrowStatement, ts.isBreakStatement, ts.isContinueStatement].some((is) => is(node));
      ts.forEachChild(node, (child) => {
        concerns = this.#concerns(child) || concerns;
      });
      this.#matters.set(node, concerns);
    }
    return this.#matters.get(node);
  }
}

export const isTransactionCall = (node) => ts.isCallExpression(node) && (calledMethodName(node) === START_TRANSACTION || TERMINATORS.has(calledMethodName(node)));

/** Calls of `startTransaction` written in the root itself, not in a function nested in it. */
export function ownStarts(root) {
  const starts = [];
  const visit = (node) => {
    if (isFunctionLike(node)) return;
    if (ts.isCallExpression(node) && calledMethodName(node) === START_TRANSACTION) starts.push(node);
    ts.forEachChild(node, visit);
  };
  ts.forEachChild(root, visit);
  return starts;
}
