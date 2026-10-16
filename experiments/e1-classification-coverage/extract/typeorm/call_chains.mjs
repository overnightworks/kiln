// Query-builder chains (S5): which reported calls are written as one chain.

import ts from "typescript";

const CHAIN_ROOT_METHOD = "createQueryBuilder";

/** Steps through parentheses and non-null assertions, which do not end a chain. */
function unwrapped(expression) {
  let current = expression;
  while (ts.isParenthesizedExpression(current) || ts.isNonNullExpression(current)) current = current.expression;
  return current;
}

/** The call a call is invoked on (`a().b()` has `a()`), if any. */
function receiverCall(call) {
  const callee = call.expression;
  if (!ts.isPropertyAccessExpression(callee) && !ts.isElementAccessExpression(callee)) return undefined;
  const receiver = unwrapped(callee.expression);
  return ts.isCallExpression(receiver) ? receiver : undefined;
}

/** The call that invokes a member of this call's result, if any: `a()` in `a().b()`. */
function callOnResult(call) {
  let current = call;
  while (ts.isParenthesizedExpression(current.parent) || ts.isNonNullExpression(current.parent)) current = current.parent;
  const member = current.parent;
  const isMemberOfResult =
    (ts.isPropertyAccessExpression(member) && member.expression === current) ||
    (ts.isElementAccessExpression(member) && member.expression === current);
  const invoked = member.parent;
  return isMemberOfResult && ts.isCallExpression(invoked) && invoked.expression === member ? invoked : undefined;
}

/** The whole written chain around a call: the outermost call that invokes members on its result. */
export function chainExpressionOf(call) {
  let top = call;
  for (let next = callOnResult(top); next !== undefined; next = callOnResult(top)) top = next;
  return top;
}

/**
 * Groups reported sites by the `createQueryBuilder()` that roots their chain.
 * @template {{ method: string, call: import("typescript").CallExpression }} S
 * @param {S[]} sites
 * @returns {Map<S, S[]>} root site to every site in its chain (the root included, in input order)
 */
export function groupChains(sites) {
  const siteOfCall = new Map(sites.map((site) => [site.call, site]));
  const chains = new Map();
  for (const site of sites) {
    const root = rootOf(site, siteOfCall);
    if (root === undefined) continue;
    if (!chains.has(root)) chains.set(root, []);
    chains.get(root).push(site);
  }
  return chains;
}

/** The deepest reported `createQueryBuilder()` on the receiver path of the site, itself included. */
function rootOf(site, siteOfCall) {
  let root;
  for (let call = site.call; call !== undefined; call = receiverCall(call)) {
    const reported = siteOfCall.get(call);
    if (reported?.method === CHAIN_ROOT_METHOD) root = reported;
  }
  return root;
}
