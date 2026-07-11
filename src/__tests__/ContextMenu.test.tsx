import React from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ContextMenu } from "../components/ContextMenu";
import { clipboard, type ClipboardItem } from "../lib/tauri-api";

vi.mock("../lib/tauri-api", () => ({
  ItemType: {
    Image: "image",
  },
  clipboard: {
    moveToTab: vi.fn(),
    copyToTab: vi.fn(),
    moveToTabBatch: vi.fn(),
    copyToTabBatch: vi.fn(),
    restoreFromTrash: vi.fn(),
    deleteByIds: vi.fn(),
    deleteByIdsPermanently: vi.fn(),
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

vi.mock("../components/ConfirmDialog", () => ({
  useConfirm: () => ({ confirm: vi.fn().mockResolvedValue(true) }),
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

  it("keeps Trash as the last context-menu action", () => {
    render(
      <ContextMenu item={item(1)} itemId={1} currentTabId={1}>
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });

    const menu = screen
      .getByRole("button", { name: /move to/i })
      .closest(".fixed") as HTMLDivElement;
    const actions = within(menu).getAllByRole("button");

    expect(actions[actions.length - 1]?.textContent).toContain("Trash");
  });

  it("opens the editor when E is pressed while the menu is open", () => {
    const onEdit = vi.fn();

    render(
      <ContextMenu item={item(1)} itemId={1} currentTabId={1} onEdit={onEdit}>
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });
    fireEvent.keyDown(document, { key: "e" });

    expect(onEdit).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: /move to/i })).toBeNull();
  });

  it("does not use E when editing is unavailable", () => {
    const onEdit = vi.fn();

    render(
      <ContextMenu
        item={{ ...item(1), type: "image" as ClipboardItem["type"] }}
        itemId={1}
        currentTabId={1}
        onEdit={onEdit}
      >
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });
    fireEvent.keyDown(document, { key: "e" });

    expect(onEdit).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: /move to/i })).toBeTruthy();
  });

  it("runs other built-in menu shortcuts", async () => {
    const onTogglePin = vi.fn();
    vi.mocked(clipboard.deleteByIds).mockResolvedValue(1);

    const { unmount } = render(
      <ContextMenu
        item={item(1)}
        itemId={1}
        currentTabId={1}
        onTogglePin={onTogglePin}
      >
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });
    fireEvent.keyDown(document, { key: "p" });
    expect(onTogglePin).toHaveBeenCalledOnce();
    unmount();

    render(
      <ContextMenu item={item(1)} itemId={1} currentTabId={1}>
        <button type="button">Item 2</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 2" }), {
      clientX: 20,
      clientY: 20,
    });
    fireEvent.keyDown(document, { key: "m" });
    expect(screen.getByRole("button", { name: "Work" })).toBeTruthy();

    fireEvent.keyDown(document, { key: "t" });
    await waitFor(() => {
      expect(clipboard.deleteByIds).toHaveBeenCalledWith([1]);
    });
  });

  it("permanently deletes an item from the Trash menu", async () => {
    vi.mocked(clipboard.deleteByIdsPermanently).mockResolvedValue(1);

    render(
      <ContextMenu item={item(1)} itemId={1} currentTabId={1} isTrash>
        <button type="button">Item 1</button>
      </ContextMenu>,
    );

    fireEvent.contextMenu(screen.getByRole("button", { name: "Item 1" }), {
      clientX: 20,
      clientY: 20,
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Delete permanently" }),
    );

    await waitFor(() => {
      expect(clipboard.deleteByIdsPermanently).toHaveBeenCalledWith([1]);
    });
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
