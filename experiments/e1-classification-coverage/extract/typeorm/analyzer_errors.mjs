// The analyzer exits 0 and reports a file it could not parse only on its error
// stream. A file that no tsconfig includes is outside the project, which is the
// frozen selection at work; any other error means the denominator lost a file.

import { ExtractorError } from "./extractor_error.mjs";

const REPORT_START = /^Parsing error: /m;
const OUTSIDE_PROJECT = /^Parsing error: ESLint was configured to run on `[^`\n]+` using `parserOptions\.project`:[\s\S]*none of those TSConfigs include this file/;

/** Fails loudly on any error output that is not a file outside the project. */
export function assertOnlyFilesOutsideTheProject(errorOutput) {
  const text = errorOutput.trim();
  if (text === "") return;
  const startsWithReport = REPORT_START.test(text) && text.search(REPORT_START) === 0;
  const reports = text.split(/^(?=Parsing error: )/m);
  const unexpected = startsWithReport ? reports.filter((report) => !OUTSIDE_PROJECT.test(report)) : reports;
  if (unexpected.length > 0) throw new ExtractorError(`the analyzer reported errors:\n${unexpected.join("\n")}`);
}
