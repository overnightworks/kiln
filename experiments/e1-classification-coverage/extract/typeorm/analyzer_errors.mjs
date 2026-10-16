// The analyzer exits 0 and reports a file it could not parse only on its error
// stream. A file that no tsconfig includes is outside the project, which is the
// frozen selection at work; any other error means the denominator lost a file.
// The error stream is accepted only when it consists, line for line, of that one
// diagnostic of typescript-estree (`createProjectProgram`), repeated.

import { ExtractorError } from "./extractor_error.mjs";

const PATH = "[^`\\n]+";
const DOCS_URL = "https://typescript-eslint.io/troubleshooting/typed-linting#i-get-errors-telling-me-eslint-was-configured-to-run--however-that-tsconfig-does-not--none-of-those-tsconfigs-include-this-file";

const lines = (...parts) => parts.map((part) => `${part}\\n`).join("");
const header = (projects) => `Parsing error: ESLint was configured to run on \`${PATH}\` using \`parserOptions\\.project\`:${projects}`;
const explanation = (inclusion, specifier) =>
  lines(
    `However, ${inclusion} include this file\\. Either:`,
    "- Change ESLint's list of included files to not include this file",
    `- Change ${specifier} to include this file`,
    "- Create a new TSConfig that includes this file and include it in your parserOptions\\.project",
    `See the typescript-eslint docs for more info: ${DOCS_URL.replaceAll(".", "\\.")}`,
  );

const SEVERAL_PROJECTS = header(" \\n(?:- [^\\n]+\\n)+") + explanation("none of those TSConfigs", "one of those TSConfigs");
const ONE_PROJECT = header(` [^\\n]+\\n`) + explanation("that TSConfig does not", "that TSConfig");
const FILE_OUTSIDE_THE_PROJECT = new RegExp(`(?:${SEVERAL_PROJECTS}|${ONE_PROJECT})`, "y");

/** Fails loudly on any error output that is not, whole, a file outside the project. */
export function assertOnlyFilesOutsideTheProject(errorOutput) {
  let consumed = 0;
  while (consumed < errorOutput.length) {
    FILE_OUTSIDE_THE_PROJECT.lastIndex = consumed;
    if (!FILE_OUTSIDE_THE_PROJECT.test(errorOutput)) break;
    consumed = FILE_OUTSIDE_THE_PROJECT.lastIndex;
  }
  if (consumed < errorOutput.length) {
    throw new ExtractorError(`the analyzer reported errors:\n${errorOutput.slice(consumed)}`);
  }
}
