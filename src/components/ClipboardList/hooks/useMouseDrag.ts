import { useCallback, useRef, useState, useEffect } from "react";
import { createLogger } from "../../../utils/logger";
import { clipboard } from "../../../lib/tauri-api";
import type { ItemTypeCache, ClipboardCacheManager } from "../cache";

const logger = createLogger("useMouseDrag");

export interface UseMouseDragParams {
  defaultTabId: number | null;
  isSearchMode: boolean;
  cacheManagerRef: React.MutableRefObject<ClipboardCacheManager>;
  typeCacheRef: React.MutableRefObject<ItemTypeCache>;
  containerRef: React.RefObject<HTMLDivElement | null>;
  setCacheVersion: (updater: (prev: number) => number) => void;
  setSelectedId: (id: number | null) => void;
}

export interface UseMouseDragReturn {
  draggedId: number | null;
  dragOverIndex: number | null;
  handleMouseDown: (itemId: number, index: number, e: React.MouseEvent) => void;
  handleMouseMove: (index: number, e: React.MouseEvent) => void;
  handleMouseUp: () => void;
  handleMouseLeave: () => void;
}

export function useMouseDrag({
  defaultTabId,
  isSearchMode,
  cacheManagerRef,
  typeCacheRef,
  containerRef,
  setCacheVersion,
  setSelectedId,
}: UseMouseDragParams): UseMouseDragReturn {
  const [draggedId, setDraggedId] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  
  const isDraggingRef = useRef(false);
  const draggedItemIdRef = useRef<number | null>(null);
  const draggedItemIndexRef = useRef<number | null>(null);
  const dragStartYRef = useRef(0);
  const hasMovedRef = useRef(false);
  const dragOverIndexRef = useRef<number | null>(null); // Use a ref to store the latest dragOverIndex
  const dragThreshold = 5; // Treat movement over 5px as dragging

  // Global mouse move
  const handleGlobalMouseMove = useCallback((e: MouseEvent) => {
    if (draggedItemIdRef.current === null || draggedItemIndexRef.current === null) return;
    
    const deltaY = Math.abs(e.clientY - dragStartYRef.current);
    
    // Check whether the drag threshold has been exceeded
    if (!hasMovedRef.current && deltaY > dragThreshold) {
      hasMovedRef.current = true;
      isDraggingRef.current = true;
      setDraggedId(draggedItemIdRef.current);
      logger.debug(`[MouseDrag] Started dragging item ${draggedItemIdRef.current}`);
    }
    
    if (!isDraggingRef.current) return;
    
    // Find the item under the mouse
    const element = document.elementFromPoint(e.clientX, e.clientY);
    const cardElement = element?.closest("[data-item-id]");
    
    if (cardElement) {
      const targetId = parseInt(cardElement.getAttribute("data-item-id") || "0");
      if (Number.isNaN(targetId)) return;
      const targetIndex = cacheManagerRef.current.getIndexById(targetId);
      
      if (targetIndex !== undefined && targetId !== draggedItemIdRef.current) {
        setDragOverIndex(targetIndex);
        dragOverIndexRef.current = targetIndex; // Update the ref synchronously
      }
    }
  }, [cacheManagerRef]);

  // Global mouse release - end dragging
  const handleGlobalMouseUp = useCallback(async () => {
    logger.debug('[MouseDrag] handleGlobalMouseUp called', {
      isDragging: isDraggingRef.current,
      draggedItemId: draggedItemIdRef.current,
      dragOverIndexState: dragOverIndex,
      dragOverIndexRef: dragOverIndexRef.current,
    });
    
    document.removeEventListener("mousemove", handleGlobalMouseMove);
    document.removeEventListener("mouseup", handleGlobalMouseUp);
    
    // Use the value from the ref to avoid closure issues
    const currentDragOverIndex = dragOverIndexRef.current;
    
    if (!isDraggingRef.current || draggedItemIdRef.current === null || currentDragOverIndex === null) {
      logger.debug('[MouseDrag] Early return - conditions not met');
      // No movement or no target; reset state
      isDraggingRef.current = false;
      setDraggedId(null);
      setDragOverIndex(null);
      draggedItemIdRef.current = null;
      draggedItemIndexRef.current = null;
      dragOverIndexRef.current = null;
      return;
    }
    
    logger.debug(`[MouseDrag] Dropping item ${draggedItemIdRef.current} at index ${currentDragOverIndex}`);
    
    const sourceIndex = draggedItemIndexRef.current;
    const targetIndex = currentDragOverIndex;
    
    logger.debug('[MouseDrag] Drop attempt', { sourceIndex, targetIndex });
    
    if (sourceIndex === null || targetIndex === null || sourceIndex === targetIndex) {
      logger.debug('[MouseDrag] Early return - invalid indices');
      // No movement or dropped onto itself; ignore
      isDraggingRef.current = false;
      setDraggedId(null);
      setDragOverIndex(null);
      draggedItemIdRef.current = null;
      draggedItemIndexRef.current = null;
      dragOverIndexRef.current = null;
      return;
    }
    
    // Get source and target items to check pin state
    const sourceItem = cacheManagerRef.current.getItem(sourceIndex);
    const targetItem = cacheManagerRef.current.getItem(targetIndex);
    
    if (!sourceItem || !targetItem) {
      logger.warn(`[MouseDrag] sourceItem or targetItem undefined`);
      isDraggingRef.current = false;
      setDraggedId(null);
      setDragOverIndex(null);
      draggedItemIdRef.current = null;
      draggedItemIndexRef.current = null;
      return;
    }
    
    // Check whether the items are in the same pin group
    const sourceIsPinned = !!sourceItem.is_pinned;
    const targetIsPinned = !!targetItem.is_pinned;
    
    if (sourceIsPinned !== targetIsPinned) {
      logger.warn("[MouseDrag] Cannot drag across pin boundaries");
      isDraggingRef.current = false;
      setDraggedId(null);
      setDragOverIndex(null);
      draggedItemIdRef.current = null;
      draggedItemIndexRef.current = null;
      return;
    }
    
    // Calculate the actual target index
    let actualTargetIndex: number;
    if (sourceIndex < targetIndex) {
      actualTargetIndex = targetIndex - 1;
    } else {
      actualTargetIndex = targetIndex;
    }
    
    logger.debug(`[MouseDrag] Moving from ${sourceIndex} to ${actualTargetIndex}`, {
      defaultTabId,
      itemId: draggedItemIdRef.current,
    });
    
    try {
      if (defaultTabId === null) {
        logger.error("[MouseDrag] defaultTabId is null");
        return;
      }
      
      // Call the backend API to move the item
      await clipboard.moveItemToPosition(
        defaultTabId,
        draggedItemIdRef.current,
        sourceIndex,
        actualTargetIndex,
      );
      
      // Update the frontend cache
      cacheManagerRef.current.moveItem(sourceIndex, actualTargetIndex);
      typeCacheRef.current.moveItem(sourceIndex, actualTargetIndex);
      setCacheVersion((prev) => prev + 1);
      
      // Select the item at its new position
      setSelectedId(draggedItemIdRef.current);
      
      logger.info(`[MouseDrag] Item moved successfully from ${sourceIndex} to ${actualTargetIndex}`);
    } catch (error) {
      logger.error("[MouseDrag] Failed to move item:", error);
    }
    
    // Reset state
    isDraggingRef.current = false;
    setDraggedId(null);
    setDragOverIndex(null);
    draggedItemIdRef.current = null;
    draggedItemIndexRef.current = null;
    dragOverIndexRef.current = null;
  }, [defaultTabId, cacheManagerRef, typeCacheRef, setCacheVersion, setSelectedId]);

  // Mouse down - start dragging
  const handleMouseDown = useCallback((itemId: number, index: number, e: React.MouseEvent) => {
    // Handle only the left button
    if (e.button !== 0) return;
    
    logger.debug(`[MouseDrag] Mouse down on item ${itemId} at index ${index}`);
    
    draggedItemIdRef.current = itemId;
    draggedItemIndexRef.current = index;
    dragStartYRef.current = e.clientY;
    hasMovedRef.current = false;
    
    // Listen for global mouse move and release
    document.addEventListener("mousemove", handleGlobalMouseMove);
    document.addEventListener("mouseup", handleGlobalMouseUp);
  }, [handleGlobalMouseMove, handleGlobalMouseUp]);

  // Clean up event listeners when the component unmounts
  useEffect(() => {
    return () => {
      document.removeEventListener("mousemove", handleGlobalMouseMove);
      document.removeEventListener("mouseup", handleGlobalMouseUp);
    };
  }, [handleGlobalMouseMove, handleGlobalMouseUp]);

  // Mouse enters target item
  const handleMouseMove = useCallback((index: number, e: React.MouseEvent) => {
    if (!isDraggingRef.current || draggedItemIdRef.current === null) return;
    
    const targetId = parseInt((e.currentTarget as HTMLElement).getAttribute("data-item-id") || "0");
    if (Number.isNaN(targetId)) return;
    
    if (targetId !== draggedItemIdRef.current) {
      setDragOverIndex(index);
      dragOverIndexRef.current = index; // Update the ref synchronously
    }
  }, []);

  // Mouse leave
  const handleMouseLeave = useCallback(() => {
    // Do not clear immediately so the drop event can be handled
  }, []);

  return {
    draggedId,
    dragOverIndex,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp: handleGlobalMouseUp,
    handleMouseLeave,
  };
}
