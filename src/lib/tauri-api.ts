import { invoke } from "@tauri-apps/api/core";
import { listen, Event } from "@tauri-apps/api/event";
import { tracedInvoke } from "../utils/traced-invoke";

// Import generated types
import type {
  Tab,
  ClipboardItem,
  ClipboardItemInput,
  ApiResult,
  ApiError,
} from "../types/generated/api";
import { ItemType } from "../types/generated/api";

// Re-export types for other modules
export type { Tab, ClipboardItem, ClipboardItemInput, ApiResult, ApiError };
export { ItemType };

export const CLIPBOARD_COUNT_CHANGED_EVENT = "clipboard:count-changed";

type ClipboardCountChangedDetail = {
  tabId?: number | null;
  reason: "create" | "delete" | "delete-range" | "delete-batch" | "clear-sensitive";
};

// Simple logger for API module
const log = (
  level: "info" | "debug" | "error",
  component: string,
  message: string,
  ...args: unknown[]
) => {
  const timestamp = new Date().toISOString();
  const formatted = `[${timestamp}] [${component}] ${level.toUpperCase()}: ${message}`;
  if (level === "error") {
    console.error(formatted, ...args);
  } else if (level === "debug") {
    console.debug(formatted, ...args);
  } else {
    console.log(formatted, ...args);
  }
};

function emitClipboardCountChanged(detail: ClipboardCountChangedDetail) {
  globalThis.dispatchEvent(
    new CustomEvent(CLIPBOARD_COUNT_CHANGED_EVENT, { detail }),
  );
}

// Internal numeric-tagged type used by the backend response format
interface ClipboardItemRaw {
  id?: number;
  type: "text" | "image" | "file";
  content: string;
  metadata?: string;
  tags?: string;
  tab_id?: number;
  is_sensitive?: number | boolean;
  is_pinned?: number | boolean;
  display_order?: number;
  created_at?: string;
  updated_at?: string;
}

function rawBool(value: number | boolean | null | undefined): boolean {
  return value === true || value === 1;
}

// Conversion function: backend format -> frontend format
function transformItem(item: ClipboardItemRaw): ClipboardItem {
  const itemType =
    item.type === "text"
      ? ItemType.Text
      : item.type === "image"
        ? ItemType.Image
        : ItemType.File;
  return {
    id: item.id ?? null,
    type: itemType,
    content: item.content,
    content_hash: null,
    metadata: item.metadata,
    tags: item.tags,
    tab_id: item.tab_id ?? null,
    is_sensitive: rawBool(item.is_sensitive),
    is_pinned: rawBool(item.is_pinned),
    display_order: item.display_order ?? null,
    created_at: item.created_at ?? null,
    updated_at: item.updated_at ?? null,
  };
}

// Tab API
export const tabs = {
  getAll: async (): Promise<Tab[]> => {
    log("info", "API", "tabs.getAll() called");
    try {
      const result = await invoke<Tab[]>("tabs_get_all");
      log("info", "API", "tabs.getAll() returned", result.length, "tabs");
      return result;
    } catch (error) {
      log("error", "API", "tabs.getAll() failed", error);
      throw error;
    }
  },
  create: async (name: string): Promise<number> => {
    log("info", "API", "tabs.create() called with name:", name);
    try {
      const result = await invoke<number>("tabs_create", { name });
      log("info", "API", "tabs.create() returned id:", result);
      return result;
    } catch (error) {
      log("error", "API", "tabs.create() failed", error);
      throw error;
    }
  },
  delete: async (id: number): Promise<void> => {
    log("info", "API", "tabs.delete() called with id:", id);
    try {
      await invoke<void>("tabs_delete", { id });
      log("info", "API", "tabs.delete() success");
    } catch (error) {
      log("error", "API", "tabs.delete() failed", error);
      throw error;
    }
  },
};

// Clipboard API
export const clipboard = {
  getByTab: async (
    tabId: number,
    limit?: number,
    offset?: number,
  ): Promise<ClipboardItem[]> => {
    log(
      "info",
      "API",
      "clipboard.getByTab() called - tabId:",
      tabId,
      "limit:",
      limit,
      "offset:",
      offset,
    );
    try {
      const result = await tracedInvoke<ClipboardItemRaw[]>(
        "clipboard_get_by_tab",
        {
          tabId,
          limit,
          offset,
        },
      );
      const transformed = result.map(transformItem);
      log(
        "info",
        "API",
        "clipboard.getByTab() returned",
        transformed.length,
        "items",
      );
      if (transformed.length > 0) {
        log(
          "debug",
          "API",
          "First item:",
          JSON.stringify(transformed[0]).substring(0, 200),
        );
      }
      return transformed;
    } catch (error) {
      log("error", "API", "clipboard.getByTab() failed", error);
      throw error;
    }
  },
  /// Get the latest clipboard item for incremental updates
  getLatest: async (tabId: number): Promise<ClipboardItem | null> => {
    log("info", "API", "clipboard.getLatest() called - tabId:", tabId);
    try {
      const result = await tracedInvoke<ClipboardItemRaw | null>(
        "clipboard_get_latest",
        {
          tabId,
        },
      );
      if (result) {
        const transformed = transformItem(result);
        log(
          "info",
          "API",
          "clipboard.getLatest() returned item id:",
          transformed.id,
          "type:",
          transformed.type,
        );
        return transformed;
      } else {
        log("info", "API", "clipboard.getLatest() returned no items");
        return null;
      }
    } catch (error) {
      log("error", "API", "clipboard.getLatest() failed", error);
      throw error;
    }
  },
  create: async (item: ClipboardItemInput): Promise<number> => {
    log(
      "info",
      "API",
      "clipboard.create() called - type:",
      item.type,
      "contentLen:",
      item.content.length,
    );
    try {
      const result = await tracedInvoke<number>("clipboard_create", { item });
      log("info", "API", "clipboard.create() returned id:", result);
      emitClipboardCountChanged({ tabId: item.tab_id, reason: "create" });
      return result;
    } catch (error) {
      log("error", "API", "clipboard.create() failed", error);
      throw error;
    }
  },
  delete: async (id: number): Promise<void> => {
    log("info", "API", "clipboard.delete() called with id:", id);
    try {
      await tracedInvoke<void>("clipboard_delete", { id });
      log("info", "API", "clipboard.delete() success");
      emitClipboardCountChanged({ reason: "delete" });
    } catch (error) {
      log("error", "API", "clipboard.delete() failed", error);
      throw error;
    }
  },
  deleteByIds: async (ids: number[]): Promise<number> => {
    log("info", "API", "clipboard.deleteByIds() called - count:", ids.length);
    try {
      const result = await tracedInvoke<number>("clipboard_delete_by_ids", {
        ids,
      });
      log("info", "API", "clipboard.deleteByIds() deleted", result, "items");
      emitClipboardCountChanged({ reason: "delete-batch" });
      return result;
    } catch (error) {
      log("error", "API", "clipboard.deleteByIds() failed", error);
      throw error;
    }
  },
  togglePin: async (id: number, isPinned: number): Promise<void> => {
    log(
      "info",
      "API",
      "clipboard.togglePin() called - id:",
      id,
      "isPinned:",
      isPinned,
    );
    try {
      await tracedInvoke<void>("clipboard_toggle_pin", { id, isPinned });
      log("info", "API", "clipboard.togglePin() success");
    } catch (error) {
      log("error", "API", "clipboard.togglePin() failed", error);
      throw error;
    }
  },
  moveToTop: async (id: number): Promise<void> => {
    log("info", "API", "clipboard.moveToTop() called with id:", id);
    try {
      await tracedInvoke<void>("clipboard_move_to_top", { id });
      log("info", "API", "clipboard.moveToTop() success");
    } catch (error) {
      log("error", "API", "clipboard.moveToTop() failed", error);
      throw error;
    }
  },
  search: async (query: string, tabId?: number): Promise<ClipboardItem[]> => {
    log(
      "info",
      "API",
      "clipboard.search() called - query:",
      query,
      "tabId:",
      tabId,
    );
    try {
      const result = await tracedInvoke<ClipboardItemRaw[]>(
        "clipboard_search",
        {
          query,
          tabId,
        },
      );
      const transformed = result.map(transformItem);
      log(
        "info",
        "API",
        "clipboard.search() returned",
        transformed.length,
        "items",
      );
      return transformed;
    } catch (error) {
      log("error", "API", "clipboard.search() failed", error);
      throw error;
    }
  },
  updateTags: async (id: number, tags: string[]): Promise<void> => {
    log(
      "info",
      "API",
      "clipboard.updateTags() called - id:",
      id,
      "tags:",
      tags,
    );
    try {
      await tracedInvoke<void>("clipboard_update_tags", { id, tags });
      log("info", "API", "clipboard.updateTags() success");
    } catch (error) {
      log("error", "API", "clipboard.updateTags() failed", error);
      throw error;
    }
  },
  updateContent: async (id: number, content: string): Promise<void> => {
    log(
      "info",
      "API",
      "clipboard.updateContent() called - id:",
      id,
      "contentLen:",
      content.length,
    );
    try {
      await tracedInvoke<void>("clipboard_update_content", { id, content });
      log("info", "API", "clipboard.updateContent() success");
    } catch (error) {
      log("error", "API", "clipboard.updateContent() failed", error);
      throw error;
    }
  },
  clearSensitive: async (): Promise<void> => {
    log("info", "API", "clipboard.clearSensitive() called");
    try {
      await tracedInvoke<void>("clipboard_clear_sensitive");
      log("info", "API", "clipboard.clearSensitive() success");
      emitClipboardCountChanged({ reason: "clear-sensitive" });
    } catch (error) {
      log("error", "API", "clipboard.clearSensitive() failed", error);
      throw error;
    }
  },
  copy: async (
    content: string,
    type: "text" | "image" | "file",
  ): Promise<void> => {
    log(
      "info",
      "API",
      "clipboard.copy() called - type:",
      type,
      "contentLen:",
      content.length,
    );
    try {
      console.log("[API] Starting clipboard.copy invoke...");
      const startTime = Date.now();
      await tracedInvoke<void>("clipboard_copy", { content, itemType: type });
      const elapsed = Date.now() - startTime;
      console.log("[API] clipboard.copy invoke completed in", elapsed, "ms");
      log("info", "API", "clipboard.copy() success");
    } catch (error) {
      console.error("[API] clipboard.copy invoke failed:", error);
      log("error", "API", "clipboard.copy() failed", error);
      throw error;
    }
  },
  /// Get total count of clipboard items for a tab
  getTotalCount: async (tabId: number): Promise<number> => {
    log("info", "API", "clipboard.getTotalCount() called - tabId:", tabId);
    try {
      const result = await invoke<number>("clipboard_get_total_count", {
        tabId,
      });
      log("info", "API", "clipboard.getTotalCount() returned:", result);
      return result;
    } catch (error) {
      log("error", "API", "clipboard.getTotalCount() failed", error);
      throw error;
    }
  },
  /// Get a single clipboard item at a specific index
  getItemAtIndex: async (
    tabId: number,
    index: number,
  ): Promise<ClipboardItem | null> => {
    log(
      "info",
      "API",
      "clipboard.getItemAtIndex() called - tabId:",
      tabId,
      "index:",
      index,
    );
    try {
      const result = await invoke<ClipboardItemRaw | null>(
        "clipboard_get_item_at_index",
        {
          tabId,
          index,
        },
      );
      if (result) {
        const transformed = transformItem(result);
        log(
          "info",
          "API",
          "clipboard.getItemAtIndex() returned item id:",
          transformed.id,
        );
        return transformed;
      } else {
        log("info", "API", "clipboard.getItemAtIndex() returned no item");
        return null;
      }
    } catch (error) {
      log("error", "API", "clipboard.getItemAtIndex() failed", error);
      throw error;
    }
  },
  /// Delete items by index range
  deleteByIndexRange: async (
    tabId: number,
    startIndex: number,
    endIndex: number,
  ): Promise<number> => {
    log(
      "info",
      "API",
      "clipboard.deleteByIndexRange() called - tabId:",
      tabId,
      "start:",
      startIndex,
      "end:",
      endIndex,
    );
    try {
      const result = await invoke<number>("clipboard_delete_by_index_range", {
        tabId,
        startIndex,
        endIndex,
      });
      log(
        "info",
        "API",
        "clipboard.deleteByIndexRange() deleted",
        result,
        "items",
      );
      emitClipboardCountChanged({ tabId, reason: "delete-range" });
      return result;
    } catch (error) {
      log("error", "API", "clipboard.deleteByIndexRange() failed", error);
      throw error;
    }
  },
  /// Get all item types for a tab (for virtual scrolling height calculation)
  getAllTypes: async (tabId: number): Promise<Array<[number, string]>> => {
    log("info", "API", "clipboard.getAllTypes() called - tabId:", tabId);
    try {
      const result = await invoke<Array<[number, string]>>(
        "clipboard_get_all_types",
        {
          tabId,
        },
      );
      log(
        "info",
        "API",
        "clipboard.getAllTypes() returned",
        result.length,
        "items",
      );
      return result;
    } catch (error) {
      log("error", "API", "clipboard.getAllTypes() failed", error);
      throw error;
    }
  },
  /// Move an item to a new position within the same pin group
  moveItemToPosition: async (
    tabId: number,
    itemId: number,
    fromIndex: number,
    toIndex: number,
  ): Promise<boolean> => {
    log(
      "info",
      "API",
      "clipboard.moveItemToPosition() called - tabId:",
      tabId,
      "itemId:",
      itemId,
      "from:",
      fromIndex,
      "to:",
      toIndex,
    );
    try {
      const result = await invoke<boolean>("clipboard_move_item_to_position", {
        tabId,
        itemId,
        fromIndex,
        toIndex,
      });
      log("info", "API", "clipboard.moveItemToPosition() returned:", result);
      return result;
    } catch (error) {
      log("error", "API", "clipboard.moveItemToPosition() failed", error);
      throw error;
    }
  },
  /// Move a single item to another tab
  moveToTab: async (itemId: number, targetTabId: number): Promise<void> => {
    log("info", "API", "clipboard.moveToTab() called - itemId:", itemId, "targetTabId:", targetTabId);
    try {
      await invoke<void>("clipboard_move_to_tab", { itemId, targetTabId });
      log("info", "API", "clipboard.moveToTab() success");
    } catch (error) {
      log("error", "API", "clipboard.moveToTab() failed", error);
      throw error;
    }
  },
  /// Copy a single item to another tab
  copyToTab: async (itemId: number, targetTabId: number): Promise<number> => {
    log("info", "API", "clipboard.copyToTab() called - itemId:", itemId, "targetTabId:", targetTabId);
    try {
      const result = await invoke<number>("clipboard_copy_to_tab", { itemId, targetTabId });
      log("info", "API", "clipboard.copyToTab() success, newId:", result);
      return result;
    } catch (error) {
      log("error", "API", "clipboard.copyToTab() failed", error);
      throw error;
    }
  },
  /// Move multiple items to another tab (batch)
  moveToTabBatch: async (ids: number[], targetTabId: number): Promise<number> => {
    log(
      "info",
      "API",
      "clipboard.moveToTabBatch() called - count:",
      ids.length,
      "targetTabId:",
      targetTabId,
    );
    try {
      const result = await invoke<number>("clipboard_move_to_tab_batch", {
        ids,
        targetTabId,
      });
      log("info", "API", "clipboard.moveToTabBatch() moved", result, "items");
      return result;
    } catch (error) {
      log("error", "API", "clipboard.moveToTabBatch() failed", error);
      throw error;
    }
  },
  /// Copy multiple items to another tab (batch)
  copyToTabBatch: async (ids: number[], targetTabId: number): Promise<number> => {
    log(
      "info",
      "API",
      "clipboard.copyToTabBatch() called - count:",
      ids.length,
      "targetTabId:",
      targetTabId,
    );
    try {
      const result = await invoke<number>("clipboard_copy_to_tab_batch", {
        ids,
        targetTabId,
      });
      log("info", "API", "clipboard.copyToTabBatch() copied", result, "items");
      return result;
    } catch (error) {
      log("error", "API", "clipboard.copyToTabBatch() failed", error);
      throw error;
    }
  },
};

// Window control API
export const window = {
  openSettings: async (): Promise<void> => {
    log("info", "API", "window.openSettings() called");
    try {
      await invoke<void>("window_open_settings");
      log("info", "API", "window.openSettings() success");
    } catch (error) {
      log("error", "API", "window.openSettings() failed", error);
      throw error;
    }
  },
  minimize: async (): Promise<void> => {
    log("debug", "API", "window.minimize() called");
    try {
      await invoke<void>("window_minimize");
    } catch (error) {
      log("error", "API", "window.minimize() failed", error);
      throw error;
    }
  },
  maximize: async (): Promise<void> => {
    log("debug", "API", "window.maximize() called");
    try {
      await invoke<void>("window_maximize");
    } catch (error) {
      log("error", "API", "window.maximize() failed", error);
      throw error;
    }
  },
  close: async (): Promise<void> => {
    log("debug", "API", "window.close() called");
    try {
      await invoke<void>("window_close");
    } catch (error) {
      log("error", "API", "window.close() failed", error);
      throw error;
    }
  },
  show: async (): Promise<void> => {
    log("debug", "API", "window.show() called");
    try {
      await invoke<void>("window_show");
    } catch (error) {
      log("error", "API", "window.show() failed", error);
      throw error;
    }
  },
  hide: async (): Promise<void> => {
    log("debug", "API", "window.hide() called");
    try {
      await invoke<void>("window_hide");
    } catch (error) {
      log("error", "API", "window.hide() failed", error);
      throw error;
    }
  },
  toggle: async (): Promise<void> => {
    log("debug", "API", "window.toggle() called");
    try {
      await invoke<void>("window_toggle");
    } catch (error) {
      log("error", "API", "window.toggle() failed", error);
      throw error;
    }
  },
  isMaximized: async (): Promise<boolean> => {
    log("debug", "API", "window.isMaximized() called");
    try {
      const result = await invoke<boolean>("window_is_maximized");
      return result;
    } catch (error) {
      log("error", "API", "window.isMaximized() failed", error);
      throw error;
    }
  },
  setAlwaysOnTop: async (alwaysOnTop: boolean): Promise<void> => {
    log("info", "API", "window.setAlwaysOnTop() called with:", alwaysOnTop);
    try {
      await invoke<void>("window_set_always_on_top", { alwaysOnTop });
      log("info", "API", "window.setAlwaysOnTop() success");
    } catch (error) {
      log("error", "API", "window.setAlwaysOnTop() failed", error);
      throw error;
    }
  },
  restoreAndPaste: async (): Promise<void> => {
    log("info", "API", "window.restoreAndPaste() called");
    try {
      await invoke<void>("window_restore_and_paste");
      log("info", "API", "window.restoreAndPaste() success");
    } catch (error) {
      log("error", "API", "window.restoreAndPaste() failed", error);
      throw error;
    }
  },
  restoreFocus: async (): Promise<void> => {
    log("info", "API", "window.restoreFocus() called");
    try {
      await invoke<void>("window_restore_focus");
      log("info", "API", "window.restoreFocus() success");
    } catch (error) {
      log("error", "API", "window.restoreFocus() failed", error);
      throw error;
    }
  },
  simulatePaste: async (): Promise<void> => {
    log("info", "API", "window.simulatePaste() called");
    try {
      await invoke<void>("window_simulate_paste");
      log("info", "API", "window.simulatePaste() success");
    } catch (error) {
      log("error", "API", "window.simulatePaste() failed", error);
      throw error;
    }
  },
  hideAndPaste: async (): Promise<void> => {
    log("info", "API", "window.hideAndPaste() called");
    try {
      await invoke<void>("window_hide_and_paste");
      log("info", "API", "window.hideAndPaste() success");
    } catch (error) {
      log("error", "API", "window.hideAndPaste() failed", error);
      throw error;
    }
  },
  checkMacosPermissions: async (): Promise<boolean> => {
    log("info", "API", "window.checkMacosPermissions() called");
    try {
      const result = await invoke<boolean>("check_macos_permissions");
      log("info", "API", "window.checkMacosPermissions() success:", result);
      return result;
    } catch (error) {
      log("error", "API", "window.checkMacosPermissions() failed", error);
      throw error;
    }
  },
  pasteToPrevious: async (): Promise<void> => {
    log("info", "API", "window.pasteToPrevious() called");
    try {
      await invoke<void>("window_paste_to_previous");
      log("info", "API", "window.pasteToPrevious() success");
    } catch (error) {
      log("error", "API", "window.pasteToPrevious() failed", error);
      throw error;
    }
  },
  startDragging: async (): Promise<void> => {
    log("debug", "API", "window.startDragging() called");
    try {
      await invoke<void>("window_start_dragging");
      log("debug", "API", "window.startDragging() success");
    } catch (error) {
      log("error", "API", "window.startDragging() failed", error);
      throw error;
    }
  },
  setContextMenuOpen: async (open: boolean): Promise<void> => {
    log("debug", "API", "window.setContextMenuOpen() called with:", open);
    try {
      await invoke<void>("window_set_context_menu_open", { open });
    } catch (error) {
      log("error", "API", "window.setContextMenuOpen() failed", error);
    }
  },
  endDragging: async (): Promise<void> => {
    log("debug", "API", "window.endDragging() called");
    try {
      await invoke<void>("window_end_dragging");
      log("debug", "API", "window.endDragging() success");
    } catch (error) {
      log("error", "API", "window.endDragging() failed", error);
      throw error;
    }
  },

  // Unified window command API (preferred for new code)
  // Actions: minimize, maximize, restore, close, show, hide, toggle,
  //          setAlwaysOnTop, startDragging, endDragging, setContextMenuOpen,
  //          hideAndPaste, pasteToPrevious, restoreFocus, simulatePaste
  command: async (action: WindowAction): Promise<void> => {
    log("debug", "API", "window.command() called with:", action);
    try {
      await invoke<void>("window_command", { action });
      log("debug", "API", "window.command() success");
    } catch (error) {
      log("error", "API", "window.command() failed", error);
      throw error;
    }
  },
};

/// Window action type for unified command API
export type WindowAction =
  | "minimize"
  | "maximize"
  | "restore"
  | "close"
  | "show"
  | "hide"
  | "toggle"
  | { setAlwaysOnTop: boolean }
  | "startDragging"
  | "endDragging"
  | { setContextMenuOpen: boolean }
  | "hideAndPaste"
  | "pasteToPrevious"
  | "restoreFocus"
  | "simulatePaste";

// Event listeners
export interface ClipboardChangedPayload {
  tabIds?: number[];
  itemIds?: number[];
  reason?: string;
}

export const events = {
  onClipboardChanged: (
    callback: (payload: ClipboardChangedPayload | null) => void,
  ): Promise<() => void> => {
    log("info", "API", "events.onClipboardChanged() registering");
    return listen<ClipboardChangedPayload>("clipboard:changed", (event) => {
      log("info", "API", "clipboard:changed event received", event.payload);
      callback(event.payload ?? null);
    }).then((unlisten) => {
      log("debug", "API", "clipboard:changed listener registered");
      return unlisten;
    });
  },
};

// Shortcut API
export const shortcut = {
  update: async (
    oldShortcut: string,
    newShortcut: string,
  ): Promise<boolean> => {
    log(
      "info",
      "API",
      "shortcut.update() called - old:",
      oldShortcut,
      "new:",
      newShortcut,
    );
    try {
      const result = await invoke<boolean>("shortcut_update", {
        oldShortcut,
        newShortcut,
      });
      log("info", "API", "shortcut.update() success:", result);
      return result;
    } catch (error) {
      log("error", "API", "shortcut.update() failed", error);
      throw error;
    }
  },

  /**
   * Temporarily pause (unregister) the global toggle shortcut.
   * Use this when entering shortcut recording mode to prevent
   * the shortcut from triggering window hide/show during recording.
   */
  pause: async (shortcutStr: string): Promise<boolean> => {
    log("info", "API", "shortcut.pause() called:", shortcutStr);
    try {
      const result = await invoke<boolean>("shortcut_pause", {
        shortcutStr,
      });
      log("info", "API", "shortcut.pause() success:", result);
      return result;
    } catch (error) {
      log("error", "API", "shortcut.pause() failed", error);
      throw error;
    }
  },

  /**
   * Resume (re-register) the global toggle shortcut.
   * Use this after shortcut recording is complete.
   */
  resume: async (shortcutStr: string): Promise<boolean> => {
    log("info", "API", "shortcut.resume() called:", shortcutStr);
    try {
      const result = await invoke<boolean>("shortcut_resume", {
        shortcutStr,
      });
      log("info", "API", "shortcut.resume() success:", result);
      return result;
    } catch (error) {
      log("error", "API", "shortcut.resume() failed", error);
      throw error;
    }
  },
};

// Settings API
export interface AppSettings {
  theme: string;
  max_items: number;
  max_images: number;
  line_height: string;
  auto_start: boolean;
  auto_hide: boolean;
  shortcut_toggle_window: string;
}

export const settings = {
  getAll: async (): Promise<AppSettings> => {
    log("info", "API", "settings.getAll() called");
    try {
      const result = await tracedInvoke<AppSettings>("settings_get_all");
      log("info", "API", "settings.getAll() success:", result);
      return result;
    } catch (error) {
      log("error", "API", "settings.getAll() failed", error);
      throw error;
    }
  },
  update: async (newSettings: Partial<AppSettings>): Promise<void> => {
    // Get current settings and merge updates
    // The frontend sometimes sends only partial fields, such as a theme toggle, so fetch full settings first
    const current = await settings.getAll();
    const merged = { ...current, ...newSettings } as AppSettings;
    log(
      "info",
      "API",
      "settings.update() called:",
      JSON.stringify({
        line_height: merged.line_height,
        input_keys: Object.keys(newSettings),
      }),
    );
    try {
      await tracedInvoke("settings_update", { newSettings: merged });
      log("info", "API", "settings.update() success");
    } catch (error) {
      log("error", "API", "settings.update() failed", error);
      throw error;
    }
  },
  /**
   * Update settings with a complete AppSettings object without calling getAll first
   * Used when the frontend already has complete settings data, such as in the Settings component
   * Benefit: removes one IPC round trip and avoids Mutex lock contention during rapid consecutive actions
   */
  updateFull: async (newSettings: AppSettings): Promise<void> => {
    log(
      "info",
      "API",
      "settings.updateFull() called:",
      JSON.stringify({
        line_height: newSettings.line_height,
      }),
    );
    try {
      await tracedInvoke("settings_update", { newSettings });
      log("info", "API", "settings.updateFull() success");
    } catch (error) {
      log("error", "API", "settings.updateFull() failed", error);
      throw error;
    }
  },
  updateToggleWindowShortcut: async (shortcut: string): Promise<void> => {
    log(
      "info",
      "API",
      "settings.updateToggleWindowShortcut() called:",
      shortcut,
    );
    try {
      await tracedInvoke("settings_update_toggle_window_shortcut", {
        shortcut,
      });
      log("info", "API", "settings.updateToggleWindowShortcut() success");
    } catch (error) {
      log(
        "error",
        "API",
        "settings.updateToggleWindowShortcut() failed",
        error,
      );
      throw error;
    }
  },
};

// Test API (for performance testing)
export const test = {
  /**
   * Insert batch test data into the database
   * @param count Number of items to insert
   * @returns Number of items actually inserted
   */
  insertBatch: async (count: number): Promise<number> => {
    log("info", "API", "test.insertBatch() called - count:", count);
    try {
      const startTime = Date.now();
      const result = await invoke<number>("test_insert_batch", { count });
      const elapsed = Date.now() - startTime;
      log(
        "info",
        "API",
        `test.insertBatch() success - inserted ${result} items in ${elapsed}ms`,
      );
      return result;
    } catch (error) {
      log("error", "API", "test.insertBatch() failed", error);
      throw error;
    }
  },

  /**
   * Clear all clipboard items from the database
   */
  clearAll: async (): Promise<void> => {
    log("info", "API", "test.clearAll() called");
    try {
      await invoke<void>("test_clear_all");
      log("info", "API", "test.clearAll() success");
    } catch (error) {
      log("error", "API", "test.clearAll() failed", error);
      throw error;
    }
  },
};

// Preview window API
export const preview = {
  /**
   * Create a new preview window for displaying an image
   * @param imageData Base64 encoded image data URL
   * @param title Window title
   * @returns Window label
   */
  create: async (imageData: string, title: string): Promise<string> => {
    log("info", "API", "preview.create() called - title:", title);
    try {
      const label = await invoke<string>("preview_create_window", {
        imageData,
        title,
      });
      log("info", "API", "preview.create() success - label:", label);
      return label;
    } catch (error) {
      log("error", "API", "preview.create() failed", error);
      throw error;
    }
  },

  /**
   * Close a specific preview window
   * @param label Window label to close
   */
  close: async (label: string): Promise<void> => {
    log("info", "API", "preview.close() called - label:", label);
    try {
      await invoke<void>("preview_close_window", { label });
      log("info", "API", "preview.close() success");
    } catch (error) {
      log("error", "API", "preview.close() failed", error);
      throw error;
    }
  },

  /**
   * Close all preview windows
   */
  closeAll: async (): Promise<void> => {
    log("info", "API", "preview.closeAll() called");
    try {
      await invoke<void>("preview_close_all");
      log("info", "API", "preview.closeAll() success");
    } catch (error) {
      log("error", "API", "preview.closeAll() failed", error);
      throw error;
    }
  },
};

// Sync types (Cloud Sync plugin)
export interface SyncProfileSummary {
  id: string;
  name: string;
  provider: "webdav" | "sftp" | "google_drive" | "one_drive";
  remote_root: string;
  encryption_enabled: boolean;
  last_sync_at: string | null;
  status: string;
}

export interface SyncProfile {
  id: string;
  name: string;
  provider: "webdav" | "sftp" | "google_drive" | "one_drive";
  remote_root: string;
  sync_tabs: TabSyncSelection;
  sync_plugins: PluginSyncSelection;
  encryption: EncryptionConfig;
  credential_refs: CredentialRefs;
  schedule: SyncScheduleConfig;
  created_at: string | null;
  updated_at: string | null;
}

export interface TabSyncSelection {
  mode: "all" | "selected";
  selected_tab_ids: number[];
}

export interface PluginSyncSelection {
  mode: "selected";
  selected_plugin_ids: string[];
  include_plugin_bundles: boolean;
  include_granted_permissions: boolean;
}

export interface EncryptionConfig {
  enabled: boolean;
  algorithm: string;
  kdf: string;
  salt_b64?: string | null;
  memory_kb?: number;
  iterations?: number;
  parallelism?: number;
}

export interface CredentialRefs {
  username?: string;
  password?: string;
  private_key?: string;
  passphrase?: string;
}

export interface SyncScheduleConfig {
  manual: boolean;
  sync_on_startup: boolean;
  startup_delay_seconds: number;
  sync_on_local_change: boolean;
  local_change_debounce_seconds: number;
  interval_minutes: number;
  retry_backoff_seconds: number[];
  pause_on_metered_network: boolean;
  paused?: boolean;
}

export interface SyncProfileInput {
  id: string;
  name: string;
  provider: string;
  remote_root: string;
  sync_tabs?: TabSyncSelection;
  sync_plugins?: PluginSyncSelection;
  encryption?: EncryptionConfig;
  credential_refs?: CredentialRefs;
  schedule?: SyncScheduleConfig;
}

export interface SecretRef {
  ref_id: string;
  profile_id: string;
  key: string;
}

export interface ConnectionTestResult {
  success: boolean;
  message: string;
  server_info?: string;
}

export interface SftpHostKeyTrustResult {
  host: string;
  port: number;
  fingerprint_sha256: string;
  known_hosts_path: string;
}

export interface SyncRunReport {
  profile_id: string;
  run_id: string;
  status: SyncRunStatus;
  started_at: string;
  completed_at: string | null;
  items_uploaded: number;
  items_downloaded: number;
  items_deleted: number;
  conflicts_found: number;
  errors: string[];
}

export type SyncRunStatus =
  | "idle"
  | "waiting_for_lock"
  | "pulling"
  | "applying_remote"
  | "uploading"
  | "committing"
  | "completed"
  | "partial_success"
  | "failed";

export interface SyncStatus {
  profile_id: string;
  status: SyncRunStatus;
  phase: string | null;
  progress: number | null;
  last_sync_at: string | null;
  next_sync_at: string | null;
  is_paused: boolean;
  is_locked: boolean;
  backoff_reason: string | null;
}

export interface SyncConflict {
  id: number;
  entity_type: string;
  entity_key: string;
  local_payload: string;
  remote_payload: string;
  reason: string;
  status: string;
  resolution: string | null;
  created_at: string;
  resolved_at: string | null;
}

export type ConflictResolutionInput =
  | "use_local"
  | "use_remote"
  | "keep_both"
  | "merge_with_local_primary"
  | "merge_with_remote_primary";

export interface SyncTabOption {
  id: number;
  name: string;
}

export interface SyncPluginOption {
  id: string;
  name: string;
  is_active: boolean;
}

export interface SyncLogEntry {
  timestamp: string;
  level: string;
  message: string;
  profile_id: string | null;
  run_id: string | null;
}

// Sync API (Cloud Sync plugin)
export const sync = {
  // Profile management
  profileList: async (): Promise<SyncProfileSummary[]> => {
    log("info", "API", "sync.profileList() called");
    try {
      const result = await invoke<SyncProfileSummary[]>("sync_profile_list");
      log("info", "API", "sync.profileList() returned", result.length, "profiles");
      return result;
    } catch (error) {
      log("error", "API", "sync.profileList() failed", error);
      throw error;
    }
  },

  profileGet: async (profileId: string): Promise<SyncProfile> => {
    log("info", "API", "sync.profileGet() called - profileId:", profileId);
    try {
      const result = await invoke<SyncProfile>("sync_profile_get", { profileId });
      log("info", "API", "sync.profileGet() success");
      return result;
    } catch (error) {
      log("error", "API", "sync.profileGet() failed", error);
      throw error;
    }
  },

  profileUpdate: async (profile: SyncProfileInput): Promise<void> => {
    log("info", "API", "sync.profileUpdate() called - profileId:", profile.id);
    try {
      await invoke<void>("sync_profile_update", { profile });
      log("info", "API", "sync.profileUpdate() success");
    } catch (error) {
      log("error", "API", "sync.profileUpdate() failed", error);
      throw error;
    }
  },

  profileDelete: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profileDelete() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_delete", { profileId });
      log("info", "API", "sync.profileDelete() success");
    } catch (error) {
      log("error", "API", "sync.profileDelete() failed", error);
      throw error;
    }
  },

  profilePause: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profilePause() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_pause", { profileId });
      log("info", "API", "sync.profilePause() success");
    } catch (error) {
      log("error", "API", "sync.profilePause() failed", error);
      throw error;
    }
  },

  profileResume: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profileResume() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_resume", { profileId });
      log("info", "API", "sync.profileResume() success");
    } catch (error) {
      log("error", "API", "sync.profileResume() failed", error);
      throw error;
    }
  },

  // Secret management
  secretSet: async (profileId: string, key: string, value: string): Promise<SecretRef> => {
    log("info", "API", "sync.secretSet() called - profileId:", profileId, "key:", key);
    try {
      const result = await invoke<SecretRef>("sync_secret_set", { profileId, key, value });
      log("info", "API", "sync.secretSet() success");
      return result;
    } catch (error) {
      log("error", "API", "sync.secretSet() failed", error);
      throw error;
    }
  },

  secretDelete: async (secretRef: string): Promise<void> => {
    log("info", "API", "sync.secretDelete() called - ref:", secretRef);
    try {
      await invoke<void>("sync_secret_delete", { secretRef });
      log("info", "API", "sync.secretDelete() success");
    } catch (error) {
      log("error", "API", "sync.secretDelete() failed", error);
      throw error;
    }
  },

  // Profile lock/unlock
  profileUnlock: async (profileId: string, password: string, rememberWithSystemKeychain: boolean): Promise<void> => {
    log("info", "API", "sync.profileUnlock() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_unlock", { profileId, password, rememberWithSystemKeychain });
      log("info", "API", "sync.profileUnlock() success");
    } catch (error) {
      log("error", "API", "sync.profileUnlock() failed", error);
      throw error;
    }
  },

  profileLock: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profileLock() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_lock", { profileId });
      log("info", "API", "sync.profileLock() success");
    } catch (error) {
      log("error", "API", "sync.profileLock() failed", error);
      throw error;
    }
  },

  // Sync operations
  testConnection: async (profileId: string): Promise<ConnectionTestResult> => {
    log("info", "API", "sync.testConnection() called - profileId:", profileId);
    try {
      const result = await invoke<ConnectionTestResult>("sync_test_connection", { profileId });
      log("info", "API", "sync.testConnection() success:", result.success);
      return result;
    } catch (error) {
      log("error", "API", "sync.testConnection() failed", error);
      throw error;
    }
  },

  trustSftpHostKey: async (profileId: string): Promise<SftpHostKeyTrustResult> => {
    log("info", "API", "sync.trustSftpHostKey() called - profileId:", profileId);
    try {
      const result = await invoke<SftpHostKeyTrustResult>("sync_trust_sftp_host_key", { profileId });
      log("info", "API", "sync.trustSftpHostKey() success - host:", result.host);
      return result;
    } catch (error) {
      log("error", "API", "sync.trustSftpHostKey() failed", error);
      throw error;
    }
  },

  runNow: async (profileId: string): Promise<SyncRunReport> => {
    log("info", "API", "sync.runNow() called - profileId:", profileId);
    try {
      const result = await invoke<SyncRunReport>("sync_run_now", { profileId });
      log("info", "API", "sync.runNow() success - status:", result.status);
      return result;
    } catch (error) {
      log("error", "API", "sync.runNow() failed", error);
      throw error;
    }
  },

  cancelRun: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.cancelRun() called - profileId:", profileId);
    try {
      await invoke<void>("sync_cancel_run", { profileId });
      log("info", "API", "sync.cancelRun() success");
    } catch (error) {
      log("error", "API", "sync.cancelRun() failed", error);
      throw error;
    }
  },

  getStatus: async (profileId: string): Promise<SyncStatus> => {
    log("info", "API", "sync.getStatus() called - profileId:", profileId);
    try {
      const result = await invoke<SyncStatus>("sync_get_status", { profileId });
      log("info", "API", "sync.getStatus() success - status:", result.status);
      return result;
    } catch (error) {
      log("error", "API", "sync.getStatus() failed", error);
      throw error;
    }
  },

  getLastReport: async (profileId: string): Promise<SyncRunReport | null> => {
    log("info", "API", "sync.getLastReport() called - profileId:", profileId);
    try {
      const result = await invoke<SyncRunReport | null>("sync_get_last_report", { profileId });
      log("info", "API", "sync.getLastReport() success");
      return result;
    } catch (error) {
      log("error", "API", "sync.getLastReport() failed", error);
      throw error;
    }
  },

  // Conflict management
  getConflicts: async (profileId: string): Promise<SyncConflict[]> => {
    log("info", "API", "sync.getConflicts() called - profileId:", profileId);
    try {
      const result = await invoke<SyncConflict[]>("sync_get_conflicts", { profileId });
      log("info", "API", "sync.getConflicts() returned", result.length, "conflicts");
      return result;
    } catch (error) {
      log("error", "API", "sync.getConflicts() failed", error);
      throw error;
    }
  },

  resolveConflict: async (profileId: string, conflictId: number, resolution: ConflictResolutionInput): Promise<void> => {
    log("info", "API", "sync.resolveConflict() called - conflictId:", conflictId);
    try {
      await invoke<void>("sync_resolve_conflict", { profileId, conflictId, resolution });
      log("info", "API", "sync.resolveConflict() success");
    } catch (error) {
      log("error", "API", "sync.resolveConflict() failed", error);
      throw error;
    }
  },

  // Options and info
  getTabOptions: async (): Promise<SyncTabOption[]> => {
    log("info", "API", "sync.getTabOptions() called");
    try {
      const result = await invoke<SyncTabOption[]>("sync_get_tab_options");
      log("info", "API", "sync.getTabOptions() returned", result.length, "tabs");
      return result;
    } catch (error) {
      log("error", "API", "sync.getTabOptions() failed", error);
      throw error;
    }
  },

  getPluginOptions: async (): Promise<SyncPluginOption[]> => {
    log("info", "API", "sync.getPluginOptions() called");
    try {
      const result = await invoke<SyncPluginOption[]>("sync_get_plugin_options");
      log("info", "API", "sync.getPluginOptions() returned", result.length, "plugins");
      return result;
    } catch (error) {
      log("error", "API", "sync.getPluginOptions() failed", error);
      throw error;
    }
  },

  getLogEntries: async (profileId: string, limit: number): Promise<SyncLogEntry[]> => {
    log("info", "API", "sync.getLogEntries() called - profileId:", profileId, "limit:", limit);
    try {
      const result = await invoke<SyncLogEntry[]>("sync_get_log_entries", { profileId, limit });
      log("info", "API", "sync.getLogEntries() returned", result.length, "entries");
      return result;
    } catch (error) {
      log("error", "API", "sync.getLogEntries() failed", error);
      throw error;
    }
  },
};
