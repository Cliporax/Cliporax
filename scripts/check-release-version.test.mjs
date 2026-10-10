import { strict as assert } from "node:assert";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";

function fixture(t, overrides = {}) {
  const root = mkdtempSync(join(tmpdir(), "cliporax-release-check-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const files = {
    "package.json": JSON.stringify({ version: "1.2.7" }),
    "package-lock.json": JSON.stringify({ version: "1.2.7", packages: { "": { version: "1.2.7" } } }),
    "src-tauri/tauri.conf.json": JSON.stringify({ version: "1.2.7" }),
    "src-tauri/Cargo.toml": '[package]\nname = "cliporax"\nversion = "1.2.7"\n',
    "src-tauri/Cargo.lock": '[[package]]\nname = "cliporax"\nversion = "1.2.7"\n',
    ...overrides,
  };
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), content);
  }
  mkdirSync(join(root, "scripts"));
  const script = join(root, "scripts/check-release-version.mjs");
  copyFileSync(new URL("./check-release-version.mjs", import.meta.url), script);
  return (args = [], env = {}) => spawnSync(process.execPath, [script, ...args], {
    encoding: "utf8",
    env: { ...process.env, RELEASE_TAG: "", GITHUB_REF: "", ...env },
  });
}

test("matching explicit and GitHub tags allow the release", (t) => {
  const run = fixture(t);
  const result = run(["--require-tag"], { GITHUB_REF: "refs/tags/v1.2.7", RELEASE_TAG: "v1.2.7" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Release version verified: 1.2.7 \(v1.2.7\)/);
  assert.equal(run(["v1.2.7"]).status, 0);
});

test("a mismatched tag fails whether supplied via CLI, RELEASE_TAG, or GitHub", (t) => {
  const run = fixture(t);
  for (const result of [run(["v1.2.8"]), run([], { RELEASE_TAG: "v1.2.8" }), run([], { GITHUB_REF: "refs/tags/v1.2.8" })]) {
    assert.equal(result.status, 1);
    assert.match(result.stderr, /release tag: v1.2.8; expected v1.2.7/);
  }
});

test("a matching override cannot hide a mismatched workflow tag", (t) => {
  const result = fixture(t)(["v1.2.7", "--require-tag"], { RELEASE_TAG: "v1.2.7", GITHUB_REF: "refs/tags/v1.2.8" });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /release tag: v1.2.8/);
});

test("required-tag mode rejects missing tags and manual runs on a branch", (t) => {
  const run = fixture(t);
  for (const env of [{}, { GITHUB_REF: "refs/heads/master" }]) {
    const result = run(["--require-tag"], env);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Release tag is required/);
  }
  // Local checks remain usable before creating a release tag.
  assert.equal(run().status, 0);
});

for (const [source, content] of Object.entries({
  "package-lock.json": JSON.stringify({ version: "1.2.8", packages: { "": { version: "1.2.7" } } }),
  "src-tauri/tauri.conf.json": JSON.stringify({ version: "1.2.8" }),
  "src-tauri/Cargo.toml": '[package]\nname = "cliporax"\nversion = "1.2.8"\n',
  "src-tauri/Cargo.lock": '[[package]]\nname = "cliporax"\nversion = "1.2.8"\n',
})) {
  test(`a matching tag cannot bypass an inconsistent ${source}`, (t) => {
    const result = fixture(t, { [source]: content })(["v1.2.7", "--require-tag"]);
    assert.equal(result.status, 1);
    assert.ok(result.stderr.includes(`${source}: 1.2.8; expected 1.2.7`), result.stderr);
  });
}

test("the root package version in package-lock must match too", (t) => {
  const result = fixture(t, { "package-lock.json": JSON.stringify({ version: "1.2.7", packages: { "": { version: "1.2.8" } } }) })(["v1.2.7"]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /package-lock.json root package: 1.2.8/);
});

test("missing versions fail closed", (t) => {
  const result = fixture(t, { "package.json": "{}" })();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /missing release version/);
});
