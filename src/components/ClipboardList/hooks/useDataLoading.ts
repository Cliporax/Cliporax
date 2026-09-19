import { useCallback, useRef } from "react";
import { createLogger } from "../../../utils/logger";
import { perfMeasure } from "../../../utils/perf";
import {
  tabs,
  clipboard,
  type ClipboardChangedPayload,
} from "../../../lib/tauri-api";
import { PAGE_SIZE, OVERSCAN, TYPE_PRELOAD_LIMIT } from "../constants";
import type { ItemTypeCache, ClipboardCacheManager } from "../cache";

const logger = createLogger("ClipboardList");

export interface UseDataLoadingParams {
  defaultTabId: number | null;
  totalCount: number;
  isAutoCaptureTab: boolean;
  isSearchMode: boolean;
  visibleStartIndex: number;
  visibleEndIndex: number;
  isMultiDraggingRef: React.MutableRefObject<boolean>;
  cacheManagerRef: React.MutableRefObject<ClipboardCacheManager>;
  typeCacheRef: React.MutableRefObject<ItemTypeCache>;
  containerRef: React.RefObject<HTMLDivElement | null>;
  setTotalCount: (count: number) => void;
  setIsLoading: (loading: boolean) => void;
  setCacheVersion: (updater: (prev: number) => number) => void;
}

export interface UseDataLoadingReturn {
  loadRange: (startIndex: number, count: number) => Promise<boolean>;
  checkAndLoadMissing: () => void;
  forceLoadVisibleRange: () => void;
  refreshList: () => Promise<void>;
  initializeData: () => Promise<void>;
  incrementalUpdate: (payload?: ClipboardChangedPayload | null) => Promise<void>;
}

export function useDataLoading({
  defaultTabId,
  totalCount,
  isAutoCaptureTab,
  isSearchMode,
  visibleStartIndex,
  visibleEndIndex,
  isMultiDraggingRef,
  cacheManagerRef,
  typeCacheRef,
  containerRef,
  setTotalCount,
  setIsLoading,
  setCacheVersion,
}: UseDataLoadingParams): UseDataLoadingReturn {
  const loadingCountRef = useRef(0);
  const visibleStartRef = useRef(visibleStartIndex);
  visibleStartRef.current = visibleStartIndex;
  const activeTabIdRef = useRef<number | null>(defaultTabId);
  activeTabIdRef.current = defaultTabId;

  // Performance statistics
  const scrollStartTimeRef = useRef<number>(0);
  const scrollStopTimeRef = useRef<number>(0);
  const loadStartTimeRef = useRef<number>(0);

  // Load data with independent requests and range locks
  const loadRange = useCallback(
    async (startIndex: number, count: number): Promise<boolean> => {
      if (defaultTabId === null) return false;
      const requestTabId = defaultTabId;
      const requestRevision = cacheManagerRef.current.getRevision();

      const endIndex = startIndex + count - 1;

      // Check whether it is already in cache
      if (cacheManagerRef.current.isRangeLoaded(startIndex, endIndex)) {
        return false;
      }

      // Use range locks instead of a global lock
      if (!cacheManagerRef.current.startLoading(startIndex, endIndex)) {
        return false; // This range is already loading
      }

      // Record load start time on the first load
      if (loadStartTimeRef.current === 0) {
        loadStartTimeRef.current = performance.now();
        const waitTime = loadStartTimeRef.current - scrollStopTimeRef.current;
        logger.info(
          `[Perf] Load started - wait time: ${waitTime.toFixed(0)}ms, startIndex: ${startIndex}`,
        );
      }

      // Increment loading count and set loading state
      loadingCountRef.current++;
      if (loadingCountRef.current === 1) {
        setIsLoading(true);
      }

      const requestStartTime = performance.now();
      try {
        logger.debug(
          `[LoadRange] Loading ${count} items from index ${startIndex}`,
        );
        const items = await clipboard.getByTab(requestTabId, count, startIndex);

        const requestDuration = performance.now() - requestStartTime;
        logger.info(
          `[Perf] Data returned - duration: ${requestDuration.toFixed(0)}ms, items: ${items.length}`,
        );

        if (activeTabIdRef.current !== requestTabId || cacheManagerRef.current.getRevision() !== requestRevision) {
          logger.debug(
            `[LoadRange] Ignoring stale response for tab ${requestTabId}`,
          );
          return false;
        }

        if (items.length > 0) {
          cacheManagerRef.current.addItems(items, startIndex);
          typeCacheRef.current.setTypes(
            items
              .filter((item) => item.id !== null)
              .map((item) => ({
                id: item.id as number,
                type: item.type as "text" | "image" | "file",
              })),
            startIndex,
          );
          // Trigger rerender
          setCacheVersion((prev) => prev + 1);
        }
        perfMeasure("ClipboardList", "load-range", requestStartTime, {
          startIndex,
          requestedCount: count,
          returnedCount: items.length,
        }, {
          minIntervalMs: 300,
          warnAtMs: 120,
        });
        return true;
      } catch (error) {
        logger.error("[LoadRange] Failed to load:", error);
        return false;
      } finally {
        if (cacheManagerRef.current.getRevision() === requestRevision) {
          cacheManagerRef.current.finishLoading(startIndex, endIndex);
        }
        // Decrement loading count and clear loading state
        loadingCountRef.current--;
        if (loadingCountRef.current === 0) {
          setIsLoading(false);
          // All loads completed; output total time
          const totalDuration = performance.now() - scrollStartTimeRef.current;
          logger.info(
            `[Perf] Load complete - total duration: ${totalDuration.toFixed(0)}ms (scroll + wait + load)`,
          );
          // Reset the timer
          scrollStartTimeRef.current = 0;
          loadStartTimeRef.current = 0;
        }
      }
    },
    [defaultTabId, cacheManagerRef, setIsLoading, setCacheVersion],
  );

  // Check and load missing data in parallel without blocking
  const checkAndLoadMissing = useCallback(() => {
    if (defaultTabId === null || totalCount === 0 || isSearchMode) return;

    const container = containerRef.current;
    if (!container) return;

    const currentScrollTop = container.scrollTop;
    const currentViewportHeight = container.clientHeight;

    const { start: currentStart, end: currentEnd } =
      typeCacheRef.current.findVisibleRange(
        currentScrollTop,
        currentViewportHeight,
        totalCount,
      );

    const loadStart = Math.max(0, currentStart - OVERSCAN);
    const loadEnd = Math.min(totalCount - 1, currentEnd + OVERSCAN);

    const missingRanges = cacheManagerRef.current.getMissingRanges(
      loadStart,
      loadEnd,
    );

    // Load all missing ranges in parallel instead of waiting serially
    missingRanges.forEach((range) => {
      loadRange(range.start, Math.min(PAGE_SIZE, range.end - range.start + 1));
    });
  }, [
    defaultTabId,
    totalCount,
    isSearchMode,
    loadRange,
    containerRef,
    typeCacheRef,
    cacheManagerRef,
  ]);

  // Force-load visible area data
  const forceLoadVisibleRange = useCallback(() => {
    checkAndLoadMissing();
  }, [checkAndLoadMissing]);

  // Structural events invalidate positional caches. Never infer insertion offsets
  // from a partial cache: deduplication, pruning and pinned items can all reorder it.
  const refreshList = useCallback(async () => {
    cacheManagerRef.current.clear();
    typeCacheRef.current.clear();
    const revision = cacheManagerRef.current.getRevision();
    const isCurrent = () => activeTabIdRef.current === defaultTabId &&
      cacheManagerRef.current.getRevision() === revision;
    setCacheVersion((prev) => prev + 1);
    if (defaultTabId === null) return;
    try {
      const count = await clipboard.getTotalCount(defaultTabId);
      if (!isCurrent()) return;
      setTotalCount(count);
      if (count > 0 && count <= TYPE_PRELOAD_LIMIT) {
        const types = await clipboard.getAllTypes(defaultTabId);
        if (!isCurrent()) return;
        typeCacheRef.current.setTypes(types.map(([id, type]) => ({
          id, type: type as "text" | "image" | "file",
        })), 0);
      }
      if (!isCurrent()) return;
      setCacheVersion((prev) => prev + 1);
      if (count > 0) {
        const start = Math.min(Math.max(0, visibleStartRef.current - OVERSCAN), count - 1);
        await loadRange(start, Math.min(PAGE_SIZE, count - start));
      }
    } catch (error) {
      logger.error("Failed to refresh clipboard list:", error);
    }
  }, [defaultTabId, loadRange, setTotalCount,
      setCacheVersion, cacheManagerRef, typeCacheRef]);

  // Initialize
  const initializeData = useCallback(async () => {
    logger.info("Initializing...");
    const allTabs = await tabs.getAll();
    const systemTab = allTabs.find((t) => t.is_default) || allTabs[0];
    if (systemTab && systemTab.id) {
      // Return tab ID so the caller can set it
      const tabId = systemTab.id;

      try {
        // Get total count
        const count = await clipboard.getTotalCount(tabId);
        logger.info("Total count:", count);
        setTotalCount(count);

        // Preload type information for all items for dynamic height calculation
        if (count > 0 && count <= TYPE_PRELOAD_LIMIT) {
          logger.info("Preloading item types for height calculation...");
          const types = await clipboard.getAllTypes(tabId);
          typeCacheRef.current.setTypes(
            types.map(([id, type]) => ({
              id,
              type: type as "text" | "image" | "file",
            })),
            0,
          );
          logger.info(`Preloaded ${types.length} item types`);
          // Trigger rerender to update height calculation
          setCacheVersion((prev) => prev + 1);
        } else if (count > TYPE_PRELOAD_LIMIT) {
          logger.info(
            `[Init] Skipping full type preload for ${count} items`,
          );
        }
      } catch (error) {
        logger.error("Failed to initialize:", error);
      }
    }
  }, [setTotalCount, setCacheVersion, typeCacheRef]);

  // Reconcile all affected positions, including batch events and moved duplicates.
  const incrementalUpdate = useCallback(async (payload?: ClipboardChangedPayload | null) => {
    if (defaultTabId === null || isMultiDraggingRef.current) return;
    if (payload?.tabIds?.length && !payload.tabIds.includes(defaultTabId)) return;
    if (!isAutoCaptureTab && !payload?.tabIds?.includes(defaultTabId)) return;
    await refreshList();
  }, [defaultTabId, isAutoCaptureTab, isMultiDraggingRef, refreshList]);

  return {
    loadRange,
    checkAndLoadMissing,
    forceLoadVisibleRange,
    refreshList,
    initializeData,
    incrementalUpdate,
  };
}
