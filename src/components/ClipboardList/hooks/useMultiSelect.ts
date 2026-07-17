import { useCallback, useRef } from "react";
import { createLogger } from "../../../utils/logger";
import { clipboard, window as windowApi } from "../../../lib/tauri-api";
import { createTransparentDragImage } from "../constants";
import type { ClipboardCacheManager } from "../cache";

const logger = createLogger("ClipboardList");

export interface UseMultiSelectParams {
  selectedId: number | null;
  isMultiSelectMode: boolean;
  checkedIds: Set<number>;
  selectionRange: { start: number; end: number } | null;
  cacheManagerRef: React.MutableRefObject<ClipboardCacheManager>;
  setIsMultiSelectMode: (mode: boolean) => void;
  setCheckedIds: (ids: Set<number>) => void;
  setSelectionRange: (range: { start: number; end: number } | null) => void;
  setSelectedId: (id: number | null) => void;
  onMultiSelectChange?: (
    selectedIds: Set<number>,
    selectedItems: any[],
  ) => void;
}

export interface UseMultiSelectReturn {
  lastCheckedIndexRef: React.MutableRefObject<number | null>;
  isMultiDraggingRef: React.MutableRefObject<boolean>;
  isPastingRef: React.MutableRefObject<boolean>;
  isIndexInRange: (index: number) => boolean;
  handleCardClick: (id: number, index: number, event: React.MouseEvent) => void;
  enterMultiSelectMode: () => void;
  exitMultiSelectMode: () => void;
  getMergedText: () => string;
  handleMultiDragStart: (e: React.DragEvent) => void;
  handleMultiDragEnd: (e: React.DragEvent) => Promise<void>;
}

// Global transparent drag image element, reused
let transparentDragImage: HTMLElement | null = null;

export function useMultiSelect({
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
}: UseMultiSelectParams): UseMultiSelectReturn {
  const lastCheckedIndexRef = useRef<number | null>(null);
  const isMultiDraggingRef = useRef(false);
  const isPastingRef = useRef(false);

  // Check whether the index is within the selected range
  const isIndexInRange = useCallback(
    (index: number): boolean => {
      if (selectionRange) {
        const start = Math.min(selectionRange.start, selectionRange.end);
        const end = Math.max(selectionRange.start, selectionRange.end);
        return index >= start && index <= end;
      }
      return false;
    },
    [selectionRange],
  );

  // Multi-select handling
  const handleCardClick = useCallback(
    (id: number, index: number, event: React.MouseEvent) => {
      // Ctrl+click: toggle a single selection
      if (event.ctrlKey || event.metaKey) {
        if (!isMultiSelectMode) {
          setIsMultiSelectMode(true);
          const newCheckedIds = new Set<number>();
          if (selectedId !== null) {
            // Find the index for selectedId as the start point
            const prevIndex = cacheManagerRef.current.getIndexById(selectedId);
            if (prevIndex !== undefined) {
              newCheckedIds.add(selectedId);
            }
          }
          newCheckedIds.add(id);
          setCheckedIds(newCheckedIds);
          // Clear range selection
          setSelectionRange(null);
        } else {
          const newCheckedIds = new Set(checkedIds);
          if (newCheckedIds.has(id)) {
            newCheckedIds.delete(id);
          } else {
            newCheckedIds.add(id);
          }
          setCheckedIds(newCheckedIds);
        }
        lastCheckedIndexRef.current = index;
        return;
      }

      // Shift+click: range selection
      if (event.shiftKey) {
        let startIndex: number;

        if (selectionRange) {
          // Existing range selection; use the current range start as the start point
          startIndex = selectionRange.start;
        } else if (lastCheckedIndexRef.current !== null) {
          // Previous operation index exists; use it as the start point
          startIndex = lastCheckedIndexRef.current;
        } else if (selectedId !== null) {
          // Single-selected item exists; use it as the start point
          const prevIndex = cacheManagerRef.current.getIndexById(selectedId);
          startIndex = prevIndex !== undefined ? prevIndex : index;
        } else {
          // No previous selection; use the current item as the start point
          startIndex = index;
        }

        // Set range selection
        const newRange = { start: startIndex, end: index };
        setSelectionRange(newRange);
        setIsMultiSelectMode(true);
        setCheckedIds(new Set()); // Clear individual selections and use range selection
        lastCheckedIndexRef.current = index;

        logger.info(`[MultiSelect] Range selection: ${startIndex} -> ${index}`);
        return;
      }

      // Normal click: single-select mode
      if (isMultiSelectMode) {
        setIsMultiSelectMode(false);
        setCheckedIds(new Set());
        setSelectionRange(null);
        onMultiSelectChange?.(new Set(), []);
      }

      setSelectedId(id);
      lastCheckedIndexRef.current = index;
    },
    [
      isMultiSelectMode,
      checkedIds,
      selectedId,
      onMultiSelectChange,
      selectionRange,
      cacheManagerRef,
      setIsMultiSelectMode,
      setCheckedIds,
      setSelectionRange,
      setSelectedId,
    ],
  );

  const enterMultiSelectMode = useCallback(() => {
    setIsMultiSelectMode(true);
    setCheckedIds(new Set());
    setSelectionRange(null);
  }, [setIsMultiSelectMode, setCheckedIds, setSelectionRange]);

  const exitMultiSelectMode = useCallback(() => {
    setIsMultiSelectMode(false);
    setCheckedIds(new Set());
    setSelectionRange(null);
    lastCheckedIndexRef.current = null;
    isMultiDraggingRef.current = false;
    onMultiSelectChange?.(new Set(), []);
  }, [
    onMultiSelectChange,
    setIsMultiSelectMode,
    setCheckedIds,
    setSelectionRange,
  ]);

  const getSelectedItems = useCallback(() => {
    if (selectionRange) {
      const start = Math.min(selectionRange.start, selectionRange.end);
      const end = Math.max(selectionRange.start, selectionRange.end);
      const items: any[] = [];

      for (let i = start; i <= end; i++) {
        const item = cacheManagerRef.current.getItem(i);
        if (item) items.push(item);
      }

      return items;
    }

    return Array.from(checkedIds)
      .map((id) => {
        const index = cacheManagerRef.current.getIndexById(id);
        return index === undefined ? null : cacheManagerRef.current.getItem(index);
      })
      .filter(Boolean);
  }, [checkedIds, selectionRange, cacheManagerRef]);

  const getMergedText = useCallback(() => {
    return getSelectedItems()
      .filter((item) => item && item.type === "text")
      .map((item) => item.content)
      .join("\n");
  }, [getSelectedItems]);

  const handleMultiDragStart = useCallback(
    (e: React.DragEvent) => {
      const textItems = getSelectedItems().filter((item) => item?.type === "text");

      if (textItems.length === 0) {
        e.preventDefault();
        return;
      }

      isMultiDraggingRef.current = true;
      const mergedText = textItems.map((item) => item.content).join("\n");
      e.dataTransfer.setData("text/plain", mergedText);
      e.dataTransfer.effectAllowed = "copy";
      // Set a transparent drag image to avoid the default rectangular background
      if (!transparentDragImage) {
        transparentDragImage = createTransparentDragImage();
      }
      e.dataTransfer.setDragImage(transparentDragImage, 0, 0);
    },
    [getSelectedItems],
  );

  const handleMultiDragEnd = useCallback(
    async (e: React.DragEvent) => {
      // Skip paste logic for reorder operations
      if (isMultiDraggingRef.current === false) {
        return;
      }

      if (isPastingRef.current) return;

      if (e.dataTransfer.dropEffect === "none") {
        exitMultiSelectMode();
        return;
      }

      isMultiDraggingRef.current = false;
      isPastingRef.current = true;

      const textItems = getSelectedItems().filter((item) => item?.type === "text");

      try {
        for (let i = textItems.length - 1; i >= 0; i--) {
          await clipboard.copy(textItems[i].content, "text");
          await new Promise((r) => setTimeout(r, 50));
          await windowApi.pasteToPrevious();
          if (i > 0) await new Promise((r) => setTimeout(r, 150));
        }
      } finally {
        isPastingRef.current = false;
        exitMultiSelectMode();
      }
    },
    [exitMultiSelectMode, getSelectedItems],
  );

  return {
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
  };
}
