// Print the licenses of the packages bundled into the web UI: the production dependencies in
// package.json and everything they depend on. Dev dependencies don't end up in the app.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

type Manifest = {
  name: string;
  version: string;
  license?: string;
  dependencies?: Record<string, string>;
};

const read = (dir: string): Manifest =>
  JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as Manifest;

/** Where `name` resolves from `from`, the way Node walks up node_modules. */
function resolve(name: string, from: string): string {
  for (let dir = from; ; dir = dirname(dir)) {
    const found = join(dir, "node_modules", name);
    if (existsSync(join(found, "package.json"))) return found;
    if (dirname(dir) === dir) throw new Error(`${name} is not installed (from ${from})`);
  }
}

const seen = new Map<string, string>();
function visit(dir: string) {
  if (seen.has(dir)) return;
  seen.set(dir, dir);
  for (const name of Object.keys(read(dir).dependencies ?? {})) visit(resolve(name, dir));
}

const root = process.cwd();
for (const name of Object.keys(read(root).dependencies ?? {})) visit(resolve(name, root));

const packages = [...seen.keys()]
  .map((dir) => ({ dir, ...read(dir) }))
  .sort((a, b) => a.name.localeCompare(b.name));

for (const { dir, name, version, license } of packages) {
  const file = readdirSync(dir).find((f) => /^(licen[cs]e|copying)/i.test(f));
  if (!file) throw new Error(`${name} has no license file`);
  console.log("=".repeat(80));
  console.log(`${name} ${version} (${license ?? "see below"})\n`);
  console.log(readFileSync(join(dir, file), "utf8").trim());
  console.log();
}
