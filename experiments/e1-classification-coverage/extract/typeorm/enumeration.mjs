// S2, S3 and S5, and the S4 evidence: from the analyzer report and the source
// text to units, mechanically.

import { groupChains, chainExpressionOf } from "./call_chains.mjs";
import { ExtractorError } from "./extractor_error.mjs";
import { feedingNames, innermostScope, namesRead, readFunctionScopes } from "./function_scopes.mjs";
import { SourceText } from "./source_text.mjs";
import { readTransactionBlocks } from "./transaction_blocks.mjs";

const UNRESOLVED_RECEIVER_TYPES = new Set(["any", "unknown", "error"]);

/**
 * @typedef {{ file: string, line: number, column: number }} SiteLocation
 * @typedef {{
 *   application: string, file: string, line: number, column: number,
 *   rule: "S2" | "S3" | "S5", parameters: string[], absorbedSites: SiteLocation[],
 * }} Unit
 */

/**
 * @param {string} application
 * @param {{ sites: import("./analyzer_report.mjs").Site[], markers: import("./analyzer_report.mjs").Marker[] }} report
 * @param {ReadonlyMap<string, string>} sources text of every file the report names
 */
export function enumerateUnits(application, report, sources) {
  const filesWithReports = new Set([...report.sites, ...report.markers].map((located) => located.file));
  const entries = [];
  const unmatchedMarkers = [];
  for (const file of [...filesWithReports].sort()) {
    const text = sources.get(file);
    if (text === undefined) throw new ExtractorError(`${file}: no source text for the reported file`);
    const sourceText = new SourceText(file, text);
    const sites = report.sites.filter((site) => site.file === file).map((site) => locatedSite(site, sourceText));
    const { blocks, unmatched } = readTransactionBlocks(sourceText, report.markers.filter((marker) => marker.file === file));
    entries.push(...unitsOfFile(application, sourceText, sites, blocks));
    unmatchedMarkers.push(...unmatched.map(({ file: markerFile, start }) => ({ file: markerFile, ...start })));
  }
  entries.sort(compareEntries);
  return {
    units: entries.map((entry) => entry.unit),
    receiverEvidence: receiverEvidence(report.sites),
    unmatchedMarkers: unmatchedMarkers.sort(compareLocations),
  };
}

/** A reported site with the call it names and where that call starts and ends. */
function locatedSite(site, sourceText) {
  const call = sourceText.callReportedAt(site, site.method);
  return { ...site, call, startOffset: sourceText.startOf(call), endOffset: call.getEnd() };
}

function unitsOfFile(application, sourceText, sites, reportedBlocks) {
  const assigned = new Set();
  for (const site of sites) {
    if (assigned.has(site.call)) {
      throw new ExtractorError(`${sourceText.file}: two reports for the call at ${site.start.line}:${site.start.column}`);
    }
    assigned.add(site.call);
  }
  const scopes = readFunctionScopes(sourceText);
  const blocks = outermost(reportedBlocks);
  const absorbed = new Map(blocks.map((block) => [block, []]));
  const entries = [];
  for (const group of groupsOf(sites)) {
    const owner = blocks.find((block) => block.start <= group.root.startOffset && group.root.startOffset < block.end);
    if (owner) absorbed.get(owner).push(...group.members);
    else entries.push(chainOrSiteEntry(application, sourceText, scopes, group));
  }
  for (const [block, members] of absorbed) entries.push(transactionEntry(application, sourceText, scopes, block, members));
  return entries;
}

/** Every S5 chain as one group, every other site as a group of its own. */
function groupsOf(sites) {
  const chains = groupChains(sites);
  const inChain = new Set([...chains.values()].flat());
  const single = sites.filter((site) => !inChain.has(site)).map((site) => ({ root: site, members: [site], chain: false }));
  const chained = [...chains].map(([root, members]) => ({ root, members, chain: true }));
  return [...chained, ...single];
}

function outermost(blocks) {
  const ordered = [...blocks].sort((first, second) => first.position - second.position);
  return ordered.filter((block) => !ordered.some((other) => other !== block && other.start <= block.position && block.position < other.end));
}

function chainOrSiteEntry(application, sourceText, scopes, { root, members, chain }) {
  const scope = innermostScope(scopes, root.startOffset);
  const read = namesRead(chain ? chainExpressionOf(root.call) : root.call);
  const parameters = scope ? feedingNames(scope, root.startOffset, read) : [];
  return entry(application, sourceText, { position: root.startOffset, end: root.endOffset }, chain ? "S5" : "S3", parameters, members);
}

function transactionEntry(application, sourceText, scopes, block, members) {
  const scope = innermostScope(scopes, block.position);
  return entry(application, sourceText, { position: block.position, end: block.end }, "S2", scope ? scope.parameters : [], members);
}

function entry(application, sourceText, { position, end }, rule, parameters, members) {
  const { line, column } = sourceText.positionAt(position);
  const absorbedSites = [...members]
    .sort((first, second) => first.startOffset - second.startOffset || first.endOffset - second.endOffset)
    .map((site) => ({ file: site.file, ...site.start }));
  return { unit: { application, file: sourceText.file, line, column, rule, parameters, absorbedSites }, end };
}

function compareEntries(first, second) {
  return compareLocations(first.unit, second.unit) || compareText(first.unit.rule, second.unit.rule) || first.end - second.end;
}

function compareLocations(first, second) {
  return compareText(first.file, second.file) || first.line - second.line || first.column - second.column;
}

function compareText(first, second) {
  return first < second ? -1 : first > second ? 1 : 0;
}

function receiverEvidence(sites) {
  const ordered = [...sites].sort(
    (first, second) =>
      compareText(first.file, second.file) || first.start.line - second.start.line || first.start.column - second.start.column ||
      first.end.line - second.end.line || first.end.column - second.end.column,
  ); // fmt: skip
  return ordered.map((site) => ({
    file: site.file,
    ...site.start,
    method: site.method,
    receiver: site.receiver,
    receiver_types: site.receiverTypes,
    receiver_resolved: site.receiverTypes.length > 0 && !UNRESOLVED_RECEIVER_TYPES.has(site.receiverTypes[0]),
  }));
}
