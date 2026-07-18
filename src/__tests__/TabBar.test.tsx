import React from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TabBar } from "../components/TabBar";
import { BottomNavigation } from "../components/BottomNavigation";

const mockSetSearchQuery = vi.fn();
const mockSetActiveTab = vi.fn();
const mockSetActivePluginTab = vi.fn();
const mockReorderTabs = vi.fn();
const pluginIconDataUrl = "data:image/svg+xml;base64,PHN2Zy8+";
let mockActivePluginTabId: string | null = null;
let mockTabs: Array<Record<string, unknown>> = [];
let mockPluginTabs: Array<{
  id: string;
  pluginId: string;
  title: string;
  iconDataUrl?: string;
  component: string;
  priority: number;
}> = [
  {
    id: "plugin:com.cliporax.file-sync:FileSyncView",
    pluginId: "com.cliporax.file-sync",
    title: "File Sync",
    iconDataUrl: undefined,
    component: "FileSyncView",
    priority: 20,
  },
];

vi.mock("../stores/tabStore", () => ({
  useTabStore: () => ({
    tabs: mockTabs,
    activeTabId: 1,
    activePluginTabId: mockActivePluginTabId,
    isLoading: false,
    isReordering: false,
    createTab: vi.fn(),
    reorderTabs: mockReorderTabs,
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
    mockTabs = [
      { id: 1, name: "Default", is_default: true, is_trash: 0 },
      { id: 2, name: "Work", is_default: false, is_trash: 0 },
      { id: 3, name: "Trash", is_default: false, is_trash: 1 },
    ];
    mockPluginTabs = [
      {
        id: "plugin:com.cliporax.file-sync:FileSyncView",
        pluginId: "com.cliporax.file-sync",
        title: "File Sync",
        iconDataUrl: undefined,
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

  it("shows tabs directly without a collections header or inline delete button", () => {
    render(<TabBar />);

    expect(screen.queryByText("Collections")).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete tab Work" })).toBeNull();
  });

  it("offers deletion from a custom tab context menu", () => {
    render(<TabBar />);

    fireEvent.contextMenu(screen.getByText("Work"), { clientX: 20, clientY: 20 });

    expect(screen.getByRole("button", { name: /delete/i })).toBeTruthy();
  });

  it("resizes the clipboard collections sidebar within its limits", () => {
    const onWidthChange = vi.fn();
    render(<TabBar width={176} onWidthChange={onWidthChange} />);

    fireEvent.pointerDown(screen.getByTestId("clipboard-sidebar-resize-handle"), {
      button: 0,
      clientX: 176,
    });
    fireEvent.pointerMove(window, { clientX: 260 });
    fireEvent.pointerUp(window);

    expect(onWidthChange).toHaveBeenCalledWith(260);
  });

  it("hides the sidebar when it is resized below the collapsed width", () => {
    const onCollapse = vi.fn();
    const onWidthChange = vi.fn();
    render(<TabBar width={80} onCollapse={onCollapse} onWidthChange={onWidthChange} />);

    fireEvent.pointerDown(screen.getByTestId("clipboard-sidebar-resize-handle"), {
      button: 0,
      clientX: 80,
    });
    fireEvent.pointerMove(window, { clientX: 50 });

    expect(onCollapse).toHaveBeenCalledOnce();
    expect(onWidthChange).not.toHaveBeenCalled();
  });

  it("switches to a plugin content tab from the bottom navigation", () => {
    render(<BottomNavigation />);

    fireEvent.click(screen.getByRole("button", { name: "File Sync" }));

    expect(mockSetActivePluginTab).toHaveBeenCalledWith(
      "plugin:com.cliporax.file-sync:FileSyncView",
    );
    expect(mockSetActiveTab).not.toHaveBeenCalled();
    expect(mockSetSearchQuery).toHaveBeenCalledWith("");
  });

  it("returns to the clipboard page from the bottom navigation", () => {
    mockActivePluginTabId = "plugin:com.cliporax.file-sync:FileSyncView";
    render(<BottomNavigation />);

    fireEvent.click(screen.getByRole("button", { name: "Clipboard" }));

    expect(mockSetActivePluginTab).toHaveBeenCalledWith(null);
    expect(mockSetSearchQuery).toHaveBeenCalledWith("");
  });

  it("marks the current bottom navigation item and gives it a visible indicator", () => {
    mockActivePluginTabId = "plugin:com.cliporax.file-sync:FileSyncView";
    render(<BottomNavigation />);

    const clipboard = screen.getByRole("button", { name: "Clipboard" });
    const fileSync = screen.getByRole("button", { name: "File Sync" });

    expect(clipboard.getAttribute("aria-current")).toBeNull();
    expect(fileSync.getAttribute("aria-current")).toBe("page");
    expect(fileSync.querySelector('[aria-hidden="true"].absolute')).toBeTruthy();
  });

  it("does not render a numeric false trash flag before a tab name", () => {
    render(<TabBar />);

    expect(screen.getByText("Default").textContent).toBe("Default");
    expect(screen.getByText("Work").textContent).toBe("Work");
  });

  it("renders Trash after all regular tabs by default", () => {
    render(<TabBar />);

    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "Default",
      "Work",
      "Trash",
    ]);
  });

  it("persists a dragged tab order", async () => {
    render(<TabBar />);
    const workTab = screen.getByRole("tab", { name: /Work/ });
    const defaultTab = screen.getByRole("tab", { name: "Default" });
    vi.spyOn(defaultTab, "getBoundingClientRect").mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 20,
    } as DOMRect);
    fireEvent.pointerDown(workTab, {
      button: 0,
      pointerId: 1,
      clientX: 100,
      clientY: 10,
    });
    fireEvent.pointerMove(workTab, {
      pointerId: 1,
      clientX: 0,
      clientY: 0,
    });
    fireEvent.pointerUp(workTab, {
      pointerId: 1,
      clientX: 0,
      clientY: 0,
    });

    await waitFor(() => {
      expect(mockReorderTabs).toHaveBeenCalledWith([2, 1, 3]);
    });
  });

  it("renders a plugin tab's own icon in the bottom navigation", () => {
    mockPluginTabs = [
      {
        id: "plugin:com.cliporax.file-sync:FileSyncView",
        pluginId: "com.cliporax.file-sync",
        title: "File Sync",
        iconDataUrl: pluginIconDataUrl,
        component: "FileSyncView",
        priority: 20,
      },
    ];
    render(<BottomNavigation />);

    expect(
      screen
        .getByRole("button", { name: "File Sync" })
        .querySelector("img")
        ?.getAttribute("src"),
    ).toBe(pluginIconDataUrl);
  });

  it("opens File Sync when a file item requests it", () => {
    render(<BottomNavigation />);

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

    render(<BottomNavigation />);

    await waitFor(() => {
      expect(mockSetActivePluginTab).toHaveBeenCalledWith(null);
    });
  });
});
