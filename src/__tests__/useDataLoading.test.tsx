import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClipboardCacheManager, ItemTypeCache } from "../components/ClipboardList/cache";
import { useDataLoading } from "../components/ClipboardList/hooks/useDataLoading";
import { clipboard } from "../lib/tauri-api";
import { ItemType, type ClipboardItem } from "../types/generated/api";

vi.mock("../lib/tauri-api", () => ({
  clipboard: {
    getAllTypes: vi.fn(),
    getById: vi.fn(),
    getLatest: vi.fn(),
    getTotalCount: vi.fn(),
  },
  events: {},
  tabs: {},
}));

const makeItem = (content: string, updatedAt: string): ClipboardItem => ({
  id: 1,
  type: ItemType.Text,
  content,
  content_hash: null,
  is_sensitive: false,
  is_pinned: false,
  created_at: "2026-07-03T00:00:00Z",
  updated_at: updatedAt,
});

describe("useDataLoading incremental updates", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("refreshes a cached top item when the backend keeps the same item ID", async () => {
    const cacheManager = new ClipboardCacheManager();
    const typeCache = new ItemTypeCache();
    cacheManager.addItems(
      [makeItem("stale content", "2026-07-03T00:00:00Z")],
      0,
    );
    typeCache.setType(0, "text");

    vi.mocked(clipboard.getLatest).mockResolvedValue(
      makeItem("updated content", "2026-07-03T00:01:00Z"),
    );
    vi.mocked(clipboard.getTotalCount).mockResolvedValue(1);

    const setTotalCount = vi.fn();
    const setCacheVersion = vi.fn();
    const { result } = renderHook(() =>
      useDataLoading({
        defaultTabId: 1,
        totalCount: 1,
        isAutoCaptureTab: true,
        isSearchMode: false,
        visibleStartIndex: 0,
        visibleEndIndex: 0,
        isMultiDraggingRef: { current: false },
        cacheManagerRef: { current: cacheManager },
        typeCacheRef: { current: typeCache },
        containerRef: { current: null },
        setTotalCount,
        setIsLoading: vi.fn(),
        setCacheVersion,
      }),
    );

    await act(async () => {
      await result.current.incrementalUpdate();
    });

    expect(cacheManager.getItem(0)).toMatchObject({
      id: 1,
      content: "updated content",
      updated_at: "2026-07-03T00:01:00Z",
    });
    expect(setTotalCount).toHaveBeenCalledWith(1);
    expect(setCacheVersion).toHaveBeenCalledOnce();
  });

  it("uses the event item ID for an in-place update without falling back to latest", async () => {
    const cacheManager = new ClipboardCacheManager();
    const typeCache = new ItemTypeCache();
    cacheManager.addItems([makeItem("old content", "2026-07-03T00:00:00Z")], 0);
    typeCache.setType(0, "text");

    vi.mocked(clipboard.getById).mockResolvedValue({
      ...makeItem("new content", "2026-07-03T00:01:00Z"),
      id: 2,
      tab_id: 1,
    });
    vi.mocked(clipboard.getTotalCount).mockResolvedValue(2);

    const { result } = renderHook(() =>
      useDataLoading({
        defaultTabId: 1,
        totalCount: 1,
        isAutoCaptureTab: true,
        isSearchMode: false,
        visibleStartIndex: 0,
        visibleEndIndex: 0,
        isMultiDraggingRef: { current: false },
        cacheManagerRef: { current: cacheManager },
        typeCacheRef: { current: typeCache },
        containerRef: { current: null },
        setTotalCount: vi.fn(),
        setIsLoading: vi.fn(),
        setCacheVersion: vi.fn(),
      }),
    );

    await act(async () => {
      await result.current.incrementalUpdate({ itemIds: [2], tabIds: [1] });
    });

    expect(clipboard.getById).toHaveBeenCalledWith(2);
    expect(clipboard.getLatest).not.toHaveBeenCalled();
    expect(cacheManager.getItem(0)).toMatchObject({ id: 2, content: "new content" });
    expect(cacheManager.getItem(1)).toMatchObject({ id: 1, content: "old content" });
  });

  it("inserts a new unpinned item after pinned items without clearing the cache", async () => {
    const cacheManager = new ClipboardCacheManager();
    const typeCache = new ItemTypeCache();
    cacheManager.addItems(
      [{ ...makeItem("pinned content", "2026-07-03T00:00:00Z"), is_pinned: true }],
      0,
    );
    typeCache.setType(0, "text");

    vi.mocked(clipboard.getLatest).mockResolvedValue({
      ...makeItem("new unpinned content", "2026-07-03T00:01:00Z"),
      id: 2,
    });
    vi.mocked(clipboard.getTotalCount).mockResolvedValue(2);
    vi.mocked(clipboard.getAllTypes).mockResolvedValue([
      [1, "text"],
      [2, "text"],
    ]);

    const setTotalCount = vi.fn();
    const setCacheVersion = vi.fn();
    const { result } = renderHook(() =>
      useDataLoading({
        defaultTabId: 1,
        totalCount: 1,
        isAutoCaptureTab: true,
        isSearchMode: false,
        visibleStartIndex: 0,
        visibleEndIndex: 0,
        isMultiDraggingRef: { current: false },
        cacheManagerRef: { current: cacheManager },
        typeCacheRef: { current: typeCache },
        containerRef: { current: null },
        setTotalCount,
        setIsLoading: vi.fn(),
        setCacheVersion,
      }),
    );

    await act(async () => {
      await result.current.incrementalUpdate();
    });

    expect(cacheManager.getItem(0)).toMatchObject({
      id: 1,
      content: "pinned content",
      is_pinned: true,
    });
    expect(cacheManager.getItem(1)).toMatchObject({
      id: 2,
      content: "new unpinned content",
      is_pinned: false,
    });
    expect(clipboard.getAllTypes).not.toHaveBeenCalled();
    expect(setTotalCount).toHaveBeenCalledWith(2);
    expect(setCacheVersion).toHaveBeenCalled();
  });

  it("moves an existing unpinned item after pinned items without refreshing", async () => {
    const cacheManager = new ClipboardCacheManager();
    const typeCache = new ItemTypeCache();
    const pinnedItem = {
      ...makeItem("pinned content", "2026-07-03T00:00:00Z"),
      is_pinned: true,
    };
    const oldTopItem = {
      ...makeItem("old top", "2026-07-03T00:00:00Z"),
      id: 2,
    };
    const movedItem = {
      ...makeItem("moved content", "2026-07-03T00:01:00Z"),
      id: 3,
    };
    cacheManager.addItems([pinnedItem, oldTopItem, movedItem], 0);
    typeCache.setTypes(
      [
        { id: 1, type: "text" },
        { id: 2, type: "text" },
        { id: 3, type: "text" },
      ],
      0,
    );

    vi.mocked(clipboard.getLatest).mockResolvedValue(movedItem);

    const setCacheVersion = vi.fn();
    const { result } = renderHook(() =>
      useDataLoading({
        defaultTabId: 1,
        totalCount: 3,
        isAutoCaptureTab: true,
        isSearchMode: false,
        visibleStartIndex: 0,
        visibleEndIndex: 2,
        isMultiDraggingRef: { current: false },
        cacheManagerRef: { current: cacheManager },
        typeCacheRef: { current: typeCache },
        containerRef: { current: null },
        setTotalCount: vi.fn(),
        setIsLoading: vi.fn(),
        setCacheVersion,
      }),
    );

    await act(async () => {
      await result.current.incrementalUpdate();
    });

    expect(cacheManager.getItem(0)?.id).toBe(1);
    expect(cacheManager.getItem(1)?.id).toBe(3);
    expect(cacheManager.getItem(2)?.id).toBe(2);
    expect(clipboard.getTotalCount).not.toHaveBeenCalled();
    expect(clipboard.getAllTypes).not.toHaveBeenCalled();
    expect(setCacheVersion).toHaveBeenCalledOnce();
  });
});
