import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";
import * as tauriApi from "../lib/tauri-api";

// Mock Tauri invoke function
const mockInvoke = vi.fn();
const mockListen = vi.fn();

// Mock the Tauri modules
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: any) => mockInvoke(cmd, args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: (event: string, callback: any) => {
    mockListen(event, callback);
    return Promise.resolve(() => {});
  },
  Event: class {},
}));

function stripTraceArgs(args: Record<string, unknown> | undefined) {
  if (!args) return args;
  return Object.fromEntries(
    Object.entries(args).filter(([key]) => !key.startsWith("_")),
  );
}

function expectInvokeCalledWith(cmd: string, args?: Record<string, unknown>) {
  const call = mockInvoke.mock.calls.find(([calledCmd]) => calledCmd === cmd);
  expect(call).toBeTruthy();
  expect(stripTraceArgs(call?.[1]) ?? {}).toEqual(args ?? {});
}

describe("Tauri API Tests", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Mock console methods to avoid noise in test output
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(console, "debug").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("Tab API", () => {
    it("should get all tabs successfully", async () => {
      const mockTabs = [
        { id: 1, name: "Default", is_default: 1 },
        { id: 2, name: "Test Tab", is_default: 0 },
      ];

      mockInvoke.mockResolvedValue(mockTabs);

      const result = await tauriApi.tabs.getAll();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("tabs_get_all");
      expect(result).toEqual(mockTabs);
    });

    it("should handle tab get all error", async () => {
      const errorMessage = "Failed to get tabs";
      mockInvoke.mockRejectedValue(new Error(errorMessage));

      await expect(tauriApi.tabs.getAll()).rejects.toThrow(errorMessage);
    });

    it("should create tab successfully", async () => {
      const tabName = "New Tab";
      const mockId = 3;

      mockInvoke.mockResolvedValue(mockId);

      const result = await tauriApi.tabs.create(tabName);

      expect(mockInvoke).toHaveBeenCalledWith("tabs_create", { name: tabName });
      expect(result).toBe(mockId);
    });

    it("should delete tab successfully", async () => {
      const tabId = 1;

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.tabs.delete(tabId);

      expect(mockInvoke).toHaveBeenCalledWith("tabs_delete", { id: tabId });
    });
  });

  describe("Clipboard API", () => {
    it("should get clipboard items by tab successfully", async () => {
      const tabId = 1;
      const mockItems = [
        {
          id: 1,
          type: "text",
          content: "Test content",
          is_pinned: 0,
        },
      ];

      mockInvoke.mockResolvedValue(mockItems);

      const result = await tauriApi.clipboard.getByTab(tabId, 10, 0);

      expectInvokeCalledWith("clipboard_get_by_tab", {
        tabId,
        limit: 10,
        offset: 0,
      });
      expect(result[0]).toMatchObject({
        id: 1,
        type: "text",
        content: "Test content",
        is_pinned: false,
        is_sensitive: false,
      });
    });

    it("should create clipboard item successfully", async () => {
      const itemInput = {
        type: tauriApi.ItemType.Text,
        content: "New clipboard content",
        content_hash: null,
        metadata: "{}",
        tags: "[]",
        tab_id: 1,
        is_sensitive: false,
        is_pinned: false,
      };
      const mockId = 5;

      mockInvoke.mockResolvedValue(mockId);

      const result = await tauriApi.clipboard.create(itemInput);

      expectInvokeCalledWith("clipboard_create", {
        item: itemInput,
      });
      expect(result).toBe(mockId);
    });

    it("should delete clipboard item successfully", async () => {
      const itemId = 1;

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.delete(itemId);

      expectInvokeCalledWith("clipboard_delete", {
        id: itemId,
      });
    });

    it("should toggle pin status successfully", async () => {
      const itemId = 1;
      const isPinned = 1;

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.togglePin(itemId, isPinned);

      expectInvokeCalledWith("clipboard_toggle_pin", {
        id: itemId,
        isPinned,
      });
    });

    it("should move item to top successfully", async () => {
      const itemId = 1;

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.moveToTop(itemId);

      expectInvokeCalledWith("clipboard_move_to_top", {
        id: itemId,
      });
    });

    it("should search clipboard items successfully", async () => {
      const query = "test";
      const tabId = 1;
      const mockResults = [{ id: 1, type: "text", content: "Test content" }];

      mockInvoke.mockResolvedValue(mockResults);

      const result = await tauriApi.clipboard.search(query, tabId);

      expectInvokeCalledWith("clipboard_search", {
        query,
        tabId,
      });
      expect(result[0]).toMatchObject({
        id: 1,
        type: "text",
        content: "Test content",
        is_pinned: false,
        is_sensitive: false,
      });
    });

    it("should support global clipboard search without a tab id", async () => {
      const query = "global";
      mockInvoke.mockResolvedValue([]);

      await tauriApi.clipboard.search(query);

      expectInvokeCalledWith("clipboard_search", {
        query,
        tabId: undefined,
      });
    });

    it("should update tags successfully", async () => {
      const itemId = 1;
      const tags = ["tag1", "tag2"];

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.updateTags(itemId, tags);

      expectInvokeCalledWith("clipboard_update_tags", {
        id: itemId,
        tags,
      });
    });

    it("should clear sensitive items successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.clearSensitive();

      expectInvokeCalledWith("clipboard_clear_sensitive");
    });

    it("should copy text successfully", async () => {
      const content = "Copy this text";
      const type = "text" as const;

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.copy(content, type);

      expectInvokeCalledWith("clipboard_copy", {
        content,
        itemType: type,
      });
    });

    it("should copy image successfully", async () => {
      const content = "data:image/png;base64,test";
      const type = "image" as const;

      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.clipboard.copy(content, type);

      expectInvokeCalledWith("clipboard_copy", {
        content,
        itemType: type,
      });
    });
  });

  describe("File Sync API", () => {
    it("uses the official plugin identity when enqueueing a clipboard item", async () => {
      mockInvoke.mockResolvedValue({ entry_ids: ["entry123"] });

      const result = await tauriApi.fileSync.enqueueClipboardItem(42);

      expect(mockInvoke).toHaveBeenCalledWith(
        "file_sync_enqueue_clipboard_item",
        {
          pluginId: "com.cliporax.file-sync",
          itemId: 42,
        },
      );
      expect(result.entry_ids).toEqual(["entry123"]);
    });

    it("propagates a backend confirmation-state error", async () => {
      mockInvoke.mockRejectedValue(
        new Error("Entry is not waiting for confirmation"),
      );

      await expect(tauriApi.fileSync.confirm("entry123")).rejects.toThrow(
        "Entry is not waiting for confirmation",
      );
    });

    it("uses the official plugin identity when checking a file item", async () => {
      mockInvoke.mockResolvedValue({
        visible: true,
        can_enqueue: false,
        reason: "Already in File Sync",
      });

      await tauriApi.fileSync.clipboardItemStatus(42);

      expect(mockInvoke).toHaveBeenCalledWith(
        "file_sync_clipboard_item_status",
        {
          pluginId: "com.cliporax.file-sync",
          itemId: 42,
        },
      );
    });
  });

  describe("Window API", () => {
    it("should minimize window successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.window.minimize();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_minimize");
    });

    it("should maximize window successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.window.maximize();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_maximize");
    });

    it("should close window successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.window.close();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_close");
    });

    it("should show window successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.window.show();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_show");
    });

    it("should hide window successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.window.hide();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_hide");
    });

    it("should toggle window successfully", async () => {
      mockInvoke.mockResolvedValue(undefined);

      await tauriApi.window.toggle();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_toggle");
    });

    it("should check if window is maximized successfully", async () => {
      const isMaximized = true;
      mockInvoke.mockResolvedValue(isMaximized);

      const result = await tauriApi.window.isMaximized();

      expect(mockInvoke).toHaveBeenCalled();
      expect(mockInvoke.mock.calls[0][0]).toBe("window_is_maximized");
      expect(result).toBe(isMaximized);
    });
  });

  describe("Event Listeners", () => {
    it("should register clipboard changed event listener", async () => {
      const mockCallback = vi.fn();
      const mockUnlisten = vi.fn();

      mockListen.mockImplementation((event, callback) => {
        // Simulate event firing
        setTimeout(() => callback({}), 0);
        return Promise.resolve(mockUnlisten);
      });

      const unlisten = await tauriApi.events.onClipboardChanged(mockCallback);

      // Wait for async operations
      await new Promise((resolve) => setTimeout(resolve, 10));

      expect(mockListen).toHaveBeenCalledWith(
        "clipboard:changed",
        expect.any(Function),
      );
      expect(mockCallback).toHaveBeenCalled();
      expect(typeof unlisten).toBe("function");
    });
  });

  describe("Error Handling", () => {
    it("should handle API errors properly", async () => {
      const errorMessage = "API Error";
      mockInvoke.mockRejectedValue(new Error(errorMessage));

      await expect(tauriApi.tabs.getAll()).rejects.toThrow(errorMessage);
      await expect(tauriApi.clipboard.getByTab(1)).rejects.toThrow(
        errorMessage,
      );
      await expect(tauriApi.window.minimize()).rejects.toThrow(errorMessage);
    });

    it("should log errors appropriately", async () => {
      const consoleErrorSpy = vi
        .spyOn(console, "error")
        .mockImplementation(() => {});
      const errorMessage = "Test error";
      mockInvoke.mockRejectedValue(new Error(errorMessage));

      try {
        await tauriApi.tabs.getAll();
      } catch (e) {
        // Expected error
      }

      // Check that console.error was called with the expected message format
      expect(consoleErrorSpy).toHaveBeenCalled();
      const callArgs = consoleErrorSpy.mock.calls[0];
      expect(callArgs[0]).toContain("[API]");
      expect(callArgs[0]).toContain("tabs.getAll() failed");

      consoleErrorSpy.mockRestore();
    });
  });
});
