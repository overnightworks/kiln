import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";

import { parseAnalyzerJson } from "./analyzer_report.mjs";
import { enumerateUnits } from "./enumeration.mjs";

const FIXTURES = path.join(import.meta.dirname, "fixtures");

/** Units of the one recorded file, enumerated from the analyzer output recorded for it. */
function recordedUnits(application, selection, sourceFile) {
  const recorded = path.join(FIXTURES, application);
  const report = parseAnalyzerJson(readFileSync(path.join(recorded, "messages.json"), "utf8"), selection);
  const sources = new Map([[sourceFile, readFileSync(path.join(recorded, sourceFile), "utf8")]]);
  return enumerateUnits(`fixture/${application}`, report, sources).units;
}

const SOURCE_PROJECT = { directory: "src", fallback: false };
const RECORDED_DB = "src/model/db/RecordedDB.ts";
const RESTORE_LINES = [28, 52];
const INSERT_ONCE_LINES = [66, 80];

const withinLines = ([first, last]) => (unit) => unit.line >= first && unit.line <= last;

test("KILN-E1TO-23 EPGStation restore yields one S2 unit parameterized by items that absorbs its four sites", () => {
  const units = recordedUnits("epgstation", SOURCE_PROJECT, RECORDED_DB).filter(withinLines(RESTORE_LINES));
  assert.deepEqual(
    units.map(({ rule, line, parameters, absorbedSites }) => [rule, line, parameters, absorbedSites.map((site) => site.line)]),
    [["S2", 34, ["items"], [39, 40, 41, 45]]],
  );
});

test("KILN-E1TO-24 EPGStation insertOnce yields one S5 unit for its two reported sites, parameterized by recorded and the earlier-bound connection", () => {
  const units = recordedUnits("epgstation", SOURCE_PROJECT, RECORDED_DB).filter(withinLines(INSERT_ONCE_LINES));
  assert.deepEqual(
    units.map(({ rule, line, column, parameters, absorbedSites }) => [rule, line, column, parameters, absorbedSites.length]),
    [["S5", 68, 29, ["recorded", "connection"], 2]],
  );
});

test("KILN-E1TO-25 cool-admin-midway CloudDBService.data yields five units and getRepository one", () => {
  const dataToGetRepository = [31, 76];
  const units = recordedUnits("cool-admin-midway", { directory: "", fallback: false }, "src/modules/cloud/service/db.ts").filter(
    withinLines(dataToGetRepository),
  );
  assert.deepEqual(
    units.map(({ rule, line }) => [rule, line]),
    [["S3", 33], ["S3", 36], ["S3", 42], ["S3", 45], ["S5", 49], ["S3", 70]],
  );
});
