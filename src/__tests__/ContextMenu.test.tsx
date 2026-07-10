import React from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ContextMenu } from "../components/ContextMenu";
import { clipboard, type ClipboardItem } from "../lib/tauri-api";

vi.mock("../lib/tauri-api", () => ({
  clipboard: {
    moveToTab: vi.fn(),
    copyToTab: vi.fn(),
    moveToTabBatch: vi.fn(),
    copyToTabBatch: vi.fn(),
  },
}));

vi.mock("../stores/tabStore", () => ({
  useTabStore: () => ({
    tabs: [
      { id: 1, name: "Default" },
      { id: 2, name: "Work" },
    ],
  }),
}));

vi.mock("../stores/clipboardStore", () => ({
  useClipboardStore: () => ({
    items: [],
    removeItem: vi.fn(),
    updateItem: vi.fn(),
  }),
}));

vi.mock("../plugin/extensions", () => ({
  useExtensionManager: () => ({
    getExtensions: () => [],
  }),
}));

const item = (id: number): ClipboardItem => ({
  id,
  type: "text" as ClipboardItem["type"],
  content: `Item ${id}`,
  content_hash: null,
  metadata: null,
  tags: null,
  tab_id: 1,
  is_sensitive: false,
  is_pinned: false,
  display_order: id,
  created_at: null,
  updated_at: null,
});

const setViewport = (width: number, height: number) => {
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: width,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: height,
  });
};

describe("ContextMenu", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("keeps the main menu inside viewport boundaries", () => {
    setViewport(400, 300);

    render(
      <ContextMenu item={item(1)} itemId={1} currentTabId={1}>
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 390,
      clientY: 290,
    });

    const menu = screen
      .getByRole("button", { name: /move to/i })
      .closest(".fixed") as HTMLDivElement;

    expect(menu.style.left).toBe("232px");
    expect(menu.style.top).toBe("228px");
  });

  it("closes an open item menu when another item is clicked", async () => {
    render(
      <>
        <ContextMenu item={item(1)} itemId={1} currentTabId={1}>
          <button type="button">Item 1</button>
        </ContextMenu>
        <ContextMenu item={item(2)} itemId={2} currentTabId={1}>
          <button type="button">Item 2</button>
        </ContextMenu>
      </>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });
    expect(screen.getByRole("button", { name: /move to/i })).toBeTruthy();

    fireEvent.pointerDown(screen.getByRole("button", { name: "Item 2" }));

    await waitFor(() => {
      expect(screen.queryByRole("button", { name: /move to/i })).toBeNull();
    });
  });

  it("moves all selected items from the right-click menu", async () => {
    const onBatchActionComplete = vi.fn();
    vi.mocked(clipboard.moveToTabBatch).mockResolvedValue(2);

    render(
      <ContextMenu
        item={item(1)}
        itemId={1}
        currentTabId={1}
        batchItemIds={new Set([1, 2])}
        onBatchActionComplete={onBatchActionComplete}
      >
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });

    fireEvent.mouseEnter(
      screen.getByRole("button", { name: /move to/i }).parentElement!,
    );
    fireEvent.click(screen.getByRole("button", { name: "Work" }));

    await waitFor(() => {
      expect(clipboard.moveToTabBatch).toHaveBeenCalledWith([1, 2], 2);
    });
    expect(clipboard.moveToTab).not.toHaveBeenCalled();
    expect(onBatchActionComplete).toHaveBeenCalledTimes(1);
  });

  it("copies all selected items from the right-click menu", async () => {
    const onBatchActionComplete = vi.fn();
    vi.mocked(clipboard.copyToTabBatch).mockResolvedValue(2);

    render(
      <ContextMenu
        item={item(1)}
        itemId={1}
        currentTabId={1}
        batchItemIds={new Set([1, 2])}
        onBatchActionComplete={onBatchActionComplete}
      >
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });

    fireEvent.mouseEnter(
      screen.getByRole("button", { name: /copy to/i }).parentElement!,
    );
    fireEvent.click(screen.getByRole("button", { name: "Work" }));

    await waitFor(() => {
      expect(clipboard.copyToTabBatch).toHaveBeenCalledWith([1, 2], 2);
    });
    expect(clipboard.copyToTab).not.toHaveBeenCalled();
    expect(onBatchActionComplete).toHaveBeenCalledTimes(1);
  });
});
