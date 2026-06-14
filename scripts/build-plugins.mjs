import { readdir, readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { exec } from 'node:child_process';
import { promisify } from 'node:util';

const execAsync = promisify(exec);
const PLUGINS_DIR = fileURLToPath(new URL('../plugins/', import.meta.url));
const isRelease = process.argv.includes('--release');

async function pathExists(path) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function runPackageCommand(pluginPath, command) {
  const hasYarnLock = await pathExists(join(pluginPath, 'yarn.lock'));
  const runner = hasYarnLock ? 'yarn' : 'npm';
  const scriptCommand = runner === 'yarn' ? `yarn ${command}` : `npm run ${command}`;

  await execAsync(scriptCommand, { cwd: pluginPath });
}

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

    if (!(await pathExists(join(pluginPath, 'node_modules')))) {
      const hasYarnLock = await pathExists(join(pluginPath, 'yarn.lock'));
      await execAsync(hasYarnLock ? 'yarn install --frozen-lockfile' : 'npm install', {
        cwd: pluginPath,
      });
    }

    await runPackageCommand(pluginPath, isRelease ? 'build' : 'build:dev');
    console.log(`[Plugins] Built ${id}`);
  });

  await Promise.all(builds);
  console.log(`[Plugins] All ${isRelease ? 'release' : 'dev'} plugins built successfully`);
}

buildPlugins().catch((error) => {
  console.error('[Plugins] Failed to build dev plugins:', error);
  process.exitCode = 1;
});
