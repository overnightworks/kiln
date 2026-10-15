// One TypeScript source file: positions in the analyzer's terms, and the call
// expressions the analyzer reports.

import ts from "typescript";

import { ExtractorError } from "./extractor_error.mjs";

/**
 * Method name of a member-access call as the analyzer reads it: a plain
 * property, or a computed one spelled as an identifier or a string literal.
 * @returns {string | undefined}
 */
export function calledMethodName(call) {
  const callee = call.expression;
  if (ts.isPropertyAccessExpression(callee)) return ts.isIdentifier(callee.name) ? callee.name.text : undefined;
  if (!ts.isElementAccessExpression(callee)) return undefined;
  const key = callee.argumentExpression;
  return ts.isIdentifier(key) || ts.isStringLiteral(key) ? key.text : undefined;
}

const LINE_TERMINATORS_AT_END = /[\r\n\u2028\u2029]+$/;

export class SourceText {
  /** @param {string} file path relative to the application root */
  constructor(file, text) {
    this.file = file;
    this.sourceFile = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
    this.text = text;
    this.lineStarts = this.sourceFile.getLineStarts();
    this.calls = collect(this.sourceFile, ts.isCallExpression);
    this.classMethods = collect(this.sourceFile, isClassMethod);
  }

  /** Offset of a validated position: the line exists and the column lies inside the line (a span's end may sit at its end). */
  offsetAt(position, { endOfSpan = false } = {}) {
    const { line, column } = position;
    const lineIndex = line - 1;
    const outside = () => new ExtractorError(`${this.file}: position ${line}:${column} is outside the source text`);
    if (!Number.isInteger(lineIndex) || lineIndex < 0 || lineIndex >= this.lineStarts.length) throw outside();
    const lineStart = this.lineStarts[lineIndex];
    const nextStart = this.lineStarts[lineIndex + 1] ?? this.text.length;
    const width = this.text.slice(lineStart, nextStart).replace(LINE_TERMINATORS_AT_END, "").length;
    const lastColumn = endOfSpan ? width : width - 1;
    if (!Number.isInteger(column) || column < 0 || column > lastColumn) throw outside();
    return lineStart + column;
  }

  positionAt(offset) {
    const { line, character } = this.sourceFile.getLineAndCharacterOfPosition(offset);
    return { line: line + 1, column: character };
  }

  /** Every call written exactly over the reported span with the reported name. */
  callsReportedAt(located, method) {
    const { start, end } = this.spanOf(located);
    return this.calls.filter(
      (call) => call.getStart(this.sourceFile) === start && call.getEnd() === end && calledMethodName(call) === method,
    );
  }

  /** Offsets of a reported span; positions outside the source text fail loudly. */
  spanOf(located) {
    return { start: this.offsetAt(located.start), end: this.offsetAt(located.end, { endOfSpan: true }) };
  }

  /** The one call the analyzer reported by its span and called name. */
  callReportedAt(located, method) {
    const fitting = this.callsReportedAt(located, method);
    if (fitting.length !== 1) {
      const where = `${located.start.line}:${located.start.column}`;
      throw new ExtractorError(`${this.file}: ${fitting.length} calls of \`${method}\` match the reported span at ${where}`);
    }
    return fitting[0];
  }

  startOf(node) {
    return node.getStart(this.sourceFile);
  }
}

/** What the analyzer calls a `MethodDefinition`: a method, constructor or accessor of a class. */
function isClassMethod(node) {
  const isMember = ts.isMethodDeclaration(node) || ts.isConstructorDeclaration(node) || ts.isGetAccessor(node) || ts.isSetAccessor(node);
  return isMember && ts.isClassLike(node.parent);
}

function collect(sourceFile, accepts) {
  const nodes = [];
  const visit = (node) => {
    if (accepts(node)) nodes.push(node);
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return nodes;
}
