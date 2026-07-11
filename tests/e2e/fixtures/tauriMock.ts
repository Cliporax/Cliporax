import { promises as fs } from "node:fs";
import { test as base, expect, type Page } from "@playwright/test";

type ClipboardItem = {
  id: number;
  type: "text" | "image" | "file";
  content: string;
  content_hash: string | null;
  metadata: string | null;
  tags: string | null;
  tab_id: number;
  is_sensitive: boolean;
  is_pinned: boolean;
  display_order: number;
  created_at: string;
  updated_at: string;
};

type TauriMockOptions = {
  items?: ClipboardItem[];
  failCommands?: Record<string, string>;
  settings?: Partial<typeof defaultSettings>;
  plugins?: MockPlugin[];
};

type MockPlugin = {
  id: string;
  name: string;
  iconDataUrl?: string;
  version?: string;
  description?: string;
  permissions?: string[];
  grantedPermissions?: string[];
  extensions?: Array<{
    point: string;
    component: string;
    icon?: string;
    condition?: string;
    priority?: number;
  }>;
  script: string;
};

const defaultSettings = {
  theme: "dark",
  max_items: 1000,
  max_images: 500,
  line_height: "medium",
  auto_start: false,
  auto_hide: true,
  show_item_index: true,
  show_line_count: true,
  show_source_host: true,
  show_action_buttons: true,
  show_edit_button: true,
  show_pin_button: true,
  show_plugin_action_buttons: true,
  plugin_action_visibility: {},
  shortcut_toggle_window: "CmdOrControl+Shift+V",
};

export function makeClipboardItems(count: number): ClipboardItem[] {
  const now = new Date("2026-01-01T00:00:00.000Z").toISOString();
  return Array.from({ length: count }, (_, index) => ({
    id: index + 1,
    type: "text",
    content: `Mock clipboard item ${index + 1}`,
    content_hash: `hash-${index + 1}`,
    metadata: null,
    tags: null,
    tab_id: 1,
    is_sensitive: false,
    is_pinned: false,
    display_order: index,
    created_at: now,
    updated_at: now,
  }));
}

async function installTauriMock(page: Page, options: TauriMockOptions = {}) {
  await page.addInitScript(({ initialItems, failCommands, settings, plugins }) => {
    const listeners = new Map<string, Map<number, Function>>();
    let nextCallbackId = 1;
    let items = [...initialItems];
    let appSettings = { ...settings };
    const tabs = [
      {
        id: 1,
        name: "Clipboard",
        is_default: true,
        auto_capture: true,
        created_at: "2026-01-01T00:00:00.000Z",
      },
    ];

    const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));

    const invoke = async (cmd: string, args: Record<string, any> = {}) => {
      (window as any).__cliporaxTauriCalls.push({ cmd, args });
      if (failCommands[cmd]) {
        throw new Error(failCommands[cmd]);
      }

      if (cmd === "app_ready") return true;
      if (cmd === "dev_log_write") return null;
      if (cmd === "tabs_get_all") return clone(tabs);
      if (cmd === "tabs_create") return tabs.length + 1;
      if (cmd === "tabs_delete" || cmd === "tabs_rename") return null;

      if (cmd === "clipboard_get_total_count") {
        return items.filter((item) => item.tab_id === args.tabId).length;
      }
      if (cmd === "clipboard_get_by_tab") {
        const offset = args.offset ?? 0;
        const limit = args.limit ?? items.length;
        return clone(
          items
            .filter((item) => item.tab_id === args.tabId)
            .slice(offset, offset + limit),
        );
      }
      if (cmd === "clipboard_get_all_types") {
        return items
          .filter((item) => item.tab_id === args.tabId)
          .map((item) => [item.id, item.type]);
      }
      if (cmd === "clipboard_get_latest") {
        return clone(items.filter((item) => item.tab_id === args.tabId)[0] ?? null);
      }
      if (cmd === "clipboard_search") {
        const query = String(args.query ?? "").toLowerCase();
        const tabId = args.tabId;
        return clone(
          items.filter((item) => {
            const matchesTab = tabId === undefined || item.tab_id === tabId;
            return matchesTab && item.content.toLowerCase().includes(query);
          }),
        );
      }
      if (cmd === "clipboard_create") {
        const nextId = Math.max(0, ...items.map((item) => item.id)) + 1;
        const now = new Date().toISOString();
        items.unshift({
          ...args.item,
          id: nextId,
          tab_id: args.item.tab_id ?? 1,
          display_order: 0,
          created_at: now,
          updated_at: now,
        });
        return nextId;
      }
      if (cmd === "clipboard_delete") {
        items = items.filter((item) => item.id !== args.id);
        return null;
      }
      if (cmd === "clipboard_delete_by_ids") {
        const ids = new Set(args.ids ?? []);
        const before = items.length;
        items = items.filter((item) => !ids.has(item.id));
        return before - items.length;
      }
      if (cmd === "clipboard_delete_by_ids_permanently") {
        const ids = new Set(args.ids ?? []);
        const before = items.length;
        items = items.filter((item) => !ids.has(item.id));
        return before - items.length;
      }
      if (cmd === "clipboard_update_content") {
        items = items.map((item) =>
          item.id === args.id
            ? { ...item, content: args.content, updated_at: new Date().toISOString() }
            : item,
        );
        return null;
      }
      if (
        cmd === "clipboard_toggle_pin" ||
        cmd === "clipboard_move_to_top" ||
        cmd === "clipboard_copy" ||
        cmd === "clipboard_update_tags" ||
        cmd === "clipboard_clear_sensitive" ||
        cmd === "clipboard_move_item_to_position" ||
        cmd === "clipboard_move_to_tab" ||
        cmd === "clipboard_copy_to_tab" ||
        cmd === "clipboard_move_to_tab_batch" ||
        cmd === "clipboard_copy_to_tab_batch" ||
        cmd === "clipboard_delete_by_index_range"
      ) {
        return cmd === "clipboard_copy_to_tab" ? 1 : null;
      }

      if (cmd === "settings_get_all") return clone(appSettings);
      if (cmd === "settings_update") {
        appSettings = { ...appSettings, ...(args.newSettings ?? {}) };
        return null;
      }
      if (cmd === "settings_update_toggle_window_shortcut") {
        appSettings.shortcut_toggle_window = args.shortcut;
        return null;
      }

      if (
        cmd.startsWith("plugin_") ||
        cmd.startsWith("plugin:") ||
        cmd.startsWith("file_sync_") ||
        cmd.startsWith("sync_")
      ) {
        if (cmd === "plugin_get_all") {
          return plugins.map((plugin: any) => ({
            id: plugin.id,
            name: plugin.name,
            version: plugin.version ?? "0.1.0",
            description: plugin.description ?? "",
            author: "Test",
            state: "active",
            permissions: (plugin.permissions ?? []).map((permission: string) => ({
              permission,
              reason: "test",
              required: true,
            })),
            type: "utility",
            isBuiltin: false,
          }));
        }
        if (cmd === "plugin_get_detail") {
          const plugin = plugins.find((candidate: any) => candidate.id === args.pluginId);
          if (!plugin) throw new Error(`Plugin not found: ${args.pluginId}`);
          return {
            manifest: {
              id: plugin.id,
              name: plugin.name,
              version: plugin.version ?? "0.1.0",
              description: plugin.description ?? "",
            author: { name: "Test" },
            icon: plugin.iconDataUrl ? "assets/icon.svg" : undefined,
              main: "main.js",
              type: "utility",
              permissions: (plugin.permissions ?? []).map((permission: string) => ({
                permission,
                reason: "test",
                required: true,
              })),
              extensions: plugin.extensions ?? [],
            },
            state: "active",
            grantedPermissions: plugin.grantedPermissions ?? plugin.permissions ?? [],
            config: {},
            statistics: {
              activatedCount: 1,
              totalRuntimeMs: 0,
              errorCount: 0,
            },
          };
        }
        if (cmd === "plugin_read_script") {
          const plugin = plugins.find((candidate: any) => candidate.id === args.pluginId);
          if (!plugin) throw new Error(`Plugin not found: ${args.pluginId}`);
          return plugin.script;
        }
        if (cmd === "plugin_read_icon") {
          const plugin = plugins.find((candidate: any) => candidate.id === args.pluginId);
          if (!plugin?.iconDataUrl) throw new Error("Plugin icon not found");
          return plugin.iconDataUrl;
        }
        if (cmd.endsWith("get_all")) return [];
        if (cmd.includes("get_sources") || cmd.includes("get_plugins")) return [];
        if (cmd.includes("get_install_status")) return null;
        return null;
      }

      if (cmd === "window_open_settings") {
        window.location.assign("/settings");
        return null;
      }
      if (cmd.startsWith("test_")) return null;
      if (cmd.startsWith("shortcut_")) return true;
      if (cmd.startsWith("window_") || cmd.startsWith("plugin:window|")) return null;
      if (cmd === "plugin:event|emit") return null;
      if (cmd === "plugin:event|listen") return nextCallbackId++;
      if (cmd === "plugin:event|unlisten") return null;
      if (cmd === "plugin:shell|open") return null;

      return null;
    };

    (window as any).__cliporaxTauriCalls = [];
    (window as any).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" } },
      invoke,
      transformCallback: (callback: Function) => {
        const id = nextCallbackId++;
        (window as any)[`_${id}`] = callback;
        return id;
      },
      unregisterCallback: (id: number) => {
        delete (window as any)[`_${id}`];
      },
    };
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      registerListener: (event: string, id: number, handler: Function) => {
        if (!listeners.has(event)) listeners.set(event, new Map());
        listeners.get(event)!.set(id, handler);
      },
      unregisterListener: (event: string, id: number) => {
        listeners.get(event)?.delete(id);
      },
    };
  }, {
    initialItems: options.items ?? [],
    failCommands: options.failCommands ?? {},
    settings: { ...defaultSettings, ...options.settings },
    plugins: options.plugins ?? [],
  });
}

export const test = base.extend<{
  mockTauri: (options?: TauriMockOptions) => Promise<void>;
  debugArtifacts: void;
}>({
  debugArtifacts: [
    async ({ page }, use, testInfo) => {
      const consoleLines: string[] = [];

      page.on("console", (message) => {
        consoleLines.push(`[${message.type()}] ${message.text()}`);
      });
      page.on("pageerror", (error) => {
        consoleLines.push(`[pageerror] ${error.stack ?? error.message}`);
      });

      await use();

      if (testInfo.status !== testInfo.expectedStatus) {
        await fs.writeFile(
          testInfo.outputPath("console.log"),
          consoleLines.join("\n") || "No browser console messages captured.\n",
          "utf8",
        );

        try {
          await fs.writeFile(
            testInfo.outputPath("dom.html"),
            await page.content(),
            "utf8",
          );
        } catch (error) {
          await fs.writeFile(
            testInfo.outputPath("dom.html"),
            `Failed to capture DOM: ${String(error)}\n`,
            "utf8",
          );
        }
      }
    },
    { auto: true },
  ],
  mockTauri: async ({ page }, use) => {
    await use((options) => installTauriMock(page, options));
  },
});

export { expect };
