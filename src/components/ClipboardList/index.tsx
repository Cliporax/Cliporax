import React, {
  useEffect,
  useLayoutEffect,
  useState,
  useRef,
  useCallback,
  forwardRef,
  useImperativeHandle,
} from "react";
import ClipboardCard from "../ClipboardCard";
import { createLogger } from "../../utils/logger";
import { perfLog, perfMeasure } from "../../utils/perf";
import { clipboard, window as windowApi, events } from "../../lib/tauri-api";
import { useTheme } from "../../contexts/ThemeContext";
import { useTabStore } from "../../stores/tabStore";

// Import split modules
import { ClipboardListRef, ClipboardListProps } from "./types";
export type { ClipboardListRef, ClipboardListProps };
import {
  IMAGE_HEIGHT,
  CARD_GAP,
  CONTENT_PADDING_TOP,
  CONTENT_PADDING_BOTTOM,
  CONTENT_PADDING_LEFT,
  CONTENT_PADDING_RIGHT,
  SCROLLBAR_WIDTH,
  SCROLLBAR_GAP,
  OVERSCAN,
  TYPE_PRELOAD_LIMIT,
  getTextHeight,
  createTransparentDragImage,
} from "./constants";
import { ItemTypeCache, ClipboardCacheManager } from "./cache";
import { SkeletonCard } from "./ui/SkeletonCard";
import { DropIndicator } from "./DropIndicator";
import { Scrollbar } from "./ui/Scrollbar";
import { StatusOverlay } from "./ui/StatusOverlay";
import {
  useDataLoading,
  useDeleteHandler,
  useDragReorder,
  useMultiSelect,
  useMouseDrag,
  useOverlapDetection,
} from "./hooks";
import {
  calculateScrollbarMetrics,
  calculateSearchContentHeight,
  calculateSearchVisibleRange,
  collectVisibleItems,
  shouldUseMouseReorder,
} from "./virtualization";

const logger = createLogger("ClipboardList");

const isMacOS =
  /Mac|iPod|iPhone|iPad/.test(navigator.platform) ||
  (navigator.userAgent.includes("Mac") && !("ontouchend" in document));
const useMouseReorder = shouldUseMouseReorder(
  navigator.platform,
  navigator.userAgent,
  "ontouchend" in document,
);

// Global transparent drag image element, reused
let transparentDragImage: HTMLElement | null = null;

// ============================================================
// Main component
// ============================================================

const ClipboardList = forwardRef<ClipboardListRef, ClipboardListProps>(
  (
    {
      searchQuery = "",
      searchMode = "fuzzy",
      searchScope = "current",
      lineHeight = "medium",
      tabId = null,
      onEdit,
      refreshTrigger,
      onMultiSelectChange,
    },
    ref,
  ) => {
    const mountStartRef = useRef(performance.now());
    const { resolvedTheme } = useTheme();
    const isDark = resolvedTheme === "dark";
    const tabs = useTabStore((state) => state.tabs);

    // Base state
    const [defaultTabId, setDefaultTabId] = useState<number | null>(null);
    const [loadedTabId, setLoadedTabId] = useState<number | null>(null);
    const [selectedId, setSelectedId] = useState<number | null>(null);
    const [refreshKey, setRefreshKey] = useState(0);

    // Virtual scrolling state
    const [totalCount, setTotalCount] = useState(0);
    const [isLoading, setIsLoading] = useState(false);

    // Search-related state
    const [searchResults, setSearchResults] = useState<any[]>([]);
    const [isSearching, setIsSearching] = useState(false);
    const searchDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(
      null,
    );

    // Multi-select state
    const [isMultiSelectMode, setIsMultiSelectMode] = useState(false);
    const [checkedIds, setCheckedIds] = useState<Set<number>>(new Set());

    // Range selection state
    const [selectionRange, setSelectionRange] = useState<{
      start: number;
      end: number;
    } | null>(null);

    // Drag reordering state - simplified so there is only one insertion point between two items
    const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
    const [draggedId, setDraggedId] = useState<number | null>(null);

    // Cache manager
    const cacheManagerRef = useRef(new ClipboardCacheManager(2000));
    const typeCacheRef = useRef(new ItemTypeCache());

    // Cache version
    const [cacheVersion, setCacheVersion] = useState(0);

    // Sync lineHeight to ItemTypeCache
    useEffect(() => {
      typeCacheRef.current.setLineHeight(lineHeight);
      setCacheVersion((prev) => prev + 1);
    }, [lineHeight]);

    // Scroll-related state
    const containerRef = useRef<HTMLDivElement>(null);
    const contentRef = useRef<HTMLDivElement>(null);
    const [scrollTop, setScrollTop] = useState(0);
    const [viewportHeight, setViewportHeight] = useState(0);

    // Independent scroll position for each tab
    const tabScrollPositionsRef = useRef<Map<number, number>>(new Map());
    const pendingScrollRestoreRef = useRef<{
      tabId: number;
      scrollTop: number;
    } | null>(null);
    const tabLoadSeqRef = useRef(0);

    // Custom scrollbar state
    const [isDraggingScrollbar, setIsDraggingScrollbar] = useState(false);
    const scrollbarDragStartY = useRef(0);
    const scrollbarDragStartTop = useRef(0);

    // Scroll-stop detection
    const scrollStopTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
      null,
    );
    const scrollLoadTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
      null,
    );
    const wheelLogTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
      null,
    );
    const resizeFrameRef = useRef<number | null>(null);
    const scrollFrameRef = useRef<number | null>(null);
    const lastScrollTopRef = useRef(0);
    const wheelStatsRef = useRef({
      startMs: 0,
      lastMs: 0,
      events: 0,
      deltaY: 0,
      scrollEvents: 0,
      maxScrollDelayMs: 0,
    });

    // Performance statistics
    const scrollStartTimeRef = useRef<number>(0);
    const scrollStopTimeRef = useRef<number>(0);

    // Whether search mode is active
    const isSearchMode = !!(searchQuery && searchQuery.trim() !== "");
    const currentTab = tabs.find((tab) => tab.id === defaultTabId);
    const isAutoCaptureTab = Boolean(currentTab?.is_default);

    useEffect(() => {
      perfMeasure("ClipboardList", "mounted", mountStartRef.current, undefined, {
        minIntervalMs: 0,
        warnAtMs: 100,
      });
    }, []);

    const loadTabData = useCallback(
      async (
        nextTabId: number,
        savedScroll: number,
        source: "init" | "switch" | "fallback",
      ) => {
        const seq = tabLoadSeqRef.current + 1;
        tabLoadSeqRef.current = seq;

        logger.info(`[TabLoad] ${source} loading tab ${nextTabId}`);

        setDefaultTabId(nextTabId);
        setLoadedTabId(null);
        setTotalCount(0);
        setIsLoading(true);
        setSearchResults([]);
        cacheManagerRef.current.clear();
        typeCacheRef.current.clear();
        setCacheVersion((prev) => prev + 1);

        pendingScrollRestoreRef.current = { tabId: nextTabId, scrollTop: savedScroll };
        if (containerRef.current) {
          containerRef.current.scrollTop = savedScroll;
        }
        setScrollTop(savedScroll);

        try {
          const countStart = performance.now();
          const count = await clipboard.getTotalCount(nextTabId);

          if (tabLoadSeqRef.current !== seq) {
            logger.debug(`[TabLoad] Ignoring stale count for tab ${nextTabId}`);
            return;
          }

          perfMeasure("ClipboardList", `${source}-count-loaded`, countStart, {
            tabId: nextTabId,
            totalCount: count,
          }, {
            minIntervalMs: 0,
            warnAtMs: 100,
          });
          logger.info(`Total count for tab ${nextTabId}:`, count);
          setTotalCount(count);

          if (count > 0 && count <= TYPE_PRELOAD_LIMIT) {
            const typeStart = performance.now();
            const types = await clipboard.getAllTypes(nextTabId);

            if (tabLoadSeqRef.current !== seq) {
              logger.debug(`[TabLoad] Ignoring stale types for tab ${nextTabId}`);
              return;
            }

            typeCacheRef.current.setTypes(
              types.map(([id, type]) => ({
                id,
                type: type as "text" | "image" | "file",
              })),
              0,
            );
            perfMeasure("ClipboardList", `${source}-types-loaded`, typeStart, {
              tabId: nextTabId,
              typeCount: types.length,
            }, {
              minIntervalMs: 0,
              warnAtMs: 150,
            });
            setCacheVersion((prev) => prev + 1);
          } else if (count > TYPE_PRELOAD_LIMIT) {
            logger.info(`[TabLoad] Skipping full type preload for ${count} items`);
          }

          if (tabLoadSeqRef.current === seq) {
            setLoadedTabId(nextTabId);
          }
        } catch (error) {
          if (tabLoadSeqRef.current === seq) {
            logger.error(`[TabLoad] Failed to load tab ${nextTabId}:`, error);
          }
        } finally {
          if (tabLoadSeqRef.current === seq) {
            setIsLoading(false);
          }
        }
      },
      [],
    );

    // ========== Use split hooks ==========

    // Data loading hook
    const {
      loadRange,
      checkAndLoadMissing,
      forceLoadVisibleRange,
      refreshList,
      initializeData,
      incrementalUpdate,
    } = useDataLoading({
      defaultTabId,
      totalCount,
      isAutoCaptureTab,
      isSearchMode,
      visibleStartIndex: 0, // Calculated later
      visibleEndIndex: 0, // Calculated later
      isMultiDraggingRef: { current: false }, // Provided later by useMultiSelect
      cacheManagerRef,
      typeCacheRef,
      containerRef,
      setTotalCount,
      setIsLoading,
      setCacheVersion,
    });
    const activeTabIdRef = useRef<number | null>(defaultTabId);
    const incrementalUpdateRef = useRef(incrementalUpdate);

    useEffect(() => {
      activeTabIdRef.current = defaultTabId;
      incrementalUpdateRef.current = incrementalUpdate;
    }, [defaultTabId, incrementalUpdate]);

    // Multi-select hook
    const {
      lastCheckedIndexRef,
      isMultiDraggingRef,
      isPastingRef,
      isIndexInRange,
      handleCardClick,
      enterMultiSelectMode,
      exitMultiSelectMode,
      getMergedText,
      handleMultiDragStart,
      handleMultiDragEnd,
    } = useMultiSelect({
      selectedId,
      isMultiSelectMode,
      checkedIds,
      selectionRange,
      cacheManagerRef,
      setIsMultiSelectMode,
      setCheckedIds,
      setSelectionRange,
      setSelectedId,
      onMultiSelectChange,
    });

    // Delete handler hook
    const handleDeleteSelected = useDeleteHandler({
      selectedId,
      isMultiSelectMode,
      checkedIds,
      selectionRange,
      defaultTabId,
      isSearchMode,
      searchResults,
      cacheManagerRef,
      typeCacheRef,
      setTotalCount,
      setSelectedId,
      setCacheVersion,
      setIsMultiSelectMode,
      setCheckedIds,
      setSelectionRange,
      setSearchResults,
    });

    // Drag reordering hook (HTML5 - Linux)
    const {
      isReorderModeRef,
      stopAutoScroll,
      startAutoScroll,
      handleCardDragOver,
      handleCardDragLeave,
      handleCardDrop,
      handleItemDragStart,
      handleItemDragEnd,
    } = useDragReorder({
      draggedId,
      defaultTabId,
      isSearchMode,
      cacheManagerRef,
      typeCacheRef,
      containerRef,
      setDragOverIndex,
      setDraggedId,
      setCacheVersion,
      setSelectedId,
    });

    // Mouse drag hook for macOS / Windows WebView compatibility
    const {
      draggedId: mouseDraggedId,
      dragOverIndex: mouseDragOverIndex,
      handleMouseDown,
      handleMouseMove: handleCardMouseMove,
      handleMouseUp,
      handleMouseLeave: handleCardMouseLeave,
    } = useMouseDrag({
      defaultTabId,
      isSearchMode,
      cacheManagerRef,
      typeCacheRef,
      containerRef,
      setCacheVersion,
      setSelectedId,
    });

    // Choose drag state based on platform
    const activeDraggedId = useMouseReorder ? mouseDraggedId : draggedId;
    const activeDragOverIndex = useMouseReorder ? mouseDragOverIndex : dragOverIndex;

    // ========== Initialization and data listeners ==========

    // Sync tabId prop to defaultTabId state - handles Tab switching
    useEffect(() => {
      if (tabId === null || tabId === defaultTabId) return;

      logger.info(`Tab changed from ${defaultTabId} to ${tabId}`);

      // Save scroll position for old tab
      if (defaultTabId !== null) {
        const currentScrollTop = containerRef.current?.scrollTop ?? scrollTop;
        tabScrollPositionsRef.current.set(defaultTabId, currentScrollTop);
        logger.debug(
          `Saved scroll position for tab ${defaultTabId}: ${currentScrollTop}`,
        );
      }

      const savedScroll = tabScrollPositionsRef.current.get(tabId) || 0;
      logger.debug(
        `Restoring scroll position for tab ${tabId}: ${savedScroll}`,
      );
      loadTabData(tabId, savedScroll, "switch");
    }, [defaultTabId, loadTabData, scrollTop, tabId]);

    // Initialize
    useEffect(() => {
      if (defaultTabId !== null) return;

      const init = async () => {
        const initStart = performance.now();
        logger.info("Initializing...");
        perfLog("ClipboardList", "init-start", undefined, { minIntervalMs: 0 });
        const allTabs = await (
          await import("../../lib/tauri-api")
        ).tabs.getAll();
        perfMeasure("ClipboardList", "tabs-loaded", initStart, {
          tabCount: allTabs.length,
        }, {
          minIntervalMs: 0,
          warnAtMs: 100,
        });
        const systemTab = allTabs.find((t) => t.is_default) || allTabs[0];
        const initialTabId = tabId ?? systemTab?.id ?? null;
        if (initialTabId) {
          await loadTabData(initialTabId, 0, tabId ? "init" : "fallback");
          perfMeasure("ClipboardList", "init-complete", initStart, {
            tabId: initialTabId,
          }, {
            minIntervalMs: 0,
            warnAtMs: 250,
          });
        }
      };
      init();
    }, [defaultTabId, loadTabData, tabId]);

    // Listen for clipboard actions (move/copy) and refresh
    useEffect(() => {
      const handleClipboardAction = (e: Event) => {
        const { action, itemId } = (e as CustomEvent).detail as {
          action: "move" | "copy" | "delete";
          itemId: number;
        };
        logger.info(
          `Clipboard action received: ${action} for item ${itemId}`,
        );

        if (action === "move" || action === "delete") {
          // Incremental delete: remove only the affected item, don't clear all caches
          const index = cacheManagerRef.current.getIndexById(itemId);
          if (index !== undefined) {
            cacheManagerRef.current.removeAtIndex(index);
            typeCacheRef.current.removeAtIndex(index);
            setTotalCount((prev) => Math.max(0, prev - 1));
            setCacheVersion((prev) => prev + 1);
            logger.debug(
              `[ClipboardAction] Incrementally removed item ${itemId} at index ${index}`,
            );
          } else {
            // Item not in cache (e.g. scrolled far away), just update count
            if (defaultTabId !== null) {
              clipboard
                .getTotalCount(defaultTabId)
                .then(setTotalCount)
                .catch((err) =>
                  logger.error("Failed to get total count:", err),
                );
            }
          }
        }
        if (action === "copy") {
          refreshList();
        }
      };

      window.addEventListener("clipboard:action", handleClipboardAction);
      return () =>
        window.removeEventListener("clipboard:action", handleClipboardAction);
    }, [defaultTabId, refreshList]);

    // Listen for container size changes
    useEffect(() => {
      const container = containerRef.current;
      if (!container) return;

      const resizeObserver = new ResizeObserver((entries) => {
        const entry = entries[0];
        if (!entry) return;

        const nextHeight = entry.contentRect.height;
        if (resizeFrameRef.current !== null) {
          cancelAnimationFrame(resizeFrameRef.current);
        }

        const resizeStart = performance.now();
        resizeFrameRef.current = requestAnimationFrame(() => {
          setViewportHeight((current) =>
            Math.abs(current - nextHeight) > 1 ? nextHeight : current,
          );
          perfMeasure("ClipboardList", "resize-viewport", resizeStart, {
            viewportHeight: Number(nextHeight.toFixed(0)),
          }, {
            minIntervalMs: 500,
            warnAtMs: 32,
          });
          resizeFrameRef.current = null;
        });
      });

      resizeObserver.observe(container);
      setViewportHeight(container.clientHeight);

      return () => {
        resizeObserver.disconnect();
        if (resizeFrameRef.current !== null) {
          cancelAnimationFrame(resizeFrameRef.current);
          resizeFrameRef.current = null;
        }
        if (scrollStopTimerRef.current) {
          clearTimeout(scrollStopTimerRef.current);
        }
        if (scrollLoadTimerRef.current) {
          clearTimeout(scrollLoadTimerRef.current);
        }
        if (wheelLogTimerRef.current) {
          clearTimeout(wheelLogTimerRef.current);
        }
        if (scrollFrameRef.current !== null) {
          cancelAnimationFrame(scrollFrameRef.current);
          scrollFrameRef.current = null;
        }
      };
    }, []);

    // ========== Virtual scrolling calculations ==========

    // Calculate content height using dynamic heights
    const contentHeight = isSearchMode
      ? calculateSearchContentHeight(searchResults, lineHeight)
      : typeCacheRef.current.getTotalHeight(totalCount);

    // Total height = content height + vertical padding
    const totalHeight =
      contentHeight + CONTENT_PADDING_TOP + CONTENT_PADDING_BOTTOM;

    // Calculate visible range
    const effectiveViewportHeight = viewportHeight || 500;
    const visibleContentTop = Math.max(0, scrollTop - CONTENT_PADDING_TOP);
    const visibleContentBottom = visibleContentTop + effectiveViewportHeight;

    const { start: visibleStartIndex, end: visibleEndIndex } = isSearchMode
      ? calculateSearchVisibleRange(searchResults, lineHeight, visibleContentTop, visibleContentBottom)
      : typeCacheRef.current.findVisibleRange(
          visibleContentTop,
          effectiveViewportHeight,
          totalCount,
        );

    // Get visible items
    const getVisibleItems = useCallback(() => {
      return collectVisibleItems({
        visibleStartIndex,
        visibleEndIndex,
        isSearchMode,
        searchResults,
        totalCount,
        lineHeight,
        getCachedItem: (index) => cacheManagerRef.current.getItem(index),
        getCachedPosition: (index) => typeCacheRef.current.getPosition(index, totalCount),
      });
    }, [
      visibleStartIndex,
      visibleEndIndex,
      isSearchMode,
      searchResults,
      totalCount,
      cacheVersion,
      lineHeight,
    ]);

    const { items: visibleItems, missingCount } = getVisibleItems();

    const selectedIdsForBatch = React.useMemo(() => {
      if (selectionRange) {
        const ids = new Set<number>();
        const start = Math.min(selectionRange.start, selectionRange.end);
        const end = Math.max(selectionRange.start, selectionRange.end);
        for (let i = start; i <= end; i++) {
          const item = cacheManagerRef.current.getItem(i);
          if (typeof item?.id === "number") {
            ids.add(item.id);
          }
        }
        return ids;
      }

      return checkedIds;
    }, [checkedIds, selectionRange, cacheVersion]);

    const handleBatchActionComplete = useCallback(() => {
      exitMultiSelectMode();
      refreshList();
    }, [exitMultiSelectMode, refreshList]);

    // ========== Overlap detection ==========
    const {
      getOverlapRecords,
      clearOverlapRecords,
      hasOverlaps,
    } = useOverlapDetection({
      visibleItems,
      lineHeight,
      cacheVersion,
    });

    // Whether fast scrolling is active
    const isFastScrolling =
      missingCount > 0 && !isSearchMode;

    // ========== Data loading triggers ==========

    const loadMissingForCurrentViewport = useCallback(() => {
      if (isSearchMode || totalCount === 0 || !containerRef.current) return;

      const container = containerRef.current;
      const currentScrollTop = container.scrollTop;
      const currentViewportHeight = container.clientHeight;

      const { start: currentStart, end: currentEnd } =
        typeCacheRef.current.findVisibleRange(
          currentScrollTop,
          currentViewportHeight,
          totalCount,
        );

      const PAGE_SIZE = 100;
      const loadStart = Math.max(0, currentStart - OVERSCAN);
      const loadEnd = Math.min(totalCount - 1, currentEnd + OVERSCAN);

      const missingRanges = cacheManagerRef.current.getMissingRanges(
        loadStart,
        loadEnd,
      );

      missingRanges.forEach((range) => {
        loadRange(
          range.start,
          Math.min(PAGE_SIZE, range.end - range.start + 1),
        );
      });
    }, [isSearchMode, totalCount, loadRange]);

    useLayoutEffect(() => {
      const pendingRestore = pendingScrollRestoreRef.current;
      const container = containerRef.current;

      if (
        !pendingRestore ||
        !container ||
        defaultTabId !== pendingRestore.tabId ||
        loadedTabId !== pendingRestore.tabId
      ) {
        return;
      }

      const maxRestorableScrollTop = Math.max(
        0,
        totalHeight - container.clientHeight,
      );
      const restoredScrollTop = Math.max(
        0,
        Math.min(pendingRestore.scrollTop, maxRestorableScrollTop),
      );

      container.scrollTop = restoredScrollTop;
      setScrollTop((current) =>
        Math.abs(current - restoredScrollTop) > 1 ? restoredScrollTop : current,
      );
      pendingScrollRestoreRef.current = null;

      requestAnimationFrame(() => {
        loadMissingForCurrentViewport();
      });

      logger.debug(
        `[TabRestore] Restored scrollTop ${restoredScrollTop} for tab ${pendingRestore.tabId}`,
      );
    }, [
      defaultTabId,
      loadedTabId,
      loadMissingForCurrentViewport,
      totalHeight,
    ]);

    const scheduleViewportLoad = useCallback(
      (delayMs: number) => {
        if (scrollLoadTimerRef.current) {
          clearTimeout(scrollLoadTimerRef.current);
        }
        scrollLoadTimerRef.current = setTimeout(() => {
          scrollLoadTimerRef.current = null;
          loadMissingForCurrentViewport();
        }, delayMs);
      },
      [loadMissingForCurrentViewport],
    );

    const flushWheelStats = useCallback(() => {
      const stats = wheelStatsRef.current;
      if (stats.events === 0) return;

      perfLog(
        "ClipboardList",
        "wheel-burst",
        {
          durationMs: Number((stats.lastMs - stats.startMs).toFixed(1)),
          wheelEvents: stats.events,
          scrollEvents: stats.scrollEvents,
          deltaY: Number(stats.deltaY.toFixed(0)),
          maxScrollDelayMs: Number(stats.maxScrollDelayMs.toFixed(1)),
          visibleStartIndex,
          visibleEndIndex,
          missingCount,
          renderedItems: visibleItems.length,
          totalCount,
        },
        { minIntervalMs: 250 },
      );

      stats.startMs = 0;
      stats.lastMs = 0;
      stats.events = 0;
      stats.deltaY = 0;
      stats.scrollEvents = 0;
      stats.maxScrollDelayMs = 0;
    }, [
      missingCount,
      totalCount,
      visibleEndIndex,
      visibleItems.length,
      visibleStartIndex,
    ]);

    const handleWheelCapture = useCallback(
      (e: React.WheelEvent<HTMLDivElement>) => {
        const now = performance.now();
        const stats = wheelStatsRef.current;
        if (stats.events === 0) {
          stats.startMs = now;
        }

        stats.lastMs = now;
        stats.events++;
        stats.deltaY += Math.abs(e.deltaY);

        if (wheelLogTimerRef.current) {
          clearTimeout(wheelLogTimerRef.current);
        }
        wheelLogTimerRef.current = setTimeout(() => {
          wheelLogTimerRef.current = null;
          flushWheelStats();
        }, 240);
      },
      [flushWheelStats],
    );

    // When cacheVersion changes, check whether more loading is needed
    useEffect(() => {
      loadMissingForCurrentViewport();
    }, [cacheVersion, loadMissingForCurrentViewport]);

    // Preload type information again when leaving search mode
    useEffect(() => {
      if (
        !isSearchMode &&
        defaultTabId !== null &&
        totalCount > 0 &&
        totalCount <= TYPE_PRELOAD_LIMIT
      ) {
        if (typeCacheRef.current.size() === 0) {
          logger.info("[ExitSearch] Reloading type cache...");
          clipboard.getAllTypes(defaultTabId).then((types) => {
            typeCacheRef.current.setTypes(
              types.map(([id, type]) => ({
                id,
                type: type as "text" | "image" | "file",
              })),
              0,
            );
            setCacheVersion((prev) => prev + 1);
            logger.info(`[ExitSearch] Reloaded ${types.length} item types`);
          });
        }
      }
    }, [isSearchMode, defaultTabId, totalCount]);

    // Initial load and data preload
    useEffect(() => {
      if (defaultTabId === null || totalCount === 0 || isSearchMode) return;
      loadMissingForCurrentViewport();
    }, [
      defaultTabId,
      totalCount,
      visibleStartIndex,
      visibleEndIndex,
      isSearchMode,
      loadMissingForCurrentViewport,
    ]);

    // Listen for external refresh triggers
    useEffect(() => {
      if (refreshTrigger !== undefined && refreshTrigger > 0) {
        logger.info(
          "[ClipboardList] Refresh triggered by prop, refreshTrigger:",
          refreshTrigger,
        );
        refreshList();
      }
    }, [refreshTrigger, refreshList]);

    // Listen for clipboard changes
    useEffect(() => {
      let disposed = false;
      let unlisten: (() => void) | null = null;

      events
        .onClipboardChanged((payload) => {
          const currentTabId = activeTabIdRef.current;
          if (
            payload?.tabIds &&
            currentTabId !== null &&
            !payload.tabIds.includes(currentTabId)
          ) {
            logger.debug(
              `[ClipboardList] Ignoring clipboard event for tabs ${payload.tabIds.join(",")}; current tab ${currentTabId}`,
            );
            return;
          }
          incrementalUpdateRef.current();
        })
        .then((nextUnlisten) => {
          if (disposed) {
            nextUnlisten();
          } else {
            unlisten = nextUnlisten;
          }
        })
        .catch((error) => {
          logger.error("[ClipboardList] Failed to register clipboard listener:", error);
        });

      return () => {
        disposed = true;
        unlisten?.();
      };
    }, []);

    // ========== Search ==========

    useEffect(() => {
      if (searchDebounceRef.current) {
        clearTimeout(searchDebounceRef.current);
      }

      if (!searchQuery || searchQuery.trim() === "") {
        setSearchResults([]);
        setIsSearching(false);
        return;
      }

      if (searchScope === "current" && defaultTabId === null) return;

      searchDebounceRef.current = setTimeout(async () => {
        setIsSearching(true);

        try {
          let actualQuery = searchQuery;
          if (
            searchMode === "regex" &&
            searchQuery.toLowerCase().startsWith("regx:")
          ) {
            actualQuery = searchQuery.slice(5).trim();
          }

          const results =
            searchScope === "global"
              ? await clipboard.search(actualQuery)
              : await clipboard.search(actualQuery, defaultTabId ?? undefined);

          if (
            searchMode === "regex" &&
            searchQuery.toLowerCase().startsWith("regx:")
          ) {
            const patternStr = searchQuery.slice(5).trim();
            try {
              const pattern = new RegExp(patternStr, "i");
              const filtered = results.filter((item: any) => {
                return item.type === "text" && pattern.test(item.content);
              });
              setSearchResults(filtered);
            } catch {
              setSearchResults([]);
            }
          } else {
            setSearchResults(results);
          }
        } catch (error) {
          logger.error("[Search] Backend search failed:", error);
          setSearchResults([]);
        } finally {
          setIsSearching(false);
        }
      }, 300);

      return () => {
        if (searchDebounceRef.current) {
          clearTimeout(searchDebounceRef.current);
        }
      };
    }, [searchQuery, searchMode, searchScope, defaultTabId]);

    // ========== Scroll handling ==========

    const handleScroll = useCallback(
      (e: React.UIEvent<HTMLDivElement>) => {
        const scrollEventStart = performance.now();
        const target = e.target as HTMLDivElement;
        const newScrollTop = target.scrollTop;
        const wheelStats = wheelStatsRef.current;

        if (wheelStats.events > 0) {
          const scrollDelay = scrollEventStart - wheelStats.lastMs;
          wheelStats.scrollEvents++;
          wheelStats.maxScrollDelayMs = Math.max(
            wheelStats.maxScrollDelayMs,
            scrollDelay,
          );
        }

        if (scrollStartTimeRef.current === 0) {
          scrollStartTimeRef.current = performance.now();
          logger.info(
            `[Perf] Scroll started - scrollTop: ${newScrollTop.toFixed(0)}`,
          );
        }

        setScrollTop(newScrollTop);
        lastScrollTopRef.current = newScrollTop;

        if (scrollFrameRef.current === null) {
          scrollFrameRef.current = requestAnimationFrame(() => {
            perfMeasure("ClipboardList", "scroll-next-frame", scrollEventStart, {
              scrollTop: Number(newScrollTop.toFixed(0)),
              visibleStartIndex,
              visibleEndIndex,
              missingCount,
              renderedItems: visibleItems.length,
            }, {
              minIntervalMs: 500,
              warnAtMs: 16,
            });
            scrollFrameRef.current = null;
          });
        }

        if (scrollStopTimerRef.current) {
          clearTimeout(scrollStopTimerRef.current);
        }

        scrollStopTimerRef.current = setTimeout(() => {
          scrollStopTimeRef.current = performance.now();
          const scrollDuration =
            scrollStopTimeRef.current - scrollStartTimeRef.current;
          logger.info(`[Perf] Scroll stopped - duration: ${scrollDuration.toFixed(0)}ms`);

          loadMissingForCurrentViewport();
        }, 150);

        scheduleViewportLoad(40);
      },
      [
        loadMissingForCurrentViewport,
        missingCount,
        scheduleViewportLoad,
        visibleEndIndex,
        visibleItems.length,
        visibleStartIndex,
      ],
    );

    // ========== Custom scrollbar ==========

    const {
      marginTop: SCROLLBAR_MARGIN_TOP,
      trackHeight: scrollbarTrackHeight,
      thumbHeight: scrollbarThumbHeight,
      thumbMaxTravel: scrollbarThumbMaxTravel,
      maxScrollTop,
      thumbTop: scrollbarThumbTop,
    } = calculateScrollbarMetrics({
      viewportHeight,
      contentHeight,
      totalHeight,
      scrollTop,
    });

    const handleScrollbarMouseDown = useCallback(
      (e: React.MouseEvent) => {
        e.preventDefault();
        setIsDraggingScrollbar(true);
        scrollbarDragStartY.current = e.clientY;
        scrollbarDragStartTop.current = scrollTop;
      },
      [scrollTop],
    );

    useEffect(() => {
      if (!isDraggingScrollbar) return;

      const handleMouseMove = (e: MouseEvent) => {
        const deltaY = e.clientY - scrollbarDragStartY.current;
        const scrollRatio = deltaY / scrollbarThumbMaxTravel;
        const newScrollTop = Math.max(
          0,
          Math.min(
            maxScrollTop,
            scrollbarDragStartTop.current + scrollRatio * maxScrollTop,
          ),
        );

        if (containerRef.current) {
          containerRef.current.scrollTop = newScrollTop;
        }
      };

      const handleMouseUp = () => {
        setIsDraggingScrollbar(false);
      };

      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);

      return () => {
        document.removeEventListener("mousemove", handleMouseMove);
        document.removeEventListener("mouseup", handleMouseUp);
      };
    }, [isDraggingScrollbar, scrollTop, maxScrollTop, scrollbarThumbMaxTravel]);

    const handleScrollbarTrackClick = useCallback(
      (e: React.MouseEvent) => {
        if (e.target === e.currentTarget) {
          const rect = (e.target as HTMLDivElement).getBoundingClientRect();
          const clickY = e.clientY - rect.top - SCROLLBAR_MARGIN_TOP;
          const clickRatio = clickY / scrollbarTrackHeight;
          const newScrollTop = clickRatio * maxScrollTop - viewportHeight / 2;

          if (containerRef.current) {
            containerRef.current.scrollTop = Math.max(
              0,
              Math.min(maxScrollTop, newScrollTop),
            );
          }
        }
      },
      [maxScrollTop, viewportHeight, scrollbarTrackHeight],
    );

    // ========== Action handlers ==========

    const handleCopy = (content: string, type: "text" | "image" | "file") => {
      clipboard.copy(content, type);
    };

    const handleTogglePin = async (id: number, currentPinned: boolean) => {
      try {
        await clipboard.togglePin(id, currentPinned ? 0 : 1);
        refreshList();
      } catch (error) {
        logger.error("Failed to toggle pin:", error);
      }
    };

    const handleDoubleClick = async (item: any) => {
      const isWindowPinned = (globalThis as any).__clipboardXPinned || false;

      try {
        // Copy content to system clipboard
        await clipboard.copy(item.content, item.type);

        // Move item to top and refresh list
        if (!item.is_pinned) {
          try {
            await clipboard.moveToTop(item.id);
          } catch {}
        }

        // Refresh the list to reflect the new order
        refreshList();

        if (!isWindowPinned) {
          await windowApi.hideAndPaste();
        } else {
          await windowApi.pasteToPrevious();
        }
      } catch (error) {
        logger.error("Failed to handle double click:", error);
      }
    };

    // ========== Keyboard events ==========

    useEffect(() => {
      const handleKeyDown = (e: KeyboardEvent) => {
        const target = e.target as HTMLElement;
        if (
          target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable
        ) {
          return;
        }

        if (e.key === "Escape" && isMultiSelectMode) {
          setIsMultiSelectMode(false);
          setCheckedIds(new Set());
          setSelectionRange(null);
          onMultiSelectChange?.(new Set(), []);
          return;
        }

        if ((e.ctrlKey || e.metaKey) && e.key === "a" && isMultiSelectMode) {
          e.preventDefault();
          // Select-all logic
        }

        if (
          (e.ctrlKey || e.metaKey) &&
          e.key.toLowerCase() === "c" &&
          isMultiSelectMode
        ) {
          const mergedText = getMergedText();
          if (mergedText.length > 0) {
            e.preventDefault();
            clipboard.copy(mergedText, "text").catch((error) => {
              logger.error("Failed to copy merged selected text:", error);
            });
          }
          return;
        }

        if (e.key === "Delete" || e.key === "Backspace") {
          handleDeleteSelected();
        }
      };

      globalThis.addEventListener("keydown", handleKeyDown);
      return () => globalThis.removeEventListener("keydown", handleKeyDown);
    }, [getMergedText, handleDeleteSelected, isMultiSelectMode, onMultiSelectChange]);

    // ========== Exposed methods ==========

    const updateItemContent = useCallback(
      (id: number, newContent: string): boolean => {
        const success = cacheManagerRef.current.updateItemContent(
          id,
          newContent,
        );
        if (success) {
          setCacheVersion((prev) => prev + 1);
        }
        return success;
      },
      [],
    );

    useImperativeHandle(
      ref,
      () => ({
        enterMultiSelectMode,
        exitMultiSelectMode,
        getMergedText,
        getCheckedIds: () => checkedIds,
        getSelectedItems: () =>
          Array.from(checkedIds)
            .map((id) =>
              cacheManagerRef.current.getItem(
                cacheManagerRef.current.getIndexById(id) ?? 0,
              ),
            )
            .filter(Boolean),
        updateItemContent,
        // Overlap detection methods
        getOverlapRecords,
        clearOverlapRecords,
        hasOverlaps: () => hasOverlaps,
      }),
      [
        enterMultiSelectMode,
        exitMultiSelectMode,
        getMergedText,
        checkedIds,
        updateItemContent,
        getOverlapRecords,
        clearOverlapRecords,
        hasOverlaps,
      ],
    );

    // ========== Scrollbar drag tooltip ==========

    const getDragTooltipIndex = useCallback(() => {
      if (isSearchMode) {
        return Math.floor(scrollTop / getTextHeight(lineHeight)) + 1;
      }
      let low = 0,
        high = totalCount - 1;
      while (low <= high) {
        const mid = Math.floor((low + high) / 2);
        const pos = typeCacheRef.current.getPosition(mid, totalCount);
        const height = typeCacheRef.current.getHeight(mid);
        if (pos + height < scrollTop) {
          low = mid + 1;
        } else {
          high = mid - 1;
        }
      }
      return Math.max(0, low) + 1;
    }, [scrollTop, totalCount, isSearchMode, lineHeight]);

    const dragTooltipIndex = getDragTooltipIndex();
    const dragTooltipItem = cacheManagerRef.current.getItem(
      dragTooltipIndex - 1,
    );

    // ========== Render ==========

    return (
      <div
        data-testid="clipboard-list"
        style={{
          display: "flex",
          width: "100%",
          height: "100%",
          position: "relative",
          backgroundColor: isDark ? "#0f172a" : "transparent",
        }}
      >
        {/* Main scroll area - hide native scrollbar */}
        <div
          data-testid="clipboard-scroll-container"
          ref={containerRef}
          onScroll={handleScroll}
          onWheelCapture={handleWheelCapture}
          onClick={() => !isMultiSelectMode && setSelectedId(null)}
          style={{
            flex: 1,
            height: "100%",
            overflow: "auto",
            overflowY: "scroll",
            scrollbarWidth: "none",
            msOverflowStyle: "none",
          }}
        >
          <style>{`
            div::-webkit-scrollbar { display: none; }
          `}</style>

          {/* Placeholder area - expands scroll height */}
          <div
            ref={contentRef}
            style={{
              width: "100%",
              height: totalHeight,
              position: "relative",
            }}
          >
            {/* Visible items */}
            {visibleItems.map(({ item, index, top }) => {
              const itemHeight =
                item.type === "image"
                  ? IMAGE_HEIGHT
                  : getTextHeight(lineHeight);

              return (
                <div
                  key={item.id || `item-${index}`}
                  style={{
                    position: "absolute",
                    top: top + CONTENT_PADDING_TOP,
                    left: CONTENT_PADDING_LEFT,
                    right:
                      CONTENT_PADDING_RIGHT + SCROLLBAR_GAP + SCROLLBAR_WIDTH,
                    height: itemHeight,
                    boxSizing: "border-box",
                  }}
                >
                  <ClipboardCard
                    id={item.id}
                    content={item.content}
                    type={item.type}
                    index={index + 1}
                    isPinned={!!item.is_pinned}
                    isSelected={selectedId === item.id}
                    lineHeight={lineHeight}
                    isMultiSelectMode={isMultiSelectMode}
                    isMultiSelected={
                      checkedIds.has(item.id) || isIndexInRange(index)
                    }
                    batchItemIds={selectedIdsForBatch}
                    isDraggingItem={activeDraggedId === item.id}
                    tabId={item.tab_id ?? tabId}
                    metadata={item.metadata}
                    searchQuery={searchQuery}
                    searchMode={searchMode}
                    isSearchMode={isSearchMode}
                    onBatchActionComplete={handleBatchActionComplete}
                    onClick={(e) => handleCardClick(item.id, index, e)}
                    onDoubleClick={() => handleDoubleClick(item)}
                    onTogglePin={() =>
                      handleTogglePin(item.id, !!item.is_pinned)
                    }
                    onEdit={
                      onEdit
                        ? () =>
                            onEdit({
                              id: item.id,
                              content: item.content,
                              type: item.type,
                            })
                        : undefined
                    }
                    // macOS / Windows: simulate dragging with mouse events
                    onMouseDown={!useMouseReorder ? undefined : (e) => handleMouseDown(item.id, index, e)}
                    onMouseMove={!useMouseReorder ? undefined : (e) => handleCardMouseMove(index, e)}
                    onMouseUp={!useMouseReorder ? undefined : handleMouseUp}
                    onMouseLeaveCard={!useMouseReorder ? undefined : handleCardMouseLeave}
                    // Linux: use HTML5 drag
                    onDragStart={useMouseReorder ? undefined : (e) => {
                      logger.debug('[ClipboardList] Drag started for item:', item.id);
                      // Set only the custom data type so macOS does not treat this as a text copy operation
                      e.dataTransfer.setData(
                        "application/x-cliporax-id",
                        String(item.id),
                      );
                      // Explicitly mark this as a move operation and disable copying
                      e.dataTransfer.effectAllowed = "move";
                      e.dataTransfer.dropEffect = "move";
                      
                      // macOS WKWebView compatibility: do not use a transparent image; let the system show the default drag effect
                      // A transparent image makes macOS think this is an external drag and show a copy icon
                      // if (!transparentDragImage) {
                      //   transparentDragImage = createTransparentDragImage();
                      // }
                      // e.dataTransfer.setDragImage(transparentDragImage, 0, 0);
                      
                      handleItemDragStart(item.id);
                    }}
                    onDragEnd={useMouseReorder ? undefined : () => handleItemDragEnd()}
                    onMultiDragStart={isMacOS ? undefined : handleMultiDragStart}
                    onMultiDragEnd={isMacOS ? undefined : handleMultiDragEnd}
                    onDragOver={useMouseReorder ? undefined : (e) => handleCardDragOver(index, e)}
                    onDragLeave={useMouseReorder ? undefined : handleCardDragLeave}
                    onDrop={useMouseReorder ? undefined : (e) => {
                      logger.debug(
                        `[ClipboardList] onDrop triggered for index ${index}`,
                      );
                      handleCardDrop(index);
                    }}
                  />
                </div>
              );
            })}

            {/* Drag insertion position indicator - displayed in the middle of the gap between two items */}
            {activeDragOverIndex !== null &&
              activeDraggedId !== null &&
              (() => {
                const sourceIndex =
                  cacheManagerRef.current.getIndexById(activeDraggedId);
                if (sourceIndex === undefined) return false;

                // Ensure dropIndicator is not shown above or below the source item
                // dragOverIndex means the indicator is shown between dragOverIndex - 1 and dragOverIndex
                // If sourceIndex === dragOverIndex, the indicator is above the source item
                // If sourceIndex === dragOverIndex - 1, the indicator is below the source item
                const isAboveSource = activeDragOverIndex === sourceIndex;
                const isBelowSource = activeDragOverIndex - 1 === sourceIndex;

                return !isAboveSource && !isBelowSource;
              })() && (
                <DropIndicator
                  previousBottom={
                    activeDragOverIndex > 0
                      ? typeCacheRef.current.getPosition(
                          activeDragOverIndex - 1,
                          totalCount,
                        ) +
                        typeCacheRef.current.getHeight(activeDragOverIndex - 1) +
                        CONTENT_PADDING_TOP
                      : CONTENT_PADDING_TOP
                  }
                  currentTop={
                    typeCacheRef.current.getPosition(
                      activeDragOverIndex,
                      totalCount,
                    ) + CONTENT_PADDING_TOP
                  }
                />
              )}

            {/* Show skeleton rows to fill missing positions during fast scrolling */}
            {isFastScrolling &&
              missingCount > 0 &&
              !isSearchMode &&
              Array.from({
                length: Math.min(visibleEndIndex - visibleStartIndex + 1, 10),
              }).map((_, i) => {
                const skeletonIndex = visibleStartIndex + i;
                const cachedItem =
                  cacheManagerRef.current.getItem(skeletonIndex);
                if (cachedItem) return null;
                const skeletonTop = typeCacheRef.current.getPosition(
                  skeletonIndex,
                  totalCount,
                );
                const skeletonHeight =
                  typeCacheRef.current.getHeight(skeletonIndex);
                return (
                  <div
                    key={`skeleton-fast-${skeletonIndex}`}
                    style={{
                      position: "absolute",
                      top: skeletonTop + CONTENT_PADDING_TOP,
                      left: CONTENT_PADDING_LEFT,
                      right:
                        CONTENT_PADDING_RIGHT + SCROLLBAR_GAP + SCROLLBAR_WIDTH,
                    }}
                  >
                    <SkeletonCard height={skeletonHeight} isDark={isDark} />
                  </div>
                );
              })}

            {/* Initial loading skeleton */}
            {isLoading &&
              totalCount > 0 &&
              visibleItems.length === 0 &&
              !isFastScrolling &&
              Array.from({ length: 5 }).map((_, i) => (
                <div
                  key={`skeleton-${i}`}
                  style={{
                    position: "absolute",
                    top:
                      i * (getTextHeight(lineHeight) + CARD_GAP) +
                      CONTENT_PADDING_TOP,
                    left: CONTENT_PADDING_LEFT,
                    right:
                      CONTENT_PADDING_RIGHT + SCROLLBAR_GAP + SCROLLBAR_WIDTH,
                  }}
                >
                  <SkeletonCard isDark={isDark} lineHeight={lineHeight} />
                </div>
              ))}
          </div>
        </div>

        {/* Custom scrollbar */}
        <Scrollbar
          viewportHeight={viewportHeight}
          contentHeight={contentHeight}
          totalHeight={totalHeight}
          isDark={isDark}
          scrollTop={scrollTop}
          scrollbarTrackHeight={scrollbarTrackHeight}
          scrollbarThumbHeight={scrollbarThumbHeight}
          scrollbarThumbTop={scrollbarThumbTop}
          isDraggingScrollbar={isDraggingScrollbar}
          dragTooltipIndex={dragTooltipIndex}
          totalCount={totalCount}
          dragTooltipItem={dragTooltipItem}
          onScrollbarMouseDown={handleScrollbarMouseDown}
          onScrollbarTrackClick={handleScrollbarTrackClick}
        />

        {/* Status overlay */}
        <StatusOverlay
          isSearching={isSearching}
          isSearchMode={isSearchMode}
          searchResults={searchResults}
          searchMode={searchMode}
          totalCount={totalCount}
          isLoading={isLoading}
          isDark={isDark}
        />

      </div>
    );
  },
);

ClipboardList.displayName = "ClipboardList";

export default ClipboardList;
