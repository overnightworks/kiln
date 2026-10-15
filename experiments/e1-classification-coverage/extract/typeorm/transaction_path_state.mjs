// S2 path enumeration, the immutable part: what one control-flow path has done to the open
// transactions. Nothing here changes a state in place.

/**
 * @typedef {{ start: ts.CallExpression, loopDepth: number }} OpenStart
 *
 * One path so far: per receiver the starts still open (innermost last) and the loop depth each
 * was made at, the starts a commit or rollback popped with where that terminator ends, and the
 * starts a loop boundary has already made unmatched.
 * @typedef {{
 *   open: ReadonlyMap<string, readonly OpenStart[]>,
 *   loopDepth: number,
 *   pops: readonly (readonly [ts.CallExpression, number])[],
 *   spoiled: readonly ts.CallExpression[],
 * }} PathState
 */

/** @type {PathState} */
export const START_OF_PATH = Object.freeze({ open: new Map(), loopDepth: 0, pops: [], spoiled: [] });

const openOf = (state, receiver) => state.open.get(receiver) ?? [];
const withOpen = (state, receiver, starts) => ({ ...state, open: new Map(state.open).set(receiver, starts) });

export const started = (state, receiver, start) => withOpen(state, receiver, [...openOf(state, receiver), { start, loopDepth: state.loopDepth }]);

/**
 * A commit or rollback pops the innermost open start of its receiver; one with nothing open is
 * stray and changes nothing. A start made outside the loop the terminator sits in is not popped:
 * its terminator lies across the loop boundary, so the start is unmatched.
 */
export function terminated(state, receiver, end) {
  const open = openOf(state, receiver);
  const innermost = open.at(-1);
  if (innermost === undefined) return state;
  if (innermost.loopDepth < state.loopDepth) return { ...state, spoiled: [...state.spoiled, innermost.start] };
  return { ...withOpen(state, receiver, open.slice(0, -1)), pops: [...state.pops, [innermost.start, end]] };
}

export const enteredLoop = (state) => ({ ...state, loopDepth: state.loopDepth + 1 });

/** Starts made in the loop body and still open when it ends cannot be closed outside it. */
export function leftLoop(state) {
  const spoiled = [...state.spoiled];
  const open = new Map();
  for (const [receiver, starts] of state.open) {
    const isLocal = (entry) => entry.loopDepth === state.loopDepth;
    spoiled.push(...starts.filter(isLocal).map(({ start }) => start));
    open.set(receiver, starts.filter((entry) => !isLocal(entry)));
  }
  return { ...state, open, loopDepth: state.loopDepth - 1, spoiled };
}

/** The starts a path ends without having closed, plus those a loop boundary already spoiled. */
export const spoiledAtEnd = (state) => [...state.spoiled, ...[...state.open.values()].flatMap((starts) => starts.map(({ start }) => start))];

/** Two paths with the same key have done the same to every transaction, so they are one path. */
export function keyOf({ exit, state }) {
  const id = (start) => `${start.pos}-${start.end}`;
  const open = [...state.open].filter(([, starts]) => starts.length > 0).sort(([first], [second]) => (first < second ? -1 : 1));
  return JSON.stringify([
    exit,
    state.loopDepth,
    open.map(([receiver, starts]) => [receiver, starts.map(({ start, loopDepth }) => [id(start), loopDepth])]),
    state.pops.map(([start, end]) => [id(start), end]),
    state.spoiled.map(id),
  ]);
}
