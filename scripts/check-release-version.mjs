import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const read = (path) => readFileSync(join(root, path), "utf8");
const packageJson = JSON.parse(read("package.json"));
const packageLock = JSON.parse(read("package-lock.json"));
const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
const cargoToml = read("src-tauri/Cargo.toml");
const cargoLock = read("src-tauri/Cargo.lock");

const cargoVersion = cargoToml.match(/^\[package\]\s*\nname = "cliporax"\s*\nversion = "([^"]+)"/m)?.[1];
const cargoLockVersion = cargoLock.match(/\[\[package\]\]\s*\nname = "cliporax"\s*\nversion = "([^"]+)"/m)?.[1];
const versions = {
  "package.json": packageJson.version,
  "package-lock.json": packageLock.version,
  "package-lock.json root package": packageLock.packages?.[""]?.version,
  "src-tauri/tauri.conf.json": tauriConfig.version,
  "src-tauri/Cargo.toml": cargoVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
};
const expected = packageJson.version;
if (typeof expected !== "string" || !expected.trim()) {
  console.error("package.json: missing release version");
  process.exit(1);
}
const mismatches = Object.entries(versions).filter(([, version]) => version !== expected);

const args = process.argv.slice(2);
const requireTag = args.includes("--require-tag");
const positionalArgs = args.filter((arg) => arg !== "--require-tag");
if (positionalArgs.length > 1 || positionalArgs.some((arg) => arg.startsWith("--"))) {
  console.error("Usage: node scripts/check-release-version.mjs [vX.Y.Z] [--require-tag]");
  process.exit(1);
}
const githubTag = process.env.GITHUB_REF?.startsWith("refs/tags/")
  ? process.env.GITHUB_REF.slice("refs/tags/".length)
  : "";
// Check every supplied tag: an argument or env override must never hide a
// mismatched tag from the actual GitHub workflow run.
const tags = [positionalArgs[0], process.env.RELEASE_TAG, githubTag].filter(Boolean);
const tag = tags[0] || "";
if (requireTag && !tag) {
  console.error(`Release tag is required; run this workflow on tag v${expected}. Branch builds cannot publish a release.`);
  process.exit(1);
}
for (const candidate of new Set(tags)) {
  if (candidate !== `v${expected}`) mismatches.push(["release tag", candidate]);
}

if (mismatches.length > 0) {
  for (const [source, version] of mismatches) {
    console.error(`${source}: ${version ?? "missing"}; expected ${source === "release tag" ? `v${expected}` : expected}`);
  }
  process.exit(1);
}

console.log(`Release version verified: ${expected}${tag ? ` (${tag})` : ""}`);
