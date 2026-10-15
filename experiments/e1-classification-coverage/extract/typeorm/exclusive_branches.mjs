// Control flow of the syntax tree: which two nodes can never run in the same pass.

import ts from "typescript";

/** The arm of an enclosing conditional that holds `node`, per conditional: if/else, ternary, switch clause. */
function armsOf(node) {
  const arms = new Map();
  for (let child = node, parent = node.parent; parent !== undefined; child = parent, parent = parent.parent) {
    const arm = armOf(parent, child);
    if (arm !== undefined) arms.set(parent, arm);
  }
  return arms;
}

function armOf(conditional, child) {
  if (ts.isIfStatement(conditional)) return [conditional.thenStatement, conditional.elseStatement].includes(child) ? child : undefined;
  if (ts.isConditionalExpression(conditional)) return [conditional.whenTrue, conditional.whenFalse].includes(child) ? child : undefined;
  return ts.isCaseBlock(conditional) ? child : undefined;
}

/** The two nodes sit in different arms of one conditional (if/else, ternary, switch clauses). */
export function inExclusiveBranches(first, second) {
  const secondArms = armsOf(second);
  return [...armsOf(first)].some(([conditional, arm]) => secondArms.has(conditional) && secondArms.get(conditional) !== arm);
}
