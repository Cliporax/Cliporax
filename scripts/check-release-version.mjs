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
const mismatches = Object.entries(versions).filter(([, version]) => version !== expected);

const tag = process.argv[2] || process.env.RELEASE_TAG || "";
if (tag && tag !== `v${expected}`) {
  mismatches.push(["release tag", tag]);
}

if (mismatches.length > 0) {
  for (const [source, version] of mismatches) {
    console.error(`${source}: ${version ?? "missing"}; expected ${source === "release tag" ? `v${expected}` : expected}`);
  }
  process.exit(1);
}

console.log(`Release version verified: ${expected}${tag ? ` (${tag})` : ""}`);
