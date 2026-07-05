import { spawn } from 'node:child_process'

const DEV_URL = process.env.CLIPORAX_DEV_URL ?? 'http://localhost:5173'
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm'
const isWindows = process.platform === 'win32'

function spawnNpm(args) {
  if (isWindows) {
    return spawn(process.env.ComSpec ?? 'cmd.exe', ['/d', '/s', '/c', [npmCommand, ...args].join(' ')], {
      stdio: 'inherit',
      env: process.env,
    })
  }

  return spawn(npmCommand, args, {
    stdio: 'inherit',
    env: process.env,
  })
}

function runNpm(args) {
  return new Promise((resolve, reject) => {
    const child = spawnNpm(args)

    child.on('error', reject)
    child.on('close', (code, signal) => {
      if (code === 0) {
        resolve()
        return
      }

      reject(new Error(`${npmCommand} ${args.join(' ')} failed with ${signal ?? `exit code ${code}`}`))
    })
  })
}

async function isDevServerAvailable() {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), 1_000)

  try {
    const response = await fetch(DEV_URL, {
      signal: controller.signal,
      cache: 'no-store',
    })
    return response.ok || response.status < 500
  } catch {
    return false
  } finally {
    clearTimeout(timeout)
  }
}

function holdForExistingDevServer() {
  console.log(`[tauri-before-dev] Reusing existing Vite dev server at ${DEV_URL}`)

  const keepAlive = setInterval(async () => {
    if (!(await isDevServerAvailable())) {
      console.error(`[tauri-before-dev] Existing Vite dev server is no longer reachable at ${DEV_URL}`)
      process.exit(1)
    }
  }, 5_000)

  const shutdown = () => {
    clearInterval(keepAlive)
    process.exit(0)
  }

  process.on('SIGINT', shutdown)
  process.on('SIGTERM', shutdown)
}

function startDevServer() {
  console.log(`[tauri-before-dev] Starting Vite dev server at ${DEV_URL}`)

  const child = spawnNpm(['run', 'dev'])

  child.on('error', (error) => {
    console.error(error)
    process.exit(1)
  })

  child.on('close', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal)
      return
    }
    process.exit(code ?? 1)
  })
}

await runNpm(['run', 'plugins:dev'])
await runNpm(['run', 'cli:prepare'])

if (await isDevServerAvailable()) {
  holdForExistingDevServer()
} else {
  startDevServer()
}
