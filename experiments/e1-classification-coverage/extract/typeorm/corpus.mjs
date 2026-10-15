// The corpus edge: manifest rows, where the clones live, and the commands that
// fetch an application, install its dependencies and install the pinned analyzer.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

import { ExtractorError } from "./extractor_error.mjs";
import { isSelected } from "./file_selection.mjs";

export const MANIFEST_PATH = path.resolve(import.meta.dirname, "../../corpus-manifest.csv");
const TYPEORM = "TypeORM";
const ANALYZER_DIRECTORY = "splinter-eslint-tools";
const ANALYZER_PACKAGES = ["@ctring/splinter-eslint@1.0.9", "typescript@5.4.3"];
const ANALYZER_SCRIPT = path.join("node_modules", "@ctring", "splinter-eslint", "dist", "index.js");
const MANIFEST_COLUMNS = ["orm", "owner_repo", "repository_url", "description", "github_stars_at_annotation", "commit_sha_paper_abbrev", "commit_sha_full"];

/** @typedef {{ orm: string, ownerRepo: string, repositoryUrl: string, commitShaFull: string, directoryName: string }} ManifestRow */
/** @typedef {{ program: string, arguments: string[], directory: string }} Command */

/** @returns {Map<string, ManifestRow>} by `owner_repo` */
export function readManifest(file) {
  const [header, ...rows] = readFileSync(file, "utf8").split("\n").filter((line) => line !== "");
  if (header !== MANIFEST_COLUMNS.join(",")) throw new ExtractorError(`${file}: unexpected manifest header`);
  return new Map(
    rows.map((row) => {
      const fields = row.split(",");
      if (fields.length !== MANIFEST_COLUMNS.length) throw new ExtractorError(`${file}: malformed manifest row ${row}`);
      const [orm, ownerRepo, repositoryUrl, , , , commitShaFull] = fields;
      return [ownerRepo, { orm, ownerRepo, repositoryUrl, commitShaFull, directoryName: ownerRepo.replace("/", "__") }];
    }),
  );
}

export function applicationRow(manifest, ownerRepo) {
  const row = manifest.get(ownerRepo);
  if (row === undefined || row.orm !== TYPEORM) throw new ExtractorError(`${ownerRepo} is not a TypeORM application of the manifest`);
  return row;
}

export function corpusDirectory(setting, repositoryRoot) {
  if (!setting) throw new ExtractorError("set the corpus directory (--corpus-dir or KILN_E1_CORPUS_DIR)");
  const directory = path.resolve(setting);
  const insideRepository = directory === repositoryRoot || directory.startsWith(repositoryRoot + path.sep);
  if (insideRepository) throw new ExtractorError(`${directory} is inside the repository; keep the corpus outside it`);
  return directory;
}

export const applicationDirectory = (row, corpus) => path.join(corpus, row.directoryName);
export const analyzerScript = (corpus) => path.join(corpus, ANALYZER_DIRECTORY, ANALYZER_SCRIPT);

/** @returns {Command[]} */
export function fetchCommands(row, corpus) {
  const target = applicationDirectory(row, corpus);
  return [
    { program: "git", arguments: ["init", "--quiet", target], directory: corpus },
    { program: "git", arguments: ["fetch", "--depth", "1", row.repositoryUrl, row.commitShaFull], directory: target },
    { program: "git", arguments: ["checkout", "--quiet", "FETCH_HEAD"], directory: target },
  ];
}

/** The application's own dependencies, so the type checker resolves real types; install scripts stay off. */
export function dependencyCommands(row, corpus) {
  const directory = applicationDirectory(row, corpus);
  return [{ program: "npm", arguments: ["install", "--ignore-scripts", "--no-audit", "--no-fund"], directory }];
}

export function analyzerInstallCommands(corpus) {
  const directory = path.join(corpus, ANALYZER_DIRECTORY);
  return [{ program: "npm", arguments: ["install", ...ANALYZER_PACKAGES], directory }];
}

export function trackedFiles(applicationRoot) {
  return git(applicationRoot, "ls-files", "-z").split("\0").filter((file) => file !== "");
}

/** The analyzer reads the filesystem, so the checkout must be exactly the manifest commit. */
export function verifySnapshot(row, applicationRoot, selection) {
  const head = git(applicationRoot, "rev-parse", "HEAD").trim();
  if (head !== row.commitShaFull) {
    throw new ExtractorError(`${applicationRoot} is at ${head}, not the manifest commit ${row.commitShaFull}`);
  }
  const status = git(applicationRoot, "status", "--porcelain", "-z", "--no-renames", "--ignored", "--untracked-files=all");
  const strays = status.split("\0").map((entry) => entry.slice(3)).filter((file) => strayInSelection(file, selection));
  if (strays.length > 0) {
    throw new ExtractorError(`${applicationRoot} differs from its commit in the selection: ${strays.sort().join(", ")}`);
  }
}

function strayInSelection(file, selection) {
  const isSource = file.endsWith(".ts") || path.posix.basename(file) === "tsconfig.json";
  const installed = file.split("/").includes("node_modules");
  return isSource && !installed && isSelected(selection, file);
}

function git(directory, ...gitArguments) {
  return execFileSync("git", gitArguments, { cwd: directory, encoding: "utf8", maxBuffer: 1 << 28 });
}
