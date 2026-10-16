// Which installed packages declare install scripts. The dependencies are installed
// with `--ignore-scripts` (inputs.md section 3, TypeORM clarification), so this reads
// package manifests and runs nothing; it shows how far the deviation can reach.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path/posix";

import { ExtractorError } from "./extractor_error.mjs";

const DEPENDENCY_SCRIPTS = ["preinstall", "install", "postinstall"];
const APPLICATION_SCRIPTS = [...DEPENDENCY_SCRIPTS, "prepare"];
const NODE_MODULES = "node_modules";

/**
 * @typedef {{ package: string, version: string, path: string, scripts: string[] }} InstallScripts
 *   `path` is the package directory below the application root ("." for the application itself).
 */

/** @returns {InstallScripts[]} the application and every installed package with at least one install script, by path */
export function installScriptsOf(applicationRoot) {
  const application = declaredScripts(applicationRoot, ".", APPLICATION_SCRIPTS);
  const dependencies = [...installedPackages(path.join(applicationRoot, NODE_MODULES), NODE_MODULES)].map(({ directory, relative }) =>
    declaredScripts(directory, relative, DEPENDENCY_SCRIPTS),
  );
  return [application, ...dependencies].filter((declared) => declared !== undefined).sort((first, second) => compare(first.path, second.path));
}

const compare = (first, second) => (first < second ? -1 : first > second ? 1 : 0);

/** Every package directory below a `node_modules` directory (or one of its `@scope` directories), nested `node_modules` included; a linked package is read but not entered. */
function* installedPackages(nodeModules, relative) {
  if (!existsSync(nodeModules)) return;
  for (const entry of readdirSync(nodeModules, { withFileTypes: true }).sort((first, second) => compare(first.name, second.name))) {
    const directory = path.join(nodeModules, entry.name);
    const entryRelative = path.join(relative, entry.name);
    if (entry.name.startsWith(".")) continue;
    if (entry.name.startsWith("@") && entry.isDirectory()) yield* installedPackages(directory, entryRelative);
    else {
      yield { directory, relative: entryRelative };
      if (entry.isDirectory()) yield* installedPackages(path.join(directory, NODE_MODULES), path.join(entryRelative, NODE_MODULES));
    }
  }
}

function declaredScripts(directory, relative, lifecycle) {
  const manifestFile = path.join(directory, "package.json");
  if (!existsSync(manifestFile)) return undefined;
  const manifest = readManifest(manifestFile);
  const declared = lifecycle.filter((name) => typeof manifest.scripts?.[name] === "string");
  // npm runs `node-gyp rebuild` as the install script of a package that ships a binding.gyp and declares neither.
  const buildsNative = existsSync(path.join(directory, "binding.gyp")) && !declared.includes("install") && !declared.includes("preinstall");
  const scripts = buildsNative ? [...declared, "install"] : declared;
  if (scripts.length === 0) return undefined;
  return { package: manifest.name ?? "", version: manifest.version ?? "", path: relative, scripts: lifecycle.filter((name) => scripts.includes(name)) };
}

function readManifest(file) {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    throw new ExtractorError(`${file}: cannot read the package manifest: ${error.message}`);
  }
}
