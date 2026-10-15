// Reads the analyzer's `messages.json` into sites and transaction markers.

import path from "node:path/posix";

import { ExtractorError } from "./extractor_error.mjs";
import { isSelected } from "./file_selection.mjs";

const ORM_METHOD_TYPES = new Set(["read", "write", "other"]);
const TRANSACTION_METHOD_TYPE = "transaction";
const IGNORED_MESSAGE_TYPES = new Set(["entity"]);

/**
 * A position: line counted from 1, column from 0. The analyzer counts both
 * from 0 and the report is converted here, once.
 * @typedef {{ line: number, column: number }} Position
 * @typedef {{ file: string, start: Position, end: Position }} Located
 * @typedef {Located & { method: string, receiver: string, receiverTypes: string[] }} Site
 * @typedef {Located & { name: string }} Marker
 */

/**
 * @param {string} text the analyzer's `messages.json`
 * @param {import("./file_selection.mjs").Selection} selection the run's selection
 * @returns {{ sites: Site[], markers: Marker[] }}
 */
export function parseAnalyzerJson(text, selection) {
  const sites = [];
  const markers = [];
  for (const message of JSON.parse(text).messages) {
    const file = applicationFile(message.filePath, selection);
    if (file === undefined) continue;
    const located = {
      file,
      start: { line: message.fromLine + 1, column: message.fromColumn },
      end: { line: message.toLine + 1, column: message.toColumn },
    };
    const content = message.content;
    if (content.type === "method") addMethodMessage(content, located, sites, markers);
    else if (!IGNORED_MESSAGE_TYPES.has(content.type)) {
      throw new ExtractorError(`unknown message type ${JSON.stringify(content.type)} in ${file}`);
    }
  }
  return { sites, markers };
}

function addMethodMessage(content, located, sites, markers) {
  if (content.methodType === TRANSACTION_METHOD_TYPE) {
    markers.push({ ...located, name: content.name });
  } else if (ORM_METHOD_TYPES.has(content.methodType)) {
    sites.push({ ...located, method: content.name, receiver: content.object, receiverTypes: content.objectTypes });
  } else {
    throw new ExtractorError(`unknown method type ${JSON.stringify(content.methodType)} in ${located.file}`);
  }
}

/** The path below the application root; undefined for a report the selection does not keep. */
function applicationFile(reportedPath, selection) {
  const file = path.join(selection.directory, reportedPath);
  const leavesRoot = path.isAbsolute(file) || file === ".." || file.startsWith("../");
  return leavesRoot || !isSelected(selection, file) ? undefined : file;
}
