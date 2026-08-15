import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const desktopDir = join(repoRoot, "apps", "desktop");
const outputPath = resolve(process.argv[2] || join(repoRoot, "target", "THIRD-PARTY-NOTICES.txt"));
const expectedNode = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8")).engines.node;

if (process.version !== `v${expectedNode}`) {
  throw new Error(`Node ${expectedNode} is required, found ${process.version}`);
}

const missingLicenseSources = new Map([
  ["cargo:alloc-stdlib@0.2.4", "cargo:alloc-no-stdlib@2.0.4"],
  ["cargo:selectors@0.36.1", "cargo:cssparser@0.36.0"],
  ["cargo:tauri-plugin@2.6.3", "cargo:tauri-build@2.6.3"],
  ["cargo:unic-char-property@0.9.0", "cargo:unic-common@0.9.0"],
  ["cargo:unic-char-range@0.9.0", "cargo:unic-common@0.9.0"],
  ["cargo:unic-ucd-ident@0.9.0", "cargo:unic-common@0.9.0"],
  ["cargo:unic-ucd-version@0.9.0", "cargo:unic-common@0.9.0"]
]);
const explicitUpstreamTexts = new Map([
  ["npm:is-reference@3.0.3", standardMitLicenseText()],
  ["npm:locate-character@3.0.0", standardMitLicenseText()],
  ["cargo:unic-common@0.9.0", `${rustUnicCopyright()}\n\n${standardMitLicenseText()}`],
  ["cargo:webview2-com-macros@0.8.1", webview2LicenseText()],
  ["cargo:webview2-com-sys@0.38.2", webview2LicenseText()],
  ["cargo:webview2-com@0.38.2", webview2LicenseText()]
]);

const components = [
  ...collectNpmRuntimeDependencies(),
  ...collectCargoDependencies(join(repoRoot, "helper", "Cargo.toml"), "1.96.0-x86_64-pc-windows-gnullvm", "x86_64-pc-windows-gnullvm"),
  ...collectCargoDependencies(join(desktopDir, "src-tauri", "Cargo.toml"), "1.96.0-x86_64-pc-windows-msvc", "x86_64-pc-windows-msvc")
];
const componentIndex = new Map(components.map((component) => [component.id, component]));

for (const component of components) {
  if (!component.text) component.text = resolveMissingLicenseText(component, componentIndex, new Set());
  component.text = normalizeText(component.text);
  if (!component.text) throw new Error(`Empty license text for ${component.id}`);
}

const groups = new Map();
for (const component of components) {
  const hash = createHash("sha256").update(component.text).digest("hex");
  const group = groups.get(hash) || { text: component.text, components: [] };
  group.components.push(`${component.id} [${component.license || "license file"}; text: ${component.source}; upstream: ${component.repository || "not declared"}]`);
  groups.set(hash, group);
}

const sections = [...groups.values()]
  .map((group) => ({ ...group, components: [...new Set(group.components)].sort() }))
  .sort((left, right) => left.components[0].localeCompare(right.components[0]));
if (sections.length === 0) throw new Error("No third-party license notices were generated");

const output = [
  "THIRD-PARTY COMPONENTS",
  "",
  "Convenient Window includes third-party software under the license terms reproduced below.",
  "The project license in LICENSE does not replace these third-party terms.",
  "Corresponding project source and locked dependency manifests are available at:",
  "https://github.com/ximizhou/convenient_window_free",
  "",
  ...sections.flatMap((section) => [
    "================================================================================",
    section.components.join("\n"),
    "--------------------------------------------------------------------------------",
    section.text,
    ""
  ])
].join("\n");

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, output, "utf8");
console.log(`Generated ${relative(repoRoot, outputPath)} for ${components.length} components in ${sections.length} license groups`);

function collectNpmRuntimeDependencies() {
  const lock = JSON.parse(readFileSync(join(desktopDir, "package-lock.json"), "utf8"));
  const components = [];
  for (const [key, entry] of Object.entries(lock.packages || {}).sort(([left], [right]) => left.localeCompare(right))) {
    if (!key.startsWith("node_modules/") || entry.dev === true || !entry.version) continue;
    const packageDir = join(desktopDir, key);
    const manifestPath = join(packageDir, "package.json");
    if (!existsSync(manifestPath)) throw new Error(`Installed npm package is missing: ${key}`);
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    const name = manifest.name || key.slice("node_modules/".length);
    const license = normalizeLicenseExpression(manifest.license || entry.license);
    components.push({
      id: `npm:${name}@${entry.version}`,
      license,
      repository: normalizeRepository(manifest.repository),
      source: "package files",
      text: readLicenseFiles(packageDir, license)
    });
  }
  return components;
}

function collectCargoDependencies(manifestPath, toolchain, target) {
  const result = spawnSync("rustup", [
    "run", toolchain, "cargo", "metadata", "--format-version", "1", "--locked",
    "--filter-platform", target, "--manifest-path", manifestPath
  ], { encoding: "utf8", windowsHide: true, maxBuffer: 64 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`cargo metadata failed for ${manifestPath}: ${result.stderr || result.stdout}`);

  const metadata = JSON.parse(result.stdout);
  const workspace = new Set(metadata.workspace_members || []);
  const nodes = new Map((metadata.resolve?.nodes || []).map((node) => [node.id, node]));
  const reachable = new Set();
  const queue = [...workspace];
  while (queue.length > 0) {
    const id = queue.pop();
    if (reachable.has(id)) continue;
    reachable.add(id);
    for (const dependency of nodes.get(id)?.deps || []) {
      if ((dependency.dep_kinds || []).some((kind) => kind.kind !== "dev")) queue.push(dependency.pkg);
    }
  }

  return metadata.packages
    .filter((pkg) => reachable.has(pkg.id) && !workspace.has(pkg.id) && pkg.source)
    .sort((left, right) => left.id.localeCompare(right.id))
    .map((pkg) => {
      const packageDir = dirname(pkg.manifest_path);
      const licenseFile = pkg.license_file ? resolve(packageDir, pkg.license_file) : null;
      return {
        id: `cargo:${pkg.name}@${pkg.version}`,
        license: normalizeLicenseExpression(pkg.license),
        repository: pkg.repository || "",
        source: licenseFile ? `declared ${relative(packageDir, licenseFile)}` : "package files",
        text: licenseFile ? readFileSync(licenseFile, "utf8") : readLicenseFiles(packageDir, pkg.license)
      };
    });
}

function readLicenseFiles(packageDir, licenseExpression) {
  const realDir = realpathSync(packageDir);
  const declared = /^SEE LICENSE IN (.+)$/i.exec(licenseExpression || "");
  if (declared) {
    const declaredRelative = declared[1].trim();
    const declaredPath = resolve(realDir, declaredRelative);
    const outsidePackage = relative(realDir, declaredPath).startsWith("..") || isAbsolute(relative(realDir, declaredPath));
    if (isAbsolute(declaredRelative) || outsidePackage) {
      throw new Error(`Declared license file escapes package directory: ${declaredRelative}`);
    }
    if (!existsSync(declaredPath) || !statSync(declaredPath).isFile()) {
      throw new Error(`Declared license file is missing: ${declaredPath}`);
    }
    return readFileSync(declaredPath, "utf8");
  }

  const paths = readdirSync(realDir)
    .filter((name) => /^(?:licen[cs]e|copying|copyright|notice|unlicense)(?:[._-]?.*)?$/i.test(name))
    .map((name) => join(realDir, name));
  const licensesDir = join(realDir, "LICENSES");
  if (existsSync(licensesDir) && statSync(licensesDir).isDirectory()) {
    paths.push(...readdirSync(licensesDir).sort().map((name) => join(licensesDir, name)));
  }
  const files = paths.filter((path) => statSync(path).isFile()).sort();
  if (files.length === 0) return null;
  return files
    .map((path) => readFileSync(path, "utf8").trim())
    .filter(Boolean)
    .join("\n\n--- Additional notice ---\n\n");
}

function resolveMissingLicenseText(component, componentIndex, resolving) {
  if (resolving.has(component.id)) throw new Error(`Circular license source mapping for ${component.id}`);
  resolving.add(component.id);
  const sourceId = missingLicenseSources.get(component.id);
  if (sourceId) {
    const source = componentIndex.get(sourceId);
    if (!source) throw new Error(`Mapped license source is unavailable for ${component.id}: ${sourceId}`);
    if (!source.text) source.text = resolveMissingLicenseText(source, componentIndex, resolving);
    component.source = `audited compatible text from locked component ${sourceId}`;
    resolving.delete(component.id);
    return source.text;
  }
  if (explicitUpstreamTexts.has(component.id)) {
    component.source = component.id.startsWith("npm:")
      ? "standard MIT terms; package declares MIT but publishes no copyright notice"
      : "audited text from the package repository declared in Cargo metadata";
    return explicitUpstreamTexts.get(component.id);
  }
  throw new Error(`No audited license source for ${component.id} (${component.license || "undeclared"})`);
}

function normalizeRepository(value) {
  if (typeof value === "string") return value;
  if (value && typeof value.url === "string") return value.url;
  return "";
}

function normalizeLicenseExpression(value) {
  if (typeof value === "string" && value.trim()) return value.trim().replaceAll("/", " OR ");
  if (value && typeof value.type === "string") return value.type.trim();
  return "";
}

function normalizeText(value) {
  return value.replace(/^\uFEFF/, "").replace(/\r\n/g, "\n").trim();
}

function rustUnicCopyright() {
  return `Copyright 2011-2015 The Rust Project developers.
Copyright 2013-2016 The rust-url developers.
Copyright 2015-2017 The Servo Project developers.
Copyright 2017 The UNIC Project developers.`;
}

function webview2LicenseText() {
  return `MIT License

Copyright (c) 2021 Bill Avery

${standardMitLicenseText()}`;
}

function standardMitLicenseText() {
  return `Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;
}
