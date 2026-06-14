import { IMAGE_HEIGHT, CARD_GAP, getTextHeight } from "./constants";

// Type cache manager for quickly retrieving each index type and speeding up height calculation
export class ItemTypeCache {
  private typeMap: Map<number, "text" | "image" | "file"> = new Map();
  private totalHeightCache: number = 0;
  private totalHeightCacheCount: number = -1;
  private positionCache: Map<number, number> = new Map(); // index -> top position
  private dirty: boolean = true;
  private lineHeight: "small" | "medium" | "large" = "medium";
  private maxSize: number;

  constructor(maxSize: number = 3000) {
    this.maxSize = maxSize;
  }

  // Set the current lineHeight, called when the size changes
  setLineHeight(lh: "small" | "medium" | "large"): void {
    if (this.lineHeight !== lh) {
      this.lineHeight = lh;
      this.dirty = true;
    }
  }

  // Set types in batches during startup preload
  setTypes(
    items: Array<{ id: number; type: "text" | "image" | "file" }>,
    startIndex: number,
  ): void {
    items.forEach((item, i) => {
      const index = startIndex + i;
      this.typeMap.set(index, item.type);
    });
    this.dirty = true;
    this.pruneCache(startIndex);
  }

  // Get a single type
  getType(index: number): "text" | "image" | "file" | undefined {
    return this.typeMap.get(index);
  }

  // Set a single type when a new item arrives
  setType(index: number, type: "text" | "image" | "file"): void {
    this.typeMap.set(index, type);
    this.dirty = true;
    this.pruneCache(index);
  }

  // Insert a new item at the top, index 0, and shift all existing indexes by +1
  insertAtTop(type: "text" | "image" | "file"): void {
    const newTypeMap = new Map<number, "text" | "image" | "file">();

    this.typeMap.forEach((t, idx) => {
      newTypeMap.set(idx + 1, t);
    });

    newTypeMap.set(0, type);
    this.typeMap = newTypeMap;
    this.dirty = true;
    this.pruneCache(0);
  }

  // Get height by type
  getHeightByType(type: "text" | "image" | "file"): number {
    return type === "image" ? IMAGE_HEIGHT : getTextHeight(this.lineHeight);
  }

  // Get the height for an index
  getHeight(index: number): number {
    const type = this.typeMap.get(index);
    return type ? this.getHeightByType(type) : getTextHeight(this.lineHeight);
  }

  private getDefaultTextHeight(): number {
    return getTextHeight(this.lineHeight);
  }

  private getKnownHeightDeltaBefore(index: number): number {
    const defaultHeight = this.getDefaultTextHeight();
    let delta = 0;

    this.typeMap.forEach((type, typeIndex) => {
      if (typeIndex < index) {
        delta += this.getHeightByType(type) - defaultHeight;
      }
    });

    return delta;
  }

  // Get the cached top position for an index
  getPosition(index: number, totalCount: number): number {
    if (index <= 0 || totalCount <= 0) return 0;

    const boundedIndex = Math.min(index, totalCount);
    return (
      boundedIndex * (this.getDefaultTextHeight() + CARD_GAP) +
      this.getKnownHeightDeltaBefore(boundedIndex)
    );
  }

  // Rebuild the position cache
  private rebuildPositionCache(totalCount: number): void {
    this.positionCache.clear();
    let currentTop = 0;

    for (let i = 0; i < totalCount; i++) {
      this.positionCache.set(i, currentTop);
      currentTop += this.getHeight(i) + CARD_GAP;
    }

    this.totalHeightCache = currentTop - CARD_GAP; // The last item does not need a gap
    this.dirty = false;
  }

  // Get total height
  getTotalHeight(totalCount: number): number {
    if (totalCount <= 0) return 0;

    if (this.dirty || this.totalHeightCacheCount !== totalCount) {
      const defaultHeight = this.getDefaultTextHeight();
      let heightDelta = 0;

      this.typeMap.forEach((type, index) => {
        if (index < totalCount) {
          heightDelta += this.getHeightByType(type) - defaultHeight;
        }
      });

      this.totalHeightCache =
        totalCount * (defaultHeight + CARD_GAP) - CARD_GAP + heightDelta;
      this.totalHeightCacheCount = totalCount;
      this.dirty = false;
    }

    return this.totalHeightCache;
  }

  // Find visible range from scroll position
  findVisibleRange(
    scrollTop: number,
    viewportHeight: number,
    totalCount: number,
  ): { start: number; end: number } {
    if (totalCount === 0) return { start: 0, end: 0 };

    // Binary search for the start position
    let low = 0,
      high = totalCount - 1;
    let startIndex = 0;

    while (low <= high) {
      const mid = Math.floor((low + high) / 2);
      const pos = this.getPosition(mid, totalCount);
      const height = this.getHeight(mid);

      if (pos + height < scrollTop) {
        low = mid + 1;
      } else {
        startIndex = mid;
        high = mid - 1;
      }
    }

    // Search downward from the start position to find the end position
    let endIndex = startIndex;
    let currentPos = this.getPosition(startIndex, totalCount);

    while (
      endIndex < totalCount - 1 &&
      currentPos < scrollTop + viewportHeight
    ) {
      endIndex++;
      currentPos = this.getPosition(endIndex, totalCount);
    }

    return { start: startIndex, end: Math.min(endIndex, totalCount - 1) };
  }

  // Clear cache
  clear(): void {
    this.typeMap.clear();
    this.positionCache.clear();
    this.totalHeightCache = 0;
    this.totalHeightCacheCount = -1;
    this.dirty = true;
  }

  private pruneCache(keepAroundIndex: number): void {
    if (this.typeMap.size <= this.maxSize) return;

    const keysByDistance = Array.from(this.typeMap.keys()).sort(
      (a, b) => Math.abs(b - keepAroundIndex) - Math.abs(a - keepAroundIndex),
    );
    const deleteCount = this.typeMap.size - this.maxSize;

    for (let i = 0; i < deleteCount; i++) {
      this.typeMap.delete(keysByDistance[i]);
    }

    this.positionCache.clear();
    this.dirty = true;
  }

  // Incremental deletion: remove the specified index and shift all following indexes by -1
  removeAtIndex(index: number): void {
    this.typeMap.delete(index);

    // Shift all following indexes by -1
    const newTypeMap = new Map<number, "text" | "image" | "file">();

    this.typeMap.forEach((t, idx) => {
      const newIdx = idx > index ? idx - 1 : idx;
      newTypeMap.set(newIdx, t);
    });

    this.typeMap = newTypeMap;
    this.dirty = true;
  }

  // Batch-delete an index range
  removeAtIndexRange(start: number, end: number): void {
    const count = end - start + 1;

    // Delete all items in the range
    for (let i = start; i <= end; i++) {
      this.typeMap.delete(i);
    }

    // Shift all following indexes forward
    const newTypeMap = new Map<number, "text" | "image" | "file">();

    this.typeMap.forEach((t, idx) => {
      const newIdx = idx > end ? idx - count : idx;
      newTypeMap.set(newIdx, t);
    });

    this.typeMap = newTypeMap;
    this.dirty = true;
  }

  // Batch-delete multiple indexes from back to front
  removeAtIndices(indices: number[]): void {
    // Sort from back to front
    const sorted = [...indices].sort((a, b) => b - a);
    for (const idx of sorted) {
      this.removeAtIndex(idx);
    }
  }

  // Move item to a new position for drag reordering
  moveItem(fromIndex: number, toIndex: number): void {
    const type = this.typeMap.get(fromIndex);
    if (type === undefined) return;

    // Ensure toIndex is within the valid range
    const maxIndex = Math.max(...Array.from(this.typeMap.keys()));
    toIndex = Math.max(0, Math.min(toIndex, maxIndex));

    if (fromIndex === toIndex) return;

    // Create a new typeMap
    const newTypeMap = new Map<number, "text" | "image" | "file">();

    if (fromIndex < toIndex) {
      // Move down: shift elements from fromIndex+1 to toIndex forward by one
      for (let i = 0; i < fromIndex; i++) {
        const t = this.typeMap.get(i);
        if (t !== undefined) newTypeMap.set(i, t);
      }
      for (let i = fromIndex + 1; i <= toIndex; i++) {
        const t = this.typeMap.get(i);
        if (t !== undefined) newTypeMap.set(i - 1, t);
      }
      newTypeMap.set(toIndex, type);
      // Elements after toIndex remain unchanged
      for (let i = toIndex + 1; i <= maxIndex; i++) {
        const t = this.typeMap.get(i);
        if (t !== undefined) newTypeMap.set(i, t);
      }
    } else {
      // Move up: shift elements from toIndex to fromIndex-1 backward by one
      for (let i = 0; i < toIndex; i++) {
        const t = this.typeMap.get(i);
        if (t !== undefined) newTypeMap.set(i, t);
      }
      newTypeMap.set(toIndex, type);
      for (let i = toIndex; i < fromIndex; i++) {
        const t = this.typeMap.get(i);
        if (t !== undefined) newTypeMap.set(i + 1, t);
      }
      // Elements after fromIndex remain unchanged
      for (let i = fromIndex + 1; i <= maxIndex; i++) {
        const t = this.typeMap.get(i);
        if (t !== undefined) newTypeMap.set(i, t);
      }
    }

    this.typeMap = newTypeMap;
    this.dirty = true;
  }

  // Get the number of cached types
  size(): number {
    return this.typeMap.size;
  }
}

// Cache manager class
export class ClipboardCacheManager {
  private cache: Map<number, any> = new Map();
  private idToIndex: Map<number, number> = new Map();
  private loadedRanges: Array<{ start: number; end: number }> = [];
  private pendingRanges: Set<string> = new Set(); // Ranges currently loading, used as range locks
  private maxSize: number;

  constructor(maxSize: number = 3000) {
    this.maxSize = maxSize;
  }

  // Range lock: mark loading started
  startLoading(start: number, end: number): boolean {
    const key = `${start}-${end}`;
    if (this.pendingRanges.has(key)) return false;
    this.pendingRanges.add(key);
    return true;
  }

  // Range lock: mark loading completed
  finishLoading(start: number, end: number): void {
    const key = `${start}-${end}`;
    this.pendingRanges.delete(key);
  }

  // Check whether a range is loading by checking overlap
  isRangeLoading(start: number, end: number): boolean {
    for (const key of this.pendingRanges) {
      const [pendingStart, pendingEnd] = key.split("-").map(Number);
      // Check for overlap
      if (start <= pendingEnd && end >= pendingStart) {
        return true;
      }
    }
    return false;
  }

  addItems(items: any[], startIndex: number): void {
    items.forEach((item, i) => {
      const index = startIndex + i;
      this.cache.set(index, item);
      if (item.id !== undefined) {
        this.idToIndex.set(item.id, index);
      }
    });

    const endIndex = startIndex + items.length - 1;
    this.loadedRanges.push({ start: startIndex, end: endIndex });
    this.mergeRanges();

    if (this.cache.size > this.maxSize) {
      this.pruneCache(startIndex);
    }
  }

  // Insert a new item at the top, index 0, and shift all existing indexes by +1
  insertAtTop(item: any): void {
    // Shift all existing indexes by +1
    const newCache = new Map<number, any>();
    const newIdToIndex = new Map<number, number>();

    this.cache.forEach((cachedItem, idx) => {
      newCache.set(idx + 1, cachedItem);
      if (cachedItem.id !== undefined) {
        newIdToIndex.set(cachedItem.id, idx + 1);
      }
    });

    // Place the new item at index 0
    newCache.set(0, item);
    if (item.id !== undefined) {
      newIdToIndex.set(item.id, 0);
    }

    this.cache = newCache;
    this.idToIndex = newIdToIndex;

    // Update loadedRanges: shift all ranges by +1, then add the new range [0,0]
    this.loadedRanges = this.loadedRanges.map((range) => ({
      start: range.start + 1,
      end: range.end + 1,
    }));
    this.loadedRanges.unshift({ start: 0, end: 0 });
    this.mergeRanges();

    if (this.cache.size > this.maxSize) {
      this.pruneCache(0);
    }
  }

  private mergeRanges(): void {
    this.loadedRanges.sort((a, b) => a.start - b.start);
    const merged: Array<{ start: number; end: number }> = [];
    for (const range of this.loadedRanges) {
      if (merged.length === 0) {
        merged.push(range);
      } else {
        const last = merged[merged.length - 1];
        if (range.start <= last.end + 1) {
          last.end = Math.max(last.end, range.end);
        } else {
          merged.push(range);
        }
      }
    }
    this.loadedRanges = merged;
  }

  getItem(index: number): any | undefined {
    return this.cache.get(index);
  }

  getIndexById(id: number): number | undefined {
    return this.idToIndex.get(id);
  }

  isRangeLoaded(start: number, end: number): boolean {
    for (const range of this.loadedRanges) {
      if (range.start <= start && range.end >= end) {
        return true;
      }
    }
    return false;
  }

  getMissingRanges(
    start: number,
    end: number,
  ): Array<{ start: number; end: number }> {
    const missing: Array<{ start: number; end: number }> = [];
    let current = start;

    while (current <= end) {
      let found = false;
      for (const range of this.loadedRanges) {
        if (range.start <= current && range.end >= current) {
          current = range.end + 1;
          found = true;
          break;
        } else if (range.start > current) {
          missing.push({ start: current, end: Math.min(range.start - 1, end) });
          current = range.start;
          found = true;
          break;
        }
      }
      if (!found) {
        missing.push({ start: current, end });
        break;
      }
    }

    return missing;
  }

  pruneCache(keepAroundIndex?: number): void {
    if (this.cache.size <= this.maxSize) return;

    const keepIndex = keepAroundIndex ?? 0;
    const keysToDelete: number[] = [];

    this.cache.forEach((_, key) => {
      if (Math.abs(key - keepIndex) > 200) {
        keysToDelete.push(key);
      }
    });

    keysToDelete.forEach((key) => {
      const item = this.cache.get(key);
      if (item?.id !== undefined) {
        this.idToIndex.delete(item.id);
      }
      this.cache.delete(key);
    });

    this.loadedRanges = this.loadedRanges.filter((range) => {
      return (
        Math.abs(range.start - keepIndex) <= 200 ||
        Math.abs(range.end - keepIndex) <= 200
      );
    });
  }

  clear(): void {
    this.cache.clear();
    this.idToIndex.clear();
    this.loadedRanges = [];
    this.pendingRanges.clear();
  }

  // Update the content of an item with a specific ID in cache
  updateItemContent(id: number, newContent: string): boolean {
    const index = this.idToIndex.get(id);
    if (index === undefined) return false;

    const item = this.cache.get(index);
    if (!item) return false;

    // Update item content
    this.cache.set(index, { ...item, content: newContent });
    return true;
  }

  // Incremental deletion: remove the item at the specified index and shift all following indexes by -1
  removeAtIndex(index: number): void {
    const item = this.cache.get(index);
    if (item?.id !== undefined) {
      this.idToIndex.delete(item.id);
    }
    this.cache.delete(index);

    // Shift all following indexes by -1
    const newCache = new Map<number, any>();
    const newIdToIndex = new Map<number, number>();

    this.cache.forEach((cachedItem, idx) => {
      const newIdx = idx > index ? idx - 1 : idx;
      newCache.set(newIdx, cachedItem);
      if (cachedItem.id !== undefined) {
        newIdToIndex.set(cachedItem.id, newIdx);
      }
    });

    this.cache = newCache;
    this.idToIndex = newIdToIndex;

    // Update loadedRanges
    this.loadedRanges = this.loadedRanges
      .map((range) => {
        if (range.start > index) {
          return { start: range.start - 1, end: range.end - 1 };
        } else if (range.end >= index) {
          return { start: range.start, end: range.end - 1 };
        }
        return range;
      })
      .filter((range) => range.start <= range.end);
    this.mergeRanges();
  }

  // Delete by ID and return the deleted index
  removeById(id: number): number | undefined {
    const index = this.idToIndex.get(id);
    if (index !== undefined) {
      this.removeAtIndex(index);
      return index;
    }
    return undefined;
  }

  // Batch-delete a specified index range in range selection mode
  removeAtIndexRange(start: number, end: number): void {
    const count = end - start + 1;

    // Delete all items in the range
    for (let i = start; i <= end; i++) {
      const item = this.cache.get(i);
      if (item?.id !== undefined) {
        this.idToIndex.delete(item.id);
      }
      this.cache.delete(i);
    }

    // Shift all following indexes forward
    const newCache = new Map<number, any>();
    const newIdToIndex = new Map<number, number>();

    this.cache.forEach((cachedItem, idx) => {
      const newIdx = idx > end ? idx - count : idx;
      newCache.set(newIdx, cachedItem);
      if (cachedItem.id !== undefined) {
        newIdToIndex.set(cachedItem.id, newIdx);
      }
    });

    this.cache = newCache;
    this.idToIndex = newIdToIndex;

    // Update loadedRanges
    this.loadedRanges = this.loadedRanges
      .map((range) => {
        if (range.start > end) {
          return { start: range.start - count, end: range.end - count };
        } else if (range.end >= start) {
          return {
            start: range.start,
            end: Math.max(range.start - 1, range.end - count),
          };
        }
        return range;
      })
      .filter((range) => range.start <= range.end);
    this.mergeRanges();
  }

  // Batch-delete multiple IDs in multi-select mode
  removeByIds(ids: Set<number>): number {
    // Get and sort all indexes to delete
    const indices = Array.from(ids)
      .map((id) => this.idToIndex.get(id))
      .filter((idx): idx is number => idx !== undefined)
      .sort((a, b) => a - b);

    if (indices.length === 0) return 0;

    // Delete from back to front to avoid index-shift issues
    for (let i = indices.length - 1; i >= 0; i--) {
      this.removeAtIndex(indices[i]);
    }

    return indices.length;
  }

  // Move item to a new position for drag reordering
  moveItem(fromIndex: number, toIndex: number): void {
    const item = this.cache.get(fromIndex);
    if (!item) return;

    // Ensure toIndex is within the valid range
    const maxIndex = Math.max(...Array.from(this.cache.keys()), 0);
    toIndex = Math.max(0, Math.min(toIndex, maxIndex));

    if (fromIndex === toIndex) return;

    // Create a new cache map
    const newCache = new Map<number, any>();
    const newIdToIndex = new Map<number, number>();

    if (fromIndex < toIndex) {
      // Move down: shift elements from fromIndex+1 to toIndex forward by one
      for (let i = 0; i < fromIndex; i++) {
        const cachedItem = this.cache.get(i);
        if (cachedItem) {
          newCache.set(i, cachedItem);
          if (cachedItem.id !== undefined) {
            newIdToIndex.set(cachedItem.id, i);
          }
        }
      }
      for (let i = fromIndex + 1; i <= toIndex; i++) {
        const cachedItem = this.cache.get(i);
        if (cachedItem) {
          newCache.set(i - 1, cachedItem);
          if (cachedItem.id !== undefined) {
            newIdToIndex.set(cachedItem.id, i - 1);
          }
        }
      }
      newCache.set(toIndex, item);
      if (item.id !== undefined) {
        newIdToIndex.set(item.id, toIndex);
      }
      // Elements after toIndex remain unchanged
      for (let i = toIndex + 1; i <= maxIndex; i++) {
        const cachedItem = this.cache.get(i);
        if (cachedItem) {
          newCache.set(i, cachedItem);
          if (cachedItem.id !== undefined) {
            newIdToIndex.set(cachedItem.id, i);
          }
        }
      }
    } else {
      // Move up: shift elements from toIndex to fromIndex-1 backward by one
      for (let i = 0; i < toIndex; i++) {
        const cachedItem = this.cache.get(i);
        if (cachedItem) {
          newCache.set(i, cachedItem);
          if (cachedItem.id !== undefined) {
            newIdToIndex.set(cachedItem.id, i);
          }
        }
      }
      newCache.set(toIndex, item);
      if (item.id !== undefined) {
        newIdToIndex.set(item.id, toIndex);
      }
      for (let i = toIndex; i < fromIndex; i++) {
        const cachedItem = this.cache.get(i);
        if (cachedItem) {
          newCache.set(i + 1, cachedItem);
          if (cachedItem.id !== undefined) {
            newIdToIndex.set(cachedItem.id, i + 1);
          }
        }
      }
      // Elements after fromIndex remain unchanged
      for (let i = fromIndex + 1; i <= maxIndex; i++) {
        const cachedItem = this.cache.get(i);
        if (cachedItem) {
          newCache.set(i, cachedItem);
          if (cachedItem.id !== undefined) {
            newIdToIndex.set(cachedItem.id, i);
          }
        }
      }
    }

    this.cache = newCache;
    this.idToIndex = newIdToIndex;
  }
}
