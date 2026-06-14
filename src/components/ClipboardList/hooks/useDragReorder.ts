import { useCallback, useRef } from "react";
import { createLogger } from "../../../utils/logger";
import { clipboard } from "../../../lib/tauri-api";
import type { ItemTypeCache, ClipboardCacheManager } from "../cache";

const logger = createLogger("ClipboardList");

export interface UseDragReorderParams {
  draggedId: number | null;
  defaultTabId: number | null;
  isSearchMode: boolean;
  cacheManagerRef: React.MutableRefObject<ClipboardCacheManager>;
  typeCacheRef: React.MutableRefObject<ItemTypeCache>;
  containerRef: React.RefObject<HTMLDivElement | null>;
  setDragOverIndex: (index: number | null) => void;
  setDraggedId: (id: number | null) => void;
  setCacheVersion: (updater: (prev: number) => number) => void;
  setSelectedId: (id: number | null) => void;
}

export interface UseDragReorderReturn {
  isReorderModeRef: React.MutableRefObject<boolean>;
  stopAutoScroll: () => void;
  startAutoScroll: (direction: "up" | "down", speed?: number) => void;
  handleCardDragOver: (index: number, event: React.DragEvent) => void;
  handleCardDragLeave: () => void;
  handleCardDrop: (targetIndex: number) => Promise<void>;
  handleItemDragStart: (itemId: number) => void;
  handleItemDragEnd: () => void;
}

export function useDragReorder({
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
}: UseDragReorderParams): UseDragReorderReturn {
  const isReorderModeRef = useRef(false);
  const autoScrollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );

  // Stop auto-scroll
  const stopAutoScroll = useCallback(() => {
    if (autoScrollIntervalRef.current) {
      clearInterval(autoScrollIntervalRef.current);
      autoScrollIntervalRef.current = null;
    }
  }, []);

  // Start auto-scroll when dragging near an edge
  const startAutoScroll = useCallback(
    (direction: "up" | "down", speed: number = 5) => {
      if (autoScrollIntervalRef.current) {
        clearInterval(autoScrollIntervalRef.current);
      }

      autoScrollIntervalRef.current = setInterval(() => {
        if (containerRef.current) {
          const newScrollTop =
            containerRef.current.scrollTop +
            (direction === "down" ? speed : -speed);
          containerRef.current.scrollTop = Math.max(0, newScrollTop);
        }
      }, 16); // ~60fps
    },
    [containerRef],
  );

  // Drag reordering handler
  const handleCardDragOver = useCallback(
    (index: number, event: React.DragEvent) => {
      // Check whether this is edge auto-scroll
      const container = containerRef.current;
      if (container) {
        const rect = container.getBoundingClientRect();
        const scrollZone = 40; // Edge scroll zone size

        if (event.clientY < rect.top + scrollZone) {
          startAutoScroll(
            "up",
            Math.max(1, (rect.top + scrollZone - event.clientY) / 10),
          );
        } else if (event.clientY > rect.bottom - scrollZone) {
          startAutoScroll(
            "down",
            Math.max(1, (event.clientY - rect.bottom + scrollZone) / 10),
          );
        } else {
          stopAutoScroll();
        }
      }

      // Exclude the dragged item itself and clear the indicator
      if (draggedId !== null) {
        const sourceIndex = cacheManagerRef.current.getIndexById(draggedId);
        if (sourceIndex === index) {
          // Mouse is on the source item; do not show dropIndicator
          setDragOverIndex(null);
          return;
        }
      }

      logger.debug(`[DragReorder] dragOver index: ${index}`);
      setDragOverIndex(index);
    },
    [
      draggedId,
      cacheManagerRef,
      startAutoScroll,
      stopAutoScroll,
      containerRef,
      setDragOverIndex,
    ],
  );

  const handleCardDragLeave = useCallback(() => {
    // Do not clear immediately so the drop event can be handled
  }, []);

  const handleCardDrop = useCallback(
    async (targetIndex: number) => {
      stopAutoScroll();

      logger.debug(
        `[DragReorder] handleCardDrop called - targetIndex: ${targetIndex}, draggedId: ${draggedId}`,
      );

      if (draggedId === null || isSearchMode) {
        logger.debug(
          `[DragReorder] Drop ignored - draggedId: ${draggedId}, isSearchMode: ${isSearchMode}`,
        );
        setDragOverIndex(null);
        return;
      }

      // Get the source index
      const sourceIndex = cacheManagerRef.current.getIndexById(draggedId);
      logger.debug(`[DragReorder] sourceIndex: ${sourceIndex}`);

      // Exclude dropping onto the source item itself
      if (sourceIndex === targetIndex) {
        logger.debug(`[DragReorder] Dropped on source, ignoring`);
        setDragOverIndex(null);
        setDraggedId(null);
        return;
      }

      if (sourceIndex === undefined) {
        logger.warn(`[DragReorder] sourceIndex undefined, dropping`);
        setDragOverIndex(null);
        setDraggedId(null);
        return;
      }

      // Mark as a reorder operation
      isReorderModeRef.current = true;

      // Get source and target items to check pin state
      const sourceItem = cacheManagerRef.current.getItem(sourceIndex);
      const targetItem = cacheManagerRef.current.getItem(targetIndex);

      if (!sourceItem || !targetItem) {
        logger.warn(
          `[DragReorder] sourceItem or targetItem undefined - source: ${sourceItem?.id}, target: ${targetItem?.id}`,
        );
        setDragOverIndex(null);
        setDraggedId(null);
        return;
      }

      // Check whether the items are in the same pin group
      const sourceIsPinned = !!sourceItem.is_pinned;
      const targetIsPinned = !!targetItem.is_pinned;

      if (sourceIsPinned !== targetIsPinned) {
        // Do not allow dragging across pin groups
        logger.warn("[DragReorder] Cannot drag across pin boundaries");
        setDragOverIndex(null);
        setDraggedId(null);
        return;
      }

      // Calculate the actual target index
      // The drop indicator appears above targetIndex to mean insertion before targetIndex
      //
      // moveItem internal behavior:
      // - Moving down (from < to): elements from from+1 through to shift forward, and the from element moves to to
      //   Example moveItem(3, 5): 4->3, 5->4, original 3->5, ending at position 5
      // - Moving up (from > to): elements from to through from-1 shift backward, and the from element moves to to
      //   Example moveItem(5, 3): original 5->3, 3->4, 4->5, ending at position 3
      //
      // User expectation:
      // - Drag above targetIndex = insert before targetIndex
      // - Dragging down (sourceIndex < targetIndex): should land at targetIndex - 1
      // - Dragging up (sourceIndex > targetIndex): should land at targetIndex

      let actualTargetIndex: number;
      if (sourceIndex < targetIndex) {
        // Dragging down: indicator is above targetIndex, expected insertion before targetIndex
        // Because moveItem places the item at to when moving down
        // Use targetIndex - 1 so the item appears before targetIndex
        actualTargetIndex = targetIndex - 1;
      } else {
        // Dragging up: indicator is above targetIndex, expected insertion at targetIndex
        actualTargetIndex = targetIndex;
      }

      // Ensure the target index is valid
      if (actualTargetIndex < 0 || actualTargetIndex === sourceIndex) {
        setDragOverIndex(null);
        setDraggedId(null);
        return;
      }

      logger.info(
        `[DragReorder] Moving item ${draggedId} from ${sourceIndex} to ${actualTargetIndex}`,
      );

      // Optimistically update cache to avoid a page refresh
      cacheManagerRef.current.moveItem(sourceIndex, actualTargetIndex);
      typeCacheRef.current.moveItem(sourceIndex, actualTargetIndex);
      setCacheVersion((prev) => prev + 1);

      // Call the backend API to sync ordering
      if (defaultTabId !== null) {
        try {
          const success = await clipboard.moveItemToPosition(
            defaultTabId,
            draggedId,
            sourceIndex,
            actualTargetIndex,
          );
          if (!success) {
            logger.warn(
              "[DragReorder] Backend rejected move, refreshing cache",
            );
            // Refresh cache if the backend rejects the operation
            cacheManagerRef.current.clear();
            typeCacheRef.current.clear();
            const types = await clipboard.getAllTypes(defaultTabId);
            typeCacheRef.current.setTypes(
              types.map(([id, type]) => ({
                id,
                type: type as "text" | "image" | "file",
              })),
              0,
            );
            setCacheVersion((prev) => prev + 1);
          }
        } catch (error) {
          logger.error("[DragReorder] Failed to sync with backend:", error);
        }
      }

      setDragOverIndex(null);
      setDraggedId(null);
    },
    [
      draggedId,
      defaultTabId,
      isSearchMode,
      stopAutoScroll,
      cacheManagerRef,
      typeCacheRef,
      setDragOverIndex,
      setDraggedId,
      setCacheVersion,
    ],
  );

  // Handle drag start in single-select mode
  const handleItemDragStart = useCallback(
    (itemId: number) => {
      setDraggedId(itemId);
      // Highlight the current item when dragging starts
      setSelectedId(itemId);
    },
    [setDraggedId, setSelectedId],
  );

  // Handle drag end
  const handleItemDragEnd = useCallback(() => {
    stopAutoScroll();
    // Clear drag state whether or not this was a reorder operation
    // Because DropIndicator should not remain visible after dragging ends
    setDragOverIndex(null);
    setDraggedId(null);
    // Reset reorder mode flag
    isReorderModeRef.current = false;
  }, [stopAutoScroll, setDragOverIndex, setDraggedId]);

  return {
    isReorderModeRef,
    stopAutoScroll,
    startAutoScroll,
    handleCardDragOver,
    handleCardDragLeave,
    handleCardDrop,
    handleItemDragStart,
    handleItemDragEnd,
  };
}
