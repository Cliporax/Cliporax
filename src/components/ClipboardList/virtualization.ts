import {
  CARD_GAP,
  CONTENT_PADDING_BOTTOM,
  CONTENT_PADDING_TOP,
  IMAGE_HEIGHT,
  OVERSCAN,
  getTextHeight,
} from "./constants";
import type { ClipboardListProps } from "./types";

type LineHeight = NonNullable<ClipboardListProps["lineHeight"]>;

interface ClipboardItemLike {
  type?: "text" | "image" | "file";
}

export interface VisibleItem<T> {
  item: T;
  index: number;
  top: number;
}

export const shouldUseMouseReorder = (platform: string, userAgent: string, hasTouchEnd: boolean) => {
  const isMacOS = /Mac|iPod|iPhone|iPad/.test(platform) || (userAgent.includes("Mac") && !hasTouchEnd);
  const isWindows = /Win/.test(platform);
  return isMacOS || isWindows;
};

export const getVirtualItemHeight = (item: ClipboardItemLike, lineHeight: LineHeight) =>
  item.type === "image" ? IMAGE_HEIGHT : getTextHeight(lineHeight);

export const calculateSearchContentHeight = <T extends ClipboardItemLike>(
  searchResults: T[],
  lineHeight: LineHeight,
) => {
  if (searchResults.length === 0) {
    return 0;
  }

  return (
    searchResults.reduce((sum, item) => sum + getVirtualItemHeight(item, lineHeight) + CARD_GAP, 0) - CARD_GAP
  );
};

export const calculateSearchVisibleRange = <T extends ClipboardItemLike>(
  searchResults: T[],
  lineHeight: LineHeight,
  visibleContentTop: number,
  visibleContentBottom: number,
) => {
  if (searchResults.length === 0) return { start: 0, end: 0 };

  let start = 0;
  let end = searchResults.length - 1;
  let currentPos = 0;

  for (let i = 0; i < searchResults.length; i++) {
    const itemHeight = getVirtualItemHeight(searchResults[i], lineHeight);
    if (currentPos + itemHeight >= visibleContentTop) {
      start = i;
      break;
    }
    currentPos += itemHeight + CARD_GAP;
  }

  currentPos = 0;
  for (let i = 0; i < searchResults.length; i++) {
    const itemHeight = getVirtualItemHeight(searchResults[i], lineHeight);
    if (currentPos >= visibleContentBottom) {
      end = Math.max(i - 1, start);
      break;
    }
    if (i === searchResults.length - 1) {
      end = i;
    }
    currentPos += itemHeight + CARD_GAP;
  }

  return { start, end: Math.max(start, end) };
};

export const calculateSearchItemTop = <T extends ClipboardItemLike>(
  searchResults: T[],
  index: number,
  lineHeight: LineHeight,
) =>
  searchResults
    .slice(0, index)
    .reduce((sum, item) => sum + getVirtualItemHeight(item, lineHeight) + CARD_GAP, 0);

export const collectVisibleItems = <T extends ClipboardItemLike>({
  visibleStartIndex,
  visibleEndIndex,
  isSearchMode,
  searchResults,
  totalCount,
  lineHeight,
  getCachedItem,
  getCachedPosition,
}: {
  visibleStartIndex: number;
  visibleEndIndex: number;
  isSearchMode: boolean;
  searchResults: T[];
  totalCount: number;
  lineHeight: LineHeight;
  getCachedItem: (index: number) => T | undefined;
  getCachedPosition: (index: number) => number;
}) => {
  const items: VisibleItem<T>[] = [];
  let missingCount = 0;

  for (let i = visibleStartIndex - OVERSCAN; i <= visibleEndIndex + OVERSCAN; i++) {
    if (i < 0) continue;
    if (isSearchMode && i >= searchResults.length) break;
    if (!isSearchMode && i >= totalCount) break;

    const item = isSearchMode ? searchResults[i] : getCachedItem(i);
    if (item) {
      items.push({
        item,
        index: i,
        top: isSearchMode ? calculateSearchItemTop(searchResults, i, lineHeight) : getCachedPosition(i),
      });
    } else {
      missingCount++;
    }
  }

  return { items, missingCount };
};

export const calculateScrollbarMetrics = ({
  viewportHeight,
  contentHeight,
  totalHeight,
  scrollTop,
}: {
  viewportHeight: number;
  contentHeight: number;
  totalHeight: number;
  scrollTop: number;
}) => {
  const marginTop = CONTENT_PADDING_TOP;
  const marginBottom = CONTENT_PADDING_BOTTOM;
  const trackHeight = viewportHeight - marginTop - marginBottom;
  const thumbHeight =
    viewportHeight > 0 && contentHeight > 0
      ? Math.max(36, Math.min(trackHeight, (viewportHeight / (contentHeight + viewportHeight)) * trackHeight))
      : 0;
  const thumbMaxTravel = Math.max(0, trackHeight - thumbHeight);
  const maxScrollTop = Math.max(0, totalHeight - viewportHeight);
  const thumbTop = maxScrollTop > 0 ? (scrollTop / maxScrollTop) * thumbMaxTravel + marginTop : marginTop;

  return {
    marginTop,
    marginBottom,
    trackHeight,
    thumbHeight,
    thumbMaxTravel,
    maxScrollTop,
    thumbTop,
  };
};
