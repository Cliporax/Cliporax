import React from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TabBar } from "../components/TabBar";

const mockSetSearchQuery = vi.fn();
const mockSetActiveTab = vi.fn();
const mockSetActivePluginTab = vi.fn();
let mockActivePluginTabId: string | null = null;
let mockPluginTabs = [
  {
    id: "plugin:com.cliporax.file-sync:FileSyncView",
    pluginId: "com.cliporax.file-sync",
    title: "File Sync",
    component: "FileSyncView",
    priority: 20,
  },
];

vi.mock("../stores/tabStore", () => ({
  useTabStore: () => ({
    tabs: [
      { id: 1, name: "Default", is_default: true },
      { id: 2, name: "Work", is_default: false },
    ],
    activeTabId: 1,
    activePluginTabId: mockActivePluginTabId,
    isLoading: false,
    loadTabs: vi.fn(),
    createTab: vi.fn(),
    deleteTab: vi.fn(),
    renameTab: vi.fn(),
    setActiveTab: mockSetActiveTab,
    setActivePluginTab: mockSetActivePluginTab,
  }),
}));

vi.mock("../plugin/extensions", () => ({
  useContentTabExtensions: () => mockPluginTabs,
}));

vi.mock("../stores/uiStore", () => ({
  useUIStore: () => ({
    setSearchQuery: mockSetSearchQuery,
  }),
}));

vi.mock("../components/Toast", () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("../components/ConfirmDialog", () => ({
  useConfirm: () => ({
    confirm: vi.fn(),
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

describe("TabBar", () => {
  beforeEach(() => {
    mockActivePluginTabId = null;
    mockPluginTabs = [
      {
        id: "plugin:com.cliporax.file-sync:FileSyncView",
        pluginId: "com.cliporax.file-sync",
        title: "File Sync",
        component: "FileSyncView",
        priority: 20,
      },
    ];
    vi.clearAllMocks();
  });

  it("keeps the rename context menu inside viewport boundaries", () => {
    setViewport(800, 600);
    render(<TabBar />);

    fireEvent.contextMenu(screen.getByText("Work"), {
      clientX: 790,
      clientY: 590,
    });

    const menu = screen
      .getByRole("button", { name: /rename/i })
      .closest(".fixed") as HTMLDivElement;

    expect(menu.style.left).toBe("664px");
    expect(menu.style.top).toBe("556px");
  });

  it("closes the rename context menu when another tab is clicked", async () => {
    render(<TabBar />);

    fireEvent.contextMenu(screen.getByText("Work"), {
      clientX: 20,
      clientY: 20,
    });
    expect(screen.getByRole("button", { name: /rename/i })).toBeTruthy();

    fireEvent.pointerDown(screen.getByText("Default"));

    await waitFor(() => {
      expect(screen.queryByRole("button", { name: /rename/i })).toBeNull();
    });
  });

  it("switches to a plugin content tab without changing the clipboard tab", () => {
    render(<TabBar />);

    fireEvent.click(screen.getByRole("button", { name: "File Sync" }));

    expect(mockSetActivePluginTab).toHaveBeenCalledWith(
      "plugin:com.cliporax.file-sync:FileSyncView",
    );
    expect(mockSetActiveTab).not.toHaveBeenCalled();
    expect(mockSetSearchQuery).toHaveBeenCalledWith("");
  });

  it("opens File Sync when a file item requests it", () => {
    render(<TabBar />);

    window.dispatchEvent(new CustomEvent("cliporax:open-file-sync"));

    expect(mockSetActivePluginTab).toHaveBeenCalledWith(
      "plugin:com.cliporax.file-sync:FileSyncView",
    );
    expect(mockSetSearchQuery).toHaveBeenCalledWith("");
  });

  it("falls back to the clipboard tab when the active plugin tab disappears", async () => {
    mockActivePluginTabId =
      "plugin:com.cliporax.file-sync:FileSyncView";
    mockPluginTabs = [];

    render(<TabBar />);

    await waitFor(() => {
      expect(mockSetActivePluginTab).toHaveBeenCalledWith(null);
    });
  });
});
