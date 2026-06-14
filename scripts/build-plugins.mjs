import { readdir, readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { exec } from 'node:child_process';
import { promisify } from 'node:util';

const execAsync = promisify(exec);
const PLUGINS_DIR = fileURLToPath(new URL('../plugins/', import.meta.url));

async function buildPlugins() {
  const entries = await readdir(PLUGINS_DIR);
  const plugins = [];

  for (const entry of entries) {
    const pluginPath = join(PLUGINS_DIR, entry);
    const pkgPath = join(pluginPath, 'package.json');

    try {
      await stat(pluginPath);
      await readFile(pkgPath, 'utf-8');
      plugins.push(entry);
    } catch {
      continue;
    }
  }

  console.log(`[Plugins] Found ${plugins.length} plugins to build`);

  const builds = plugins.map(async (id) => {
    const pluginPath = join(PLUGINS_DIR, id);
    console.log(`[Plugins] Building ${id}...`);

    try {
      await stat(join(pluginPath, 'node_modules'));
    } catch {
      await execAsync('npm install', { cwd: pluginPath });
    }

    await execAsync('npm run build:dev', { cwd: pluginPath });
    console.log(`[Plugins] Built ${id}`);
  });

  await Promise.all(builds);
  console.log('[Plugins] All plugins built successfully');
}

buildPlugins().catch((error) => {
  console.error('[Plugins] Failed to build dev plugins:', error);
  process.exitCode = 1;
});
