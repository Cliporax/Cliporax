import { spawn } from "node:child_process";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import process from "node:process";

const [, , application, tauriDriverPath, databasePath] = process.argv;
const serverUrl = "http://127.0.0.1:4444";
const elementKey = "element-6066-11e4-a52e-4f735466cecf";

if (!application || !tauriDriverPath || !databasePath) {
  console.error(
    "Usage: node tests/native-smoke/smoke.mjs <app-binary> <tauri-driver> <database>",
  );
  process.exit(2);
}

let sessionId = null;
let driverProcess = null;

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function request(method, path, body) {
  const response = await fetch(`${serverUrl}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  const payload = text ? JSON.parse(text) : {};
  if (!response.ok || payload.value?.error) {
    throw new Error(`${method} ${path} failed: ${text}`);
  }
  return payload.value;
}

async function waitForDriver() {
  const started = Date.now();
  while (Date.now() - started < 15_000) {
    try {
      await request("GET", "/status");
      return;
    } catch {
      await delay(250);
    }
  }
  throw new Error("Timed out waiting for tauri-driver");
}

async function executeSync(script, args = []) {
  return request("POST", `/session/${sessionId}/execute/sync`, {
    script,
    args,
  });
}

async function waitForFile(path, timeoutMs = 20_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (existsSync(path)) return;
    await delay(250);
  }
  throw new Error(`Timed out waiting for file: ${path}`);
}

async function waitForBodyText(pattern, timeoutMs = 20_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const text = await executeSync("return document.body.innerText;");
    if (pattern.test(text)) return text;
    await delay(250);
  }
  throw new Error(`Timed out waiting for body text matching ${pattern}`);
}

async function findElement(selector, timeoutMs = 10_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const element = await request("POST", `/session/${sessionId}/element`, {
        using: "css selector",
        value: selector,
      });
      return element[elementKey];
    } catch {
      await delay(250);
    }
  }
  throw new Error(`Timed out waiting for element: ${selector}`);
}

function seedDatabase() {
  const sql = `
    DELETE FROM clipboard_items;
    INSERT OR IGNORE INTO tabs (name, is_default, auto_capture)
    VALUES ('System Clipboard', 1, 1);
    WITH RECURSIVE seq(n) AS (
      SELECT 1
      UNION ALL
      SELECT n + 1 FROM seq WHERE n < 20
    )
    INSERT INTO clipboard_items (
      type, content, content_hash, tab_id, is_sensitive, is_pinned,
      display_order, created_at, updated_at
    )
    SELECT
      'text',
      printf('Native smoke item #%02d', n),
      printf('native-smoke-%02d', n),
      (SELECT id FROM tabs WHERE is_default = 1 ORDER BY id LIMIT 1),
      0, 0, n, datetime('now'), datetime('now')
    FROM seq;
  `;
  const result = spawnSync("sqlite3", [databasePath], {
    input: sql,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`Failed to seed smoke database: ${result.stderr}`);
  }
}

async function main() {
  driverProcess = spawn(tauriDriverPath, [], {
    stdio: ["ignore", "inherit", "inherit"],
  });

  driverProcess.on("exit", (code) => {
    if (sessionId) {
      console.error(`tauri-driver exited unexpectedly with code ${code}`);
      process.exit(1);
    }
  });

  await waitForDriver();

  const session = await request("POST", "/session", {
    capabilities: {
      alwaysMatch: {
        browserName: "wry",
        "tauri:options": { application },
      },
    },
  });
  sessionId = session.sessionId;

  await findElement('[data-testid="app-shell"]', 20_000);
  await waitForFile(databasePath);
  seedDatabase();
  await executeSync("window.location.reload();");
  await waitForBodyText(/Native smoke item #01/);

  await executeSync(`
    window.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'f',
      ctrlKey: true,
      bubbles: true
    }));
  `);
  const searchInput = await findElement('[data-testid="search-input"]');
  await request(
    "POST",
    `/session/${sessionId}/element/${searchInput}/value`,
    { text: "Native smoke item #01", value: [..."Native smoke item #01"] },
  );
  await waitForBodyText(/Native smoke item #01/);

  const handlesBefore = await request("GET", `/session/${sessionId}/window/handles`);
  const settingsButton = await findElement('[data-testid="settings-button"]');
  await request(
    "POST",
    `/session/${sessionId}/element/${settingsButton}/value`,
    { text: "\uE007", value: ["\uE007"] },
  );

  const started = Date.now();
  while (Date.now() - started < 10_000) {
    const handles = await request("GET", `/session/${sessionId}/window/handles`);
    if (handles.length > handlesBefore.length) return;
    await delay(250);
  }
  throw new Error("Settings window did not open");
}

try {
  await main();
  console.log("Native Tauri smoke passed.");
} finally {
  if (sessionId) {
    await request("DELETE", `/session/${sessionId}`).catch(() => {});
    sessionId = null;
  }
  if (driverProcess) {
    driverProcess.kill();
  }
}
