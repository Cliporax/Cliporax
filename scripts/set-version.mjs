import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
const version = process.argv[2]?.trim();

if (!version || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error("Usage: node scripts/set-version.mjs <semver>, for example: 1.2.2");
}

function updateJson(relativePath, updater) {
  const path = join(repoRoot, relativePath);
  const json = JSON.parse(readFileSync(path, "utf8"));
  updater(json);
  writeFileSync(path, `${JSON.stringify(json, null, 2)}\n`);
}

function updateText(relativePath, updater) {
  const path = join(repoRoot, relativePath);
  const before = readFileSync(path, "utf8");
  const after = updater(before);
  if (after === before) {
    return;
  }
  writeFileSync(path, after);
}

updateJson("package.json", (json) => {
  json.version = version;
});

updateJson("package-lock.json", (json) => {
  json.version = version;
  if (json.packages?.[""]?.name === "cliporax") {
    json.packages[""].version = version;
  }
});

updateText("src-tauri/tauri.conf.json", (text) =>
  text.replace(/^  "version": ".*",$/m, `  "version": "${version}",`),
);

updateText("src-tauri/Cargo.toml", (text) =>
  text.replace(/^version = ".*"$/m, `version = "${version}"`),
);

updateText("src-tauri/Cargo.lock", (text) => {
  const blockPattern = /(\[\[package\]\]\nname = "cliporax"\nversion = ")[^"]+(")/;
  if (!blockPattern.test(text)) {
    throw new Error('Could not find the cliporax package entry in src-tauri/Cargo.lock');
  }
  return text.replace(blockPattern, `$1${version}$2`);
});

console.log(`Set Cliporax version to ${version}`);
