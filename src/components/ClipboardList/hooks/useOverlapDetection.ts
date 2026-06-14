import { useEffect, useRef } from "react";
import { createLogger } from "../../../utils/logger";
import { IMAGE_HEIGHT, getTextHeight } from "../constants";

const logger = createLogger("OverlapDetector");

export interface OverlapRecord {
  timestamp: number;
  itemCount: number;
  overlaps: Array<{
    itemA: {
      id: number;
      index: number;
      top: number;
      height: number;
      bottom: number;
    };
    itemB: {
      id: number;
      index: number;
      top: number;
      height: number;
      bottom: number;
    };
    overlapAmount: number;
  }>;
}

export interface UseOverlapDetectionProps {
  visibleItems: Array<{ item: any; index: number; top: number }>;
  lineHeight: "small" | "medium" | "large";
  cacheVersion: number;
}

export function useOverlapDetection({
  visibleItems,
  lineHeight,
  cacheVersion,
}: UseOverlapDetectionProps) {
  const overlapRecordsRef = useRef<OverlapRecord[]>([]);
  const maxRecords = 100; // Keep at most 100 records

  useEffect(() => {
    if (visibleItems.length === 0) return;

    // Calculate the actual position of each item
    const itemsWithBounds = visibleItems.map(({ item, index, top }) => {
      const height = item.type === "image" ? IMAGE_HEIGHT : getTextHeight(lineHeight);
      return {
        id: item.id,
        index,
        top,
        height,
        bottom: top + height,
        item,
      };
    });

    // Sort by top position
    itemsWithBounds.sort((a, b) => a.top - b.top);

    // Detect overlaps
    const overlaps: OverlapRecord["overlaps"] = [];

    for (let i = 0; i < itemsWithBounds.length - 1; i++) {
      const current = itemsWithBounds[i];
      const next = itemsWithBounds[i + 1];

      // If the next item top is less than the current item bottom, they overlap
      if (next.top < current.bottom) {
        const overlapAmount = current.bottom - next.top;
        overlaps.push({
          itemA: {
            id: current.id,
            index: current.index,
            top: current.top,
            height: current.height,
            bottom: current.bottom,
          },
          itemB: {
            id: next.id,
            index: next.index,
            top: next.top,
            height: next.height,
            bottom: next.bottom,
          },
          overlapAmount,
        });
      }
    }

    // Record to history when an overlap is detected
    if (overlaps.length > 0) {
      const record: OverlapRecord = {
        timestamp: Date.now(),
        itemCount: visibleItems.length,
        overlaps,
      };

      overlapRecordsRef.current.push(record);

      // Limit the number of records
      if (overlapRecordsRef.current.length > maxRecords) {
        overlapRecordsRef.current = overlapRecordsRef.current.slice(-maxRecords);
      }

      // Print detailed debug information
      logger.error(
        `[Overlap Detected] Found ${overlaps.length} overlap(s) in ${visibleItems.length} visible items`,
      );
      logger.error(`[Overlap] Cache version: ${cacheVersion}, Line height: ${lineHeight}`);

      overlaps.forEach((overlap, idx) => {
        logger.error(`[Overlap #${idx + 1}]`, {
          itemA: {
            id: overlap.itemA.id,
            index: overlap.itemA.index,
            top: overlap.itemA.top.toFixed(2),
            height: overlap.itemA.height,
            bottom: overlap.itemA.bottom.toFixed(2),
          },
          itemB: {
            id: overlap.itemB.id,
            index: overlap.itemB.index,
            top: overlap.itemB.top.toFixed(2),
            height: overlap.itemB.height,
            bottom: overlap.itemB.bottom.toFixed(2),
          },
          overlapAmount: overlap.overlapAmount.toFixed(2),
        });
      });

      // Print the full visibleItems cache data
      logger.error("[Overlap] Full visible items cache:", {
        items: visibleItems.map(({ item, index, top }) => ({
          id: item.id,
          index,
          top: top.toFixed(2),
          type: item.type,
          is_pinned: item.is_pinned,
        })),
      });

      // Print position calculation details
      logger.error("[Overlap] Item positions (sorted by top):", 
        itemsWithBounds.map(item => ({
          id: item.id,
          index: item.index,
          top: item.top.toFixed(2),
          bottom: item.bottom.toFixed(2),
          height: item.height,
        }))
      );
    }
  }, [visibleItems, lineHeight, cacheVersion]);

  // Expose a method for retrieving overlap records
  const getOverlapRecords = () => overlapRecordsRef.current;

  const clearOverlapRecords = () => {
    overlapRecordsRef.current = [];
    logger.info("[OverlapDetector] Records cleared");
  };

  return {
    getOverlapRecords,
    clearOverlapRecords,
    hasOverlaps: overlapRecordsRef.current.length > 0,
  };
}
