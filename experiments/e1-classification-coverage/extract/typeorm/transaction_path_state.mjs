// S2 path enumeration, the immutable part: what one control-flow path has done to the open
// transactions. Nothing here changes a state in place.

/**
 * One path so far: per receiver the starts still open (innermost last), and the starts a commit
 * or rollback popped with where that terminator ends.
 * @typedef {{
 *   open: ReadonlyMap<string, readonly ts.CallExpression[]>,
 *   pops: readonly (readonly [ts.CallExpression, number])[],
 * }} PathState
 */

/** @type {PathState} */
export const START_OF_PATH = Object.freeze({ open: new Map(), pops: [] });

const openOf = (state, receiver) => state.open.get(receiver) ?? [];
const withOpen = (state, receiver, starts) => ({ ...state, open: new Map(state.open).set(receiver, starts) });

export const started = (state, receiver, start) => withOpen(state, receiver, [...openOf(state, receiver), start]);

/** A commit or rollback pops the innermost open start of its receiver; one with nothing open is stray and changes nothing. */
export function terminated(state, receiver, end) {
  const open = openOf(state, receiver);
  const innermost = open.at(-1);
  if (innermost === undefined) return state;
  return { ...withOpen(state, receiver, open.slice(0, -1)), pops: [...state.pops, [innermost, end]] };
}

/** The starts a path ends without having closed. */
export const openAtEnd = (state) => [...state.open.values()].flat();

/** Two paths with the same key have done the same to every transaction, so they are one path. */
export function keyOf({ exit, state }) {
  const id = (start) => `${start.pos}-${start.end}`;
  const open = [...state.open].filter(([, starts]) => starts.length > 0).sort(([first], [second]) => (first < second ? -1 : 1));
  return JSON.stringify([
    exit,
    open.map(([receiver, starts]) => [receiver, starts.map(id)]),
    state.pops.map(([start, end]) => [id(start), end]),
  ]);
}
