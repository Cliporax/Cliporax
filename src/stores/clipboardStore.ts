import { create } from "zustand";
import { ItemType, type ClipboardItem } from "../types/generated/api";
import { createLogger } from "../utils/logger";

const logger = createLogger("clipboardStore");

// Loaded range information
interface LoadedRange {
  start: number; // Database OFFSET
  end: number; // End index, exclusive
}

interface ClipboardState {
  items: ClipboardItem[];
  totalCount: number;
  activeTabId: number | null;
  loadingCount: number; // Supports concurrent loading
  error: string | null;
  hasMore: boolean;
  cursor: number;
  // Preloaded type information
  itemTypes: Array<[number, string]>; // [id, type]
  // Loaded ranges used to determine whether a position has been loaded
  loadedRanges: LoadedRange[];

  fetchItems: (tabId: number, limit?: number) => Promise<void>;
  fetchTotalCount: (tabId: number) => Promise<void>;
  fetchItemTypes: (tabId: number) => Promise<void>;
  loadMore: () => Promise<void>;
  loadRange: (start: number, count: number) => Promise<void>;
  prependItem: (item: ClipboardItem) => void;
  updateItem: (id: number, updates: Partial<ClipboardItem>) => void;
  removeItem: (id: number) => void;
  clearError: () => void;
  // Check whether a range has been loaded
  isRangeLoaded: (start: number, end: number) => boolean;
}

// Merge loaded ranges
function mergeRanges(ranges: LoadedRange[]): LoadedRange[] {
  if (ranges.length === 0) return [];

  // Sort by start position
  const sorted = [...ranges].sort((a, b) => a.start - b.start);
  const merged: LoadedRange[] = [];

  for (const range of sorted) {
    if (merged.length === 0) {
      merged.push(range);
    } else {
      const last = merged[merged.length - 1];
      // Merge when the current range is adjacent to or overlaps the previous range
      if (range.start <= last.end) {
        last.end = Math.max(last.end, range.end);
      } else {
        merged.push(range);
      }
    }
  }

  return merged;
}

export const useClipboardStore = create<ClipboardState>()((set, get) => ({
  items: [],
  totalCount: 0,
  activeTabId: null,
  loadingCount: 0,
  error: null,
  hasMore: true,
  cursor: 0,
  itemTypes: [],
  loadedRanges: [],

  // Check whether a range has been loaded
  isRangeLoaded: (start: number, end: number): boolean => {
    const { loadedRanges } = get();
    for (const range of loadedRanges) {
      if (range.start <= start && range.end >= end) {
        return true;
      }
    }
    return false;
  },

  fetchItems: async (tabId, limit = 100) => {
    set({ activeTabId: tabId, loadingCount: 1, error: null, loadedRanges: [] });
    try {
      const { clipboard } = await import("../lib/tauri-api");
      const [items, totalCount] = await Promise.all([
        clipboard.getByTab(tabId, limit, 0),
        clipboard.getTotalCount(tabId),
      ]);
      set({
        items,
        totalCount,
        hasMore: items.length < totalCount,
        cursor: limit,
        loadingCount: 0,
        loadedRanges: [{ start: 0, end: items.length }],
      });
    } catch (err) {
      set({ error: String(err), loadingCount: 0 });
    }
  },

  fetchTotalCount: async (tabId) => {
    try {
      const { clipboard } = await import("../lib/tauri-api");
      const totalCount = await clipboard.getTotalCount(tabId);
      set({ totalCount });
    } catch (err) {
      logger.error("Failed to fetch total count:", err);
    }
  },

  fetchItemTypes: async (tabId) => {
    try {
      const { clipboard } = await import("../lib/tauri-api");
      const types = await clipboard.getAllTypes(tabId);
      set({ itemTypes: types });
    } catch (err) {
      logger.error("Failed to fetch item types:", err);
    }
  },

  loadMore: async () => {
    const { items, cursor, hasMore, loadingCount, loadedRanges, activeTabId } =
      get();
    if (!hasMore || loadingCount > 0) return;
    if (activeTabId === null) return;

    set({ loadingCount: loadingCount + 1 });
    try {
      const { clipboard } = await import("../lib/tauri-api");
      const newItems = await clipboard.getByTab(activeTabId, 100, cursor);
      const newLoadedRanges = mergeRanges([
        ...loadedRanges,
        { start: cursor, end: cursor + newItems.length },
      ]);
      set({
        items: [...items, ...newItems],
        cursor: cursor + newItems.length,
        hasMore: newItems.length === 100,
        loadingCount,
        loadedRanges: newLoadedRanges,
      });
    } catch (err) {
      set({ error: String(err), loadingCount });
    }
  },

  // Load data by database OFFSET
  loadRange: async (start: number, count: number) => {
    const { loadingCount, loadedRanges, totalCount, activeTabId } = get();
    if (activeTabId === null) return;

    // Bounds check
    if (start < 0 || (totalCount > 0 && start >= totalCount)) {
      logger.debug(
        `[loadRange] Invalid range: start=${start}, totalCount=${totalCount}`,
      );
      return;
    }

    // Check whether the requested range is fully loaded
    const end = Math.min(start + count, totalCount || Infinity);
    let isLoaded = false;
    for (const range of loadedRanges) {
      if (range.start <= start && range.end >= end) {
        isLoaded = true;
        break;
      }
    }
    if (isLoaded) {
      logger.debug(`[loadRange] Range ${start}-${end} already loaded`);
      return;
    }

    // Increment the loading count
    set({ loadingCount: loadingCount + 1 });

    try {
      const { clipboard } = await import("../lib/tauri-api");
      const newItems = await clipboard.getByTab(activeTabId, count, start);
      logger.debug(
        `[loadRange] Loaded ${newItems.length} items from offset ${start}`,
      );

      if (newItems.length === 0) {
        set({ loadingCount: get().loadingCount - 1 });
        return;
      }

      // Get the current state
      const currentItems = get().items;
      const currentLoadedRanges = get().loadedRanges;

      // Placeholder used to fill gaps
      const placeholder: ClipboardItem = {
        id: null,
        type: ItemType.Text,
        content: "",
        content_hash: null,
        is_sensitive: false,
        is_pinned: false,
        created_at: null,
        updated_at: null,
      };

      // Important: keep indexes consistent
      // items[start] must correspond to the start-th database record
      let updatedItems: ClipboardItem[];

      if (currentItems.length === 0) {
        // After the initial load or a full clear
        updatedItems = [];
        // Fill with placeholders up to start
        for (let i = 0; i < start; i++) {
          updatedItems.push(placeholder);
        }
        // Add new data
        updatedItems.push(...newItems);
      } else if (start >= currentItems.length) {
        // Append to the end with a gap
        updatedItems = [...currentItems];
        // Fill the gap
        while (updatedItems.length < start) {
          updatedItems.push(placeholder);
        }
        // Add new data
        updatedItems.push(...newItems);
      } else {
        // Insert in the middle or overwrite existing data
        updatedItems = [...currentItems];
        for (let i = 0; i < newItems.length; i++) {
          const targetIndex = start + i;
          if (targetIndex < updatedItems.length) {
            // Overwrite only when the target is a placeholder to avoid replacing loaded data
            if (updatedItems[targetIndex].id === null) {
              updatedItems[targetIndex] = newItems[i];
            }
          } else {
            updatedItems.push(newItems[i]);
          }
        }
      }

      // Update loaded ranges
      const newLoadedRanges = mergeRanges([
        ...currentLoadedRanges,
        { start, end: start + newItems.length },
      ]);

      set({
        items: updatedItems,
        loadingCount: get().loadingCount - 1,
        loadedRanges: newLoadedRanges,
      });

      logger.debug(
        `[loadRange] Cache updated: items.length=${updatedItems.length}, loadedRanges=${JSON.stringify(newLoadedRanges)}`,
      );
    } catch (err) {
      logger.error("Failed to load range:", err);
      set({ error: String(err), loadingCount: get().loadingCount - 1 });
    }
  },

  prependItem: (item) =>
    set((state) => {
      // Update offsets for all loaded ranges
      const newLoadedRanges = state.loadedRanges.map((r) => ({
        start: r.start + 1,
        end: r.end + 1,
      }));
      return {
        items: [item, ...state.items],
        totalCount: state.totalCount + 1,
        loadedRanges: [{ start: 0, end: 1 }, ...newLoadedRanges],
      };
    }),

  updateItem: (id, updates) =>
    set((state) => ({
      items: state.items.map((item) =>
        item.id === id ? { ...item, ...updates } : item,
      ),
    })),

  removeItem: (id) =>
    set((state) => {
      // Find the deleted item index
      const index = state.items.findIndex((item) => item.id === id);
      if (index === -1) return state;

      const newItems = state.items.filter((item) => item.id !== id);

      // Update loaded ranges; ranges after the removed element need adjustment
      const newLoadedRanges = state.loadedRanges
        .map((r) => {
          if (r.start > index) {
            return { start: r.start - 1, end: r.end - 1 };
          } else if (r.end > index) {
            return { start: r.start, end: r.end - 1 };
          }
          return r;
        })
        .filter((r) => r.start < r.end);

      return {
        items: newItems,
        totalCount: state.totalCount - 1,
        loadedRanges: mergeRanges(newLoadedRanges),
      };
    }),

  clearError: () => set({ error: null }),
}));

export const useVisibleItems = (start: number, end: number) =>
  useClipboardStore((state) => state.items.slice(start, end));
