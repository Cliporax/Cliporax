import { copyFile, mkdir, readdir, readFile, rm, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const PLUGINS_DIR = fileURLToPath(new URL('../plugins/', import.meta.url));
const BUNDLE_DIR = fileURLToPath(new URL('../src-tauri/builtin-plugins/', import.meta.url));

async function pathExists(path) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function bundlePlugins() {
  await rm(BUNDLE_DIR, { recursive: true, force: true });
  await mkdir(BUNDLE_DIR, { recursive: true });

  const entries = await readdir(PLUGINS_DIR);
  let bundled = 0;

  for (const entry of entries) {
    const pluginPath = join(PLUGINS_DIR, entry);
    const manifestPath = join(pluginPath, 'manifest.json');
    const mainJsPath = join(pluginPath, 'main.js');

    if (!(await pathExists(manifestPath)) || !(await pathExists(mainJsPath))) {
      continue;
    }

    const manifest = JSON.parse(await readFile(manifestPath, 'utf-8'));
    const targetDir = join(BUNDLE_DIR, manifest.id || entry);
    await mkdir(targetDir, { recursive: true });
    await copyFile(manifestPath, join(targetDir, 'manifest.json'));
    await copyFile(mainJsPath, join(targetDir, 'main.js'));

    console.log(`[Plugins] Bundled plugin: ${manifest.id || entry}`);
    bundled++;
  }

  console.log(`[Plugins] Bundled ${bundled} plugins to ${BUNDLE_DIR}`);
}

bundlePlugins().catch((error) => {
  console.error('[Plugins] Failed to bundle plugins:', error);
  process.exitCode = 1;
});
