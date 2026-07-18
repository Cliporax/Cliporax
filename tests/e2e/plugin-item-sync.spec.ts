import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "./fixtures/tauriMock";

const pluginRoot = resolve(process.cwd(), "../CliporaxPlugins/plugins");
const todoScriptPath = resolve(pluginRoot, "com.cliporax.todo/main.js");
const fileSyncScriptPath = resolve(pluginRoot, "com.cliporax.file-sync/main.js");
const TODO_STORAGE_KEY = "com.cliporax.todo:items";

const todoState = (text: string) => ({
  groups: [
    {
      id: "inbox",
      name: "Inbox",
      collapsed: false,
      createdAt: "2026-01-01T00:00:00.000Z",
      order: 0,
    },
  ],
  items: [
    {
      id: `todo-${text}`,
      text,
      completed: false,
      groupId: "inbox",
      createdAt: "2026-01-01T00:00:00.000Z",
      completedAt: null,
      updatedAt: null,
      order: 0,
    },
  ],
});

const fileEntry = (id: string, displayName: string) => ({
  id,
  profile_id: "profile-1",
  origin_device_id: "device-remote",
  kind: "file",
  display_name: displayName,
  total_size: 1024,
  file_count: 1,
  revision: 1,
  status: "remote",
  confirmed: true,
  progress_bytes: 1024,
  error: null,
  synced_at: "2026-01-01T00:00:00.000Z",
  created_at: "2026-01-01T00:00:00.000Z",
  updated_at: "2026-01-01T00:00:00.000Z",
});

test("Todo and File Sync items refresh when backend sync data changes", async ({
  page,
  mockTauri,
}) => {
  test.skip(
    !existsSync(todoScriptPath) || !existsSync(fileSyncScriptPath),
    "Plugin build output is missing",
  );

  await mockTauri({
    items: [],
    plugins: [
      {
        id: "com.cliporax.todo",
        name: "TODO",
        permissions: ["ui:extension", "system:storage"],
        extensions: [
          {
            point: "content-tab",
            component: "TodoView",
            icon: "list-todo",
            priority: 40,
          },
        ],
        script: readFileSync(todoScriptPath, "utf8"),
      },
      {
        id: "com.cliporax.file-sync",
        name: "File Sync",
        permissions: ["ui:extension"],
        extensions: [
          {
            point: "content-tab",
            component: "FileSyncView",
            icon: "upload",
            priority: 50,
          },
        ],
        script: readFileSync(fileSyncScriptPath, "utf8"),
      },
    ],
    pluginStorage: {
      "com.cliporax.todo": { [TODO_STORAGE_KEY]: todoState("Before sync") },
    },
    fileSyncProfiles: [
      {
        id: "profile-1",
        name: "Test profile",
        provider: "local_folder",
        encryption_enabled: false,
      },
    ],
    fileSyncConfig: {
      default_profile_id: "profile-1",
      confirmation_threshold_bytes: 0,
      chunk_size: 1024,
    },
    fileSyncEntries: [fileEntry("file-before", "before-sync.txt")],
  });

  await page.goto("/");
  await page.getByRole("button", { name: "TODO" }).click();
  await expect(page.getByText("Before sync")).toBeVisible();
  await page.getByRole("button", { name: "Create TODO item" }).click();
  await page.getByRole("textbox", { name: "Add TODO item" }).fill("Local item");
  await page.getByRole("textbox", { name: "Add TODO item" }).press("Enter");
  await expect.poll(() =>
    page.evaluate(() =>
      (window as any).__cliporaxTauriCalls
        .filter((call: { cmd: string }) => call.cmd === "plugin_storage_set")
        .at(-1)?.args.pluginId,
    ),
  ).toBe("com.cliporax.todo");

  await page.evaluate(
    ({ key, state }) => {
      (window as any).__setPluginStorage("com.cliporax.todo", key, state);
      (window as any).__emitTauriEvent("sync:completed", {
        profileId: "profile-1",
        report: { status: "completed" },
      });
    },
    { key: TODO_STORAGE_KEY, state: todoState("After sync") },
  );
  await expect(page.getByText("After sync")).toBeVisible();
  await expect(page.getByText("Before sync")).toHaveCount(0);

  await page.getByRole("button", { name: "File Sync" }).click();
  await expect(page.getByText("before-sync.txt")).toBeVisible();

  await page.evaluate((entry) => {
    (window as any).__setFileSyncEntries([entry]);
    (window as any).__emitTauriEvent("file-sync:changed", {
      entryIds: [entry.id],
      reason: "remote-refresh",
    });
  }, fileEntry("file-after", "after-sync.txt"));
  await expect(page.getByText("after-sync.txt")).toBeVisible();
  await expect(page.getByText("before-sync.txt")).toHaveCount(0);
});
