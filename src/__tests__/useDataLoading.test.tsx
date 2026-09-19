import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClipboardCacheManager, ItemTypeCache } from "../components/ClipboardList/cache";
import { useDataLoading } from "../components/ClipboardList/hooks/useDataLoading";
import { clipboard } from "../lib/tauri-api";
import { ItemType, type ClipboardItem } from "../types/generated/api";

vi.mock("../lib/tauri-api", () => ({
  clipboard: { getAllTypes: vi.fn(), getByTab: vi.fn(), getTotalCount: vi.fn() },
  tabs: {},
}));
const item = (id: number, type = ItemType.Text): ClipboardItem => ({
  id, type, content: `item ${id}`, content_hash: null, is_sensitive: false,
  is_pinned: false, created_at: null, updated_at: null,
});
function setup() {
  const cache = new ClipboardCacheManager();
  const types = new ItemTypeCache();
  const setTotalCount = vi.fn();
  const { result } = renderHook(() => useDataLoading({
    defaultTabId: 1, totalCount: 3, isAutoCaptureTab: true, isSearchMode: false,
    visibleStartIndex: 0, visibleEndIndex: 2, isMultiDraggingRef: { current: false },
    cacheManagerRef: { current: cache }, typeCacheRef: { current: types },
    containerRef: { current: null }, setTotalCount, setIsLoading: vi.fn(),
    setCacheVersion: vi.fn(),
  }));
  return { result, cache, types, setTotalCount };
}
describe("clipboard positional cache reconciliation", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(clipboard.getTotalCount).mockResolvedValue(2);
    vi.mocked(clipboard.getAllTypes).mockResolvedValue([[2, "image"], [1, "text"]]);
    vi.mocked(clipboard.getByTab).mockResolvedValue([item(2, ItemType.Image), item(1)]);
  });
  it("reloads authoritative order and image heights for batch screenshot events", async () => {
    const { result, cache, types } = setup();
    cache.addItems([item(1)], 0);
    await act(() => result.current.incrementalUpdate({ tabIds: [1], itemIds: [2, 1] }));
    expect(cache.getItem(0).id).toBe(2);
    expect(cache.getItem(1).id).toBe(1);
    expect(types.getType(0)).toBe("image");
    expect(types.getType(1)).toBe("text");
  });
  it("rejects a page returned after a screenshot changed its offsets", async () => {
    const { result, cache } = setup();
    let resolve!: (items: ClipboardItem[]) => void;
    vi.mocked(clipboard.getByTab).mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    let pending!: Promise<boolean>;
    act(() => { pending = result.current.loadRange(0, 2); });
    await act(() => result.current.incrementalUpdate({ tabIds: [1], itemIds: [2] }));
    await act(async () => { resolve([item(1), item(3)]); await pending; });
    expect(cache.getItem(0).id).toBe(2);
    expect(cache.getItem(1).id).toBe(1);
  });
  it("ignores obsolete refresh counts when events overlap", async () => {
    const { result, setTotalCount } = setup();
    let resolve!: (count: number) => void;
    vi.mocked(clipboard.getTotalCount).mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    let pending!: Promise<void>;
    act(() => { pending = result.current.refreshList(); });
    await act(() => result.current.refreshList());
    await act(async () => { resolve(99); await pending; });
    expect(setTotalCount).toHaveBeenCalledWith(2);
    expect(setTotalCount).not.toHaveBeenCalledWith(99);
  });
  it("does not refresh for an event targeting another tab", async () => {
    const { result } = setup();
    await act(() => result.current.incrementalUpdate({ tabIds: [9], itemIds: [2] }));
    expect(clipboard.getTotalCount).not.toHaveBeenCalled();
  });
  it("preserves ID mappings when a batch swaps existing positions", () => {
    const cache = new ClipboardCacheManager();
    cache.addItems([item(1), item(2)], 0);
    cache.addItems([item(2), item(1)], 0);
    expect(cache.getIndexById(1)).toBe(1);
    expect(cache.getIndexById(2)).toBe(0);
  });
  it("removes obsolete ID mappings when a position is replaced", () => {
    const cache = new ClipboardCacheManager();
    cache.addItems([item(1)], 0);
    cache.addItems([item(2)], 0);
    expect(cache.getIndexById(1)).toBeUndefined();
    expect(cache.getIndexById(2)).toBe(0);
  });
});
