// Shared arrangement for the unit tests: one source file, the analyzer messages
// that sit exactly where its calls are written, and the units made of them.

import { parseAnalyzerJson } from "./analyzer_report.mjs";
import { enumerateUnits } from "./enumeration.mjs";

export const APPLICATION = "acme/shop";
export const FILE = "src/db.ts";
export const WHOLE_APPLICATION = { directory: "", fallback: false };

/** The analyzer's own coordinates: line and column both counted from 0. */
function coordinatesAt(source, offset) {
  const before = source.slice(0, offset);
  return { line: before.split("\n").length - 1, column: offset - (before.lastIndexOf("\n") + 1) };
}

function offsetOf(source, needle, occurrence) {
  let offset = -1;
  for (let seen = 0; seen < occurrence; seen += 1) offset = source.indexOf(needle, offset + 1);
  if (offset < 0) throw new Error(`no occurrence ${occurrence} of ${needle}`);
  return offset;
}

function message(source, needle, occurrence, content, file = FILE) {
  const start = offsetOf(source, needle, occurrence);
  const from = coordinatesAt(source, start);
  const to = coordinatesAt(source, start + needle.length);
  return { filePath: file, fromLine: from.line, fromColumn: from.column, toLine: to.line, toColumn: to.column, content };
}

/** A reported ORM call written as `needle`. */
export function site(source, needle, method, { occurrence = 1, methodType = "read", receiver = "repository", types = ["Repository<Item>"], file } = {}) {
  const content = { type: "method", name: method, methodType, object: receiver, objectTypes: types, attributes: [] };
  return message(source, needle, occurrence, content, file);
}

/** A reported transaction marker written as `needle`. */
export function marker(source, needle, name, { occurrence = 1, file } = {}) {
  const content = { type: "method", name, methodType: "transaction", object: "", objectTypes: ["any"], attributes: [] };
  return message(source, needle, occurrence, content, file);
}

export function entityMessage() {
  return { filePath: FILE, fromLine: 0, fromColumn: 0, toLine: 0, toColumn: 1, content: { type: "entity", name: "Item" } };
}

export function reportOf(messages, selection = WHOLE_APPLICATION) {
  return parseAnalyzerJson(JSON.stringify({ messages, doneFiles: [] }), selection);
}

export function enumerationOf(source, messages, file = FILE) {
  return enumerateUnits(APPLICATION, reportOf(messages), new Map([[file, source]]));
}

export const unitsOf = (source, messages) => enumerationOf(source, messages).units;

/** A unit as [rule, line, parameters, absorbed site lines]. */
export const shape = (unit) => [unit.rule, unit.line, unit.parameters, unit.absorbedSites.map((absorbed) => absorbed.line)];
