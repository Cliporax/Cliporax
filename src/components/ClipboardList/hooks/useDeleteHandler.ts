import { useCallback } from "react";
import { createLogger } from "../../../utils/logger";
import { clipboard } from "../../../lib/tauri-api";
import type { ItemTypeCache, ClipboardCacheManager } from "../cache";

const logger = createLogger("ClipboardList");
const MAX_BATCH_DELETE = 1000;

async function deleteIdsInBatches(
  ids: number[],
  permanently = false,
): Promise<number> {
  let deletedCount = 0;

  for (let i = 0; i < ids.length; i += MAX_BATCH_DELETE) {
    const batch = ids.slice(i, i + MAX_BATCH_DELETE);
    if (batch.length > 0) {
      deletedCount += permanently
        ? await clipboard.deleteByIdsPermanently(batch)
        : await clipboard.deleteByIds(batch);
    }
  }

  return deletedCount;
}

async function deleteIndexRangeInBatches(
  tabId: number,
  start: number,
  end: number,
): Promise<number> {
  let deletedCount = 0;

  for (let chunkEnd = end; chunkEnd >= start; chunkEnd -= MAX_BATCH_DELETE) {
    const chunkStart = Math.max(start, chunkEnd - MAX_BATCH_DELETE + 1);
    deletedCount += await clipboard.deleteByIndexRange(
      tabId,
      chunkStart,
      chunkEnd,
    );
  }

  return deletedCount;
}

async function deleteIndexRangePermanentlyInBatches(
  tabId: number,
  start: number,
  end: number,
): Promise<number> {
  let remaining = end - start + 1;
  let deletedCount = 0;

  while (remaining > 0) {
    const items = await clipboard.getByTab(
      tabId,
      Math.min(MAX_BATCH_DELETE, remaining),
      start,
    );
    const ids = items
      .map((item) => item.id)
      .filter((id): id is number => id !== null);
    if (ids.length === 0) break;

    deletedCount += await clipboard.deleteByIdsPermanently(ids);
    remaining -= ids.length;
  }

  return deletedCount;
}

export interface UseDeleteHandlerParams {
  selectedId: number | null;
  isMultiSelectMode: boolean;
  checkedIds: Set<number>;
  selectionRange: { start: number; end: number } | null;
  defaultTabId: number | null;
  isSearchMode: boolean;
  isTrashTab: boolean;
  searchResults: any[];
  cacheManagerRef: React.MutableRefObject<ClipboardCacheManager>;
  typeCacheRef: React.MutableRefObject<ItemTypeCache>;
  setTotalCount: (count: number | ((prev: number) => number)) => void;
  setSelectedId: (id: number | null) => void;
  setCacheVersion: (updater: (prev: number) => number) => void;
  setIsMultiSelectMode: (mode: boolean) => void;
  setCheckedIds: (ids: Set<number>) => void;
  setSelectionRange: (range: { start: number; end: number } | null) => void;
  setSearchResults: (results: any[]) => void;
}

export function useDeleteHandler({
  selectedId,
  isMultiSelectMode,
  checkedIds,
  selectionRange,
  defaultTabId,
  isSearchMode,
  isTrashTab,
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
}: UseDeleteHandlerParams): () => Promise<void> {
  const handleDeleteSelected = useCallback(async () => {
    const permanently = isTrashTab;
    // Range selection mode: use backend range deletion
    if (isMultiSelectMode && selectionRange) {
      const start = Math.min(selectionRange.start, selectionRange.end);
      const end = Math.max(selectionRange.start, selectionRange.end);
      const deleteCount = end - start + 1;

      try {
        // Search mode: use batch delete API with optimistic updates
        if (isSearchMode) {
          // Get the items to delete
          const itemsToDelete = searchResults.slice(start, end + 1);
          const idsToDelete = itemsToDelete.map((item) => item.id);

          // Optimistic update: update frontend state first
          const newSearchResults = [...searchResults];
          newSearchResults.splice(start, deleteCount);
          setSearchResults(newSearchResults);
          setTotalCount((prev) => Math.max(0, prev - deleteCount));

          // Determine the next selected item
          if (start < newSearchResults.length) {
            setSelectedId(newSearchResults[start].id);
          } else if (newSearchResults.length > 0) {
            setSelectedId(newSearchResults[newSearchResults.length - 1].id);
          } else {
            setSelectedId(null);
          }

          // Clear cache to keep data consistent after leaving search mode
          cacheManagerRef.current.clear();
          typeCacheRef.current.clear();

          // Delete asynchronously in the background without blocking the UI
          deleteIdsInBatches(idsToDelete, permanently).catch((error) => {
            logger.error("[Delete] Background delete failed:", error);
          });

          setSelectionRange(null);
          setIsMultiSelectMode(false);
          return;
        }

        // Non-search mode: use backend range deletion
        if (defaultTabId !== null) {
          if (permanently) {
            await deleteIndexRangePermanentlyInBatches(
              defaultTabId,
              start,
              end,
            );
          } else {
            await deleteIndexRangeInBatches(defaultTabId, start, end);
          }

          // Check whether the range is in cache
          const cachedStart = typeCacheRef.current.getType(start);
          const cachedEnd = typeCacheRef.current.getType(end);

          if (cachedStart !== undefined && cachedEnd !== undefined) {
            // Range is in cache; update incrementally
            cacheManagerRef.current.removeAtIndexRange(start, end);
            typeCacheRef.current.removeAtIndexRange(start, end);
            setTotalCount((prev) => Math.max(0, prev - deleteCount));

            // Select the next item, the new item at start
            const nextItem = cacheManagerRef.current.getItem(start);
            if (nextItem) {
              setSelectedId(nextItem.id);
            } else {
              setSelectedId(null);
            }
          } else {
            // Range is not in cache; fall back to reload
            cacheManagerRef.current.clear();
            typeCacheRef.current.clear();
            const [realCount, types] = await Promise.all([
              clipboard.getTotalCount(defaultTabId),
              clipboard.getAllTypes(defaultTabId),
            ]);
            typeCacheRef.current.setTypes(
              types.map(([id, type]) => ({
                id,
                type: type as "text" | "image" | "file",
              })),
              0,
            );
            setTotalCount(realCount);
            setSelectedId(null);
          }
          setCacheVersion((prev) => prev + 1);
        }
      } catch (error) {
        logger.error("[Delete] Range delete failed:", error);
      }

      setSelectionRange(null);
      setIsMultiSelectMode(false);
      return;
    }

    // Multi-select mode, non-range: use batch delete
    if (isMultiSelectMode && checkedIds.size > 0) {
      const idsToDelete = Array.from(checkedIds);

      // Search mode: use batch delete API with optimistic updates
      if (isSearchMode) {
        // Find indexes of all items to delete in searchResults
        const indices = idsToDelete
          .map((id) => searchResults.findIndex((item) => item.id === id))
          .filter((idx) => idx !== -1);

        // Find the minimum index so the new item at that position can be selected after deletion
        const minIndex = indices.length > 0 ? Math.min(...indices) : undefined;

        // Optimistic update: update frontend state first
        const newSearchResults = searchResults.filter(
          (item) => !checkedIds.has(item.id),
        );
        setSearchResults(newSearchResults);
        setTotalCount((prev) => Math.max(0, prev - checkedIds.size));

        // Determine the next selected item
        if (minIndex !== undefined && minIndex < newSearchResults.length) {
          setSelectedId(newSearchResults[minIndex].id);
        } else if (newSearchResults.length > 0) {
          setSelectedId(newSearchResults[newSearchResults.length - 1].id);
        } else {
          setSelectedId(null);
        }

        // Clear cache to keep data consistent after leaving search mode
        cacheManagerRef.current.clear();
        typeCacheRef.current.clear();

        // Delete asynchronously in the background without blocking the UI
        deleteIdsInBatches(idsToDelete, permanently).catch((error) => {
          logger.error("[Delete] Background delete failed:", error);
        });

        setCheckedIds(new Set());
        setIsMultiSelectMode(false);
        return;
      }

      // Non-search mode: use cache manager plus batch delete
      // First get indexes of all items to delete
      const indices = idsToDelete
        .map((id) => cacheManagerRef.current.getIndexById(id))
        .filter((idx): idx is number => idx !== undefined);

      // Find the minimum index so the new item at that position can be selected after deletion
      const minIndex = indices.length > 0 ? Math.min(...indices) : undefined;

      // Optimistic update: update frontend state first
      if (indices.length === checkedIds.size) {
        cacheManagerRef.current.removeByIds(checkedIds);
        typeCacheRef.current.removeAtIndices(indices);
        setTotalCount((prev) => Math.max(0, prev - checkedIds.size));

        // Determine the next selected item
        if (minIndex !== undefined) {
          const nextItem = cacheManagerRef.current.getItem(minIndex);
          setSelectedId(nextItem ? nextItem.id : null);
        } else {
          setSelectedId(null);
        }
      } else {
        // Some items are not in cache; fall back to reload
        cacheManagerRef.current.clear();
        typeCacheRef.current.clear();
        if (defaultTabId !== null) {
          // Reload asynchronously in the background
          Promise.all([
            clipboard.getTotalCount(defaultTabId),
            clipboard.getAllTypes(defaultTabId),
          ]).then(([realCount, types]) => {
            typeCacheRef.current.setTypes(
              types.map(([id, type]) => ({
                id,
                type: type as "text" | "image" | "file",
              })),
              0,
            );
            setTotalCount(realCount);
          });
        }
        setSelectedId(null);
      }

      // Delete asynchronously in the background without blocking the UI
      deleteIdsInBatches(idsToDelete, permanently).catch((error) => {
        logger.error("[Delete] Background delete failed:", error);
      });

      setCheckedIds(new Set());
      setIsMultiSelectMode(false);
      setCacheVersion((prev) => prev + 1);
      return;
    }

    // Single-item deletion: use incremental deletion to avoid refreshing the page
    if (selectedId) {
      // Search mode: remove directly from searchResults
      if (isSearchMode) {
        const deletedIndex = searchResults.findIndex(
          (item) => item.id === selectedId,
        );

        if (deletedIndex !== -1) {
          if (permanently) {
            await clipboard.deleteByIdsPermanently([selectedId]);
          } else {
            await clipboard.delete(selectedId);
          }

          // Remove from searchResults
          const newSearchResults = [...searchResults];
          newSearchResults.splice(deletedIndex, 1);
          setSearchResults(newSearchResults);

          // Update total count
          setTotalCount((prev) => Math.max(0, prev - 1));

          // Clear cache to keep data consistent after leaving search mode
          cacheManagerRef.current.clear();
          typeCacheRef.current.clear();

          // Automatically select the next item
          if (deletedIndex < newSearchResults.length) {
            setSelectedId(newSearchResults[deletedIndex].id);
          } else if (newSearchResults.length > 0) {
            setSelectedId(newSearchResults[newSearchResults.length - 1].id);
          } else {
            setSelectedId(null);
          }
        }
        return;
      }

      // Non-search mode: use cache manager
      // First get the deleted item index
      const deletedIndex = cacheManagerRef.current.getIndexById(selectedId);

      if (permanently) {
        await clipboard.deleteByIdsPermanently([selectedId]);
      } else {
        await clipboard.delete(selectedId);
      }

      // If the item is in cache, update incrementally
      if (deletedIndex !== undefined) {
        cacheManagerRef.current.removeAtIndex(deletedIndex);
        typeCacheRef.current.removeAtIndex(deletedIndex);

        // Update total count
        setTotalCount((prev) => Math.max(0, prev - 1));

        // Automatically select the next item, the new item at the same index
        const nextItem = cacheManagerRef.current.getItem(deletedIndex);
        if (nextItem) {
          setSelectedId(nextItem.id);
        } else {
          setSelectedId(null);
        }
      } else {
        // Item is not in cache; fall back to reload
        cacheManagerRef.current.clear();
        typeCacheRef.current.clear();
        if (defaultTabId !== null) {
          const [realCount, types] = await Promise.all([
            clipboard.getTotalCount(defaultTabId),
            clipboard.getAllTypes(defaultTabId),
          ]);
          typeCacheRef.current.setTypes(
            types.map(([id, type]) => ({
              id,
              type: type as "text" | "image" | "file",
            })),
            0,
          );
          setTotalCount(realCount);
        }
        setSelectedId(null);
      }

      setCacheVersion((prev) => prev + 1);
    }
  }, [
    selectedId,
    isMultiSelectMode,
    checkedIds,
    selectionRange,
    defaultTabId,
    isSearchMode,
    isTrashTab,
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
  ]);

  return handleDeleteSelected;
}
