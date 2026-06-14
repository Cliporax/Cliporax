import { chmodSync, copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
const tauriDir = join(repoRoot, "src-tauri");
const release = process.argv.includes("--release");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: tauriDir,
    stdio: "inherit",
    shell: false,
    ...options,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with status ${result.status}`);
  }
}

function output(command, args) {
  const result = spawnSync(command, args, {
    cwd: tauriDir,
    encoding: "utf8",
    shell: false,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with status ${result.status}`);
  }

  return result.stdout;
}

const cargoArgs = ["build", "--bin", "cliporax-cli"];
if (release) {
  cargoArgs.splice(1, 0, "--release");
}

const rustcVersion = output("rustc", ["-vV"]);
const hostTriple = rustcVersion
  .split(/\r?\n/)
  .find((line) => line.startsWith("host: "))
  ?.slice("host: ".length)
  .trim();
const requestedTriple = process.env.CLIPORAX_BUILD_TARGET?.trim();
const triple = requestedTriple || hostTriple;

if (!triple) {
  throw new Error("Unable to determine Rust host target triple from rustc -vV");
}

if (requestedTriple) {
  cargoArgs.push("--target", requestedTriple);
}

const exe = process.platform === "win32" ? ".exe" : "";
const profile = release ? "release" : "debug";
const source = requestedTriple
  ? join(tauriDir, "target", requestedTriple, profile, `cliporax-cli${exe}`)
  : join(tauriDir, "target", profile, `cliporax-cli${exe}`);
const destinationDir = join(tauriDir, "bin");
const destination = join(destinationDir, `cliporax-cli-${triple}${exe}`);

mkdirSync(destinationDir, { recursive: true });
if (!existsSync(destination)) {
  writeFileSync(destination, "");
  if (process.platform !== "win32") {
    chmodSync(destination, 0o755);
  }
}

run("cargo", cargoArgs);
copyFileSync(source, destination);

console.log(`[CLI] Prepared external binary: ${destination}`);
