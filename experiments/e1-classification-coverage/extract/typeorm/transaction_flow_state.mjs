// S2 control flow, the immutable part: the state one execution path carries and how
// the paths of a conditional join. Nothing here changes a state in place.

/**
 * What one path knows about one receiver: the open transactions (innermost last), the ones its
 * latest terminator closed (a later terminator extends them) and how many a loop hides.
 * A transaction is identified by its start call; alternative starts are identified by the first.
 * @typedef {{ open: ts.CallExpression[], closed: ts.CallExpression[], hidden: number }} ReceiverState
 */

/**
 * One path through a function: the receivers' states, every transaction seen on this or an
 * earlier path with its start calls and the last terminator that closed it, those that can no
 * longer be matched, and whether the path ended in `return` or `throw`.
 * @typedef {{
 *   receivers: ReadonlyMap<string, ReceiverState>,
 *   starts: ReadonlyMap<ts.CallExpression, ts.CallExpression[]>,
 *   ends: ReadonlyMap<ts.CallExpression, number>,
 *   unmatched: ReadonlySet<ts.CallExpression>,
 *   exited: boolean,
 * }} Flow
 */

const NOTHING_OPEN = Object.freeze({ open: [], closed: [], hidden: 0 });

/** @type {Flow} */
export const NEW_FLOW = Object.freeze({ receivers: new Map(), starts: new Map(), ends: new Map(), unmatched: new Set(), exited: false });

const withEntry = (map, key, value) => new Map(map).set(key, value);
const withMembers = (set, members) => new Set([...set, ...members]);
const stateOf = (flow, receiver) => flow.receivers.get(receiver) ?? NOTHING_OPEN;

/** The latest terminator position per transaction, extended by `end` for the given ones. */
function extendedEnds(ends, transactions, end) {
  return new Map([...ends, ...transactions.map((transaction) => [transaction, Math.max(ends.get(transaction) ?? end, end)])]);
}

export function opened(flow, receiver, start) {
  const state = stateOf(flow, receiver);
  return {
    ...flow,
    receivers: withEntry(flow.receivers, receiver, { ...state, open: [...state.open, start], closed: [] }),
    starts: withEntry(flow.starts, start, [start]),
  };
}

/** A terminator closes the innermost open transaction; with none it extends the ones just closed. */
export function terminated(flow, receiver, end) {
  const state = stateOf(flow, receiver);
  if (state.open.length > state.hidden) {
    const innermost = state.open.at(-1);
    const receivers = withEntry(flow.receivers, receiver, { ...state, open: state.open.slice(0, -1), closed: [innermost] });
    return { ...flow, receivers, ends: extendedEnds(flow.ends, [innermost], end) };
  }
  const crossed = state.open.slice(-1);
  return { ...flow, unmatched: withMembers(flow.unmatched, crossed), ends: extendedEnds(flow.ends, state.closed, end) };
}

/** A `return` ends the path: every transaction still open on it is left open, so it is unmatched. */
export function leftByReturn(flow) {
  const stillOpen = [...flow.receivers.values()].flatMap((state) => state.open);
  return { ...flow, unmatched: withMembers(flow.unmatched, stillOpen), exited: true };
}

/** A `throw` ends the path unchecked: judging it needs the exception modelling of `try`/`catch`. */
export const leftByThrow = (flow) => ({ ...flow, exited: true });
export const resumed = (flow) => ({ ...flow, exited: false });

/** A loop body sees no transaction from outside: its terminators cannot close one. */
export function hidingOpenTransactions(flow) {
  const receivers = new Map([...flow.receivers].map(([receiver, state]) => [receiver, { open: state.open, closed: [], hidden: state.open.length }]));
  return { ...flow, receivers };
}

/** What the loop body found out about transactions; the paths continue as they entered it. */
export const afterLoop = (entered, body) => ({ ...entered, starts: body.starts, ends: body.ends, unmatched: body.unmatched });

/** The transactions that no commit or rollback closes on every path of the function. */
export function endOfFunction(flow) {
  const stillOpen = [...flow.receivers.values()].flatMap((state) => state.open);
  return { ...flow, unmatched: withMembers(flow.unmatched, stillOpen) };
}

/** @returns {Map<ts.CallExpression, number | undefined>} per start call, where its transaction ends */
export function endsOfStarts(flow) {
  return new Map([...flow.starts].flatMap(([transaction, starts]) => starts.map((start) => [start, flow.unmatched.has(transaction) ? undefined : flow.ends.get(transaction)])));
}

/**
 * The paths of a conditional meet. A path that exited does not continue, so it does not decide
 * (unless none continues), but what it learned about terminators stays. Per receiver:
 * - a transaction open before stays open if every continuing path leaves it open and is closed
 *   if every one closes it; open on some and closed on others, it is unmatched;
 * - one started in the arms continues as a single alternative opening when every arm leaves one
 *   open at that depth; a start left open in only some arms is unmatched.
 * @param {Flow} entered the state the arms started from
 * @param {Flow[]} arms
 * @returns {Flow}
 */
export function joined(entered, arms) {
  const continuing = arms.some((arm) => !arm.exited) ? arms.filter((arm) => !arm.exited) : arms;
  const receiverNames = new Set([entered, ...continuing].flatMap((flow) => [...flow.receivers.keys()]));
  const joins = new Map([...receiverNames].map((name) => [name, joinReceiver(stateOf(entered, name), continuing.map((arm) => stateOf(arm, name)))]));
  const joinList = [...joins.values()];
  const alternatives = joinList.flatMap((join) => join.alternatives);
  const abandoned = joinList.flatMap((join) => join.abandoned);
  const allUnmatched = withMembers(new Set(), arms.flatMap((arm) => [...arm.unmatched]));
  const startsOfAll = new Map(arms.flatMap((arm) => [...arm.starts]));
  const ends = arms.reduce((merged, arm) => new Map([...merged, ...[...arm.ends].map(([id, end]) => [id, Math.max(merged.get(id) ?? end, end)])]), new Map());

  const starts = new Map(startsOfAll);
  for (const [kept, ...others] of alternatives) {
    starts.set(kept, [kept, ...others].flatMap((id) => startsOfAll.get(id)));
    for (const other of others) starts.delete(other);
    if ([kept, ...others].some((id) => allUnmatched.has(id))) allUnmatched.add(kept);
  }
  for (const id of abandoned) allUnmatched.add(id);

  const receivers = new Map([...joins].map(([name, join]) => [name, { open: join.open, closed: join.closed.filter((id) => !allUnmatched.has(id)), hidden: stateOf(entered, name).hidden }]));
  return { receivers, starts, ends, unmatched: allUnmatched, exited: arms.every((arm) => arm.exited) };
}

function joinReceiver(before, arms) {
  const keptCounts = arms.map((arm) => sharedPrefixLength(before.open, arm.open));
  const keptByAll = Math.min(...keptCounts);
  const keptBySome = Math.max(...keptCounts);
  const beforeSet = new Set(before.open);
  const tails = arms.map((arm) => arm.open.slice(keptByAll).filter((id) => !beforeSet.has(id)));
  const depth = Math.min(...tails.map((tail) => tail.length));
  const alternatives = Array.from({ length: depth }, (_, level) => tails.map((tail) => tail[level]));
  return {
    open: [...before.open.slice(0, keptByAll), ...alternatives.map(([first]) => first)],
    closed: [...new Set(arms.flatMap((arm) => arm.closed))],
    alternatives,
    abandoned: [...before.open.slice(keptByAll, keptBySome), ...tails.flatMap((tail) => tail.slice(depth))],
  };
}

function sharedPrefixLength(base, other) {
  let length = 0;
  while (length < base.length && base[length] === other[length]) length += 1;
  return length;
}
