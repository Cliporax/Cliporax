import React from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ContextMenu } from "../components/ContextMenu";

vi.mock("../lib/tauri-api", () => ({
  clipboard: {
    moveToTab: vi.fn(),
    copyToTab: vi.fn(),
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
    removeItem: vi.fn(),
  }),
}));

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
  it("keeps the main menu inside viewport boundaries", () => {
    setViewport(400, 300);

    render(
      <ContextMenu itemId={1} currentTabId={1}>
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
        <ContextMenu itemId={1} currentTabId={1}>
          <button type="button">Item 1</button>
        </ContextMenu>
        <ContextMenu itemId={2} currentTabId={1}>
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
});
