import { readdir, readFile, stat, mkdir, copyFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { homedir, platform } from 'node:os';

const PLUGINS_DIR = fileURLToPath(new URL('../plugins/', import.meta.url));
const APP_IDENTIFIER = 'com.cliporax.app';

function getRuntimePluginDir() {
  switch (platform()) {
    case 'linux': {
      const dataHome = process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share');
      return join(dataHome, APP_IDENTIFIER, 'plugins');
    }
    case 'darwin':
      return join(homedir(), 'Library', 'Application Support', APP_IDENTIFIER, 'plugins');
    case 'win32': {
      const appData = process.env.APPDATA;
      if (!appData) {
        throw new Error('APPDATA is not set; cannot resolve Tauri app data directory');
      }
      return join(appData, APP_IDENTIFIER, 'plugins');
    }
    default:
      throw new Error(`Unsupported platform: ${platform()}`);
  }
}

async function installPlugins() {
  const runtimeDir = getRuntimePluginDir();
  await mkdir(runtimeDir, { recursive: true });

  const entries = await readdir(PLUGINS_DIR);
  let installed = 0;

  for (const entry of entries) {
    const pluginPath = join(PLUGINS_DIR, entry);
    const manifestPath = join(pluginPath, 'manifest.json');
    const mainJsPath = join(pluginPath, 'main.js');

    try {
      await stat(pluginPath);
      await readFile(manifestPath, 'utf-8');
      await stat(mainJsPath);
    } catch {
      continue;
    }

    const targetDir = join(runtimeDir, entry);
    const manifest = JSON.parse(await readFile(manifestPath, 'utf-8'));
    if (manifest.isBuiltin === true) {
      console.log(`[Plugins] Skipped builtin dev plugin: ${manifest.id || entry}`);
      continue;
    }

    await mkdir(targetDir, { recursive: true });
    delete manifest.is_builtin;
    delete manifest.isBuiltin;
    await writeFile(join(targetDir, 'manifest.json'), JSON.stringify(manifest, null, 2));

    await copyFile(mainJsPath, join(targetDir, 'main.js'));

    console.log(`[Plugins] Installed dev plugin: ${entry}`);
    installed++;
  }

  console.log(`[Plugins] Installed ${installed} dev plugins to ${runtimeDir}`);
}

installPlugins().catch((error) => {
  console.error('[Plugins] Failed to install dev plugins:', error);
  process.exitCode = 1;
});
