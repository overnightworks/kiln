// What the source text says about the function around a position: its declared
// parameters and the locals bound before the position (S3 parameters).

import ts from "typescript";

/**
 * @typedef {{
 *   start: number, end: number, parameters: string[],
 *   bindings: { availableFrom: number, name: string }[],
 * }} FunctionScope
 */

export function isFunctionLike(node) {
  return (
    ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node) || ts.isArrowFunction(node) ||
    ts.isMethodDeclaration(node) || ts.isConstructorDeclaration(node) ||
    ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node)
  ); // fmt: skip
}

/** @returns {FunctionScope[]} the scopes of every function that has a body */
export function readFunctionScopes(sourceText) {
  const scopes = [];
  const visit = (node) => {
    if (isFunctionLike(node) && node.body) scopes.push(scopeOf(node, sourceText));
    ts.forEachChild(node, visit);
  };
  visit(sourceText.sourceFile);
  return scopes;
}

/** The innermost scope whose body holds the offset. */
export function innermostScope(scopes, offset) {
  const around = scopes.filter((scope) => scope.start <= offset && offset < scope.end);
  return around.reduce((inner, scope) => (inner === undefined || scope.start > inner.start ? scope : inner), undefined);
}

/** Declared parameters, then locals bound before `offset`, that `referenced` names. */
export function feedingNames(scope, offset, referenced) {
  const earlier = scope.bindings.filter((binding) => binding.availableFrom <= offset).map((binding) => binding.name);
  return [...new Set([...scope.parameters, ...earlier])].filter((name) => referenced.has(name));
}

/** Every identifier read or written in the expression, however it is used. */
export function namesRead(node) {
  const names = new Set();
  const visit = (child) => {
    if (ts.isIdentifier(child) && !isMemberName(child)) names.add(child.text);
    ts.forEachChild(child, visit);
  };
  visit(node);
  return names;
}

function isMemberName(identifier) {
  const parent = identifier.parent;
  const isPropertyOfAccess = ts.isPropertyAccessExpression(parent) && parent.name === identifier;
  const isKey = ts.isPropertyAssignment(parent) && parent.name === identifier;
  return isPropertyOfAccess || isKey;
}

function scopeOf(node, sourceText) {
  return {
    start: sourceText.startOf(node.body),
    end: node.body.getEnd(),
    parameters: node.parameters.flatMap((parameter) => boundNames(parameter.name)).filter((name) => name !== "this"),
    bindings: ownBindings(node.body),
  };
}

function boundNames(name) {
  if (ts.isIdentifier(name)) return [name.text];
  return name.elements.flatMap((element) => (ts.isOmittedExpression(element) ? [] : boundNames(element.name)));
}

/** Locals declared in the function itself, in source order; a declaration binds after its own end. */
function ownBindings(body) {
  const bindings = [];
  const visit = (node) => {
    if (isFunctionLike(node) || ts.isClassLike(node)) return;
    if (ts.isVariableDeclaration(node)) {
      const availableFrom = isLoopVariable(node) ? node.name.getEnd() : node.getEnd();
      for (const name of boundNames(node.name)) bindings.push({ availableFrom, name });
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
  return bindings.sort((first, second) => first.availableFrom - second.availableFrom);
}

function isLoopVariable(declaration) {
  const list = declaration.parent;
  return ts.isVariableDeclarationList(list) && (ts.isForOfStatement(list.parent) || ts.isForInStatement(list.parent));
}
