// Method interface exposed to the parent component
export interface ClipboardListRef {
  enterMultiSelectMode: () => void;
  exitMultiSelectMode: () => void;
  getMergedText: () => string;
  getCheckedIds: () => Set<number>;
  getSelectedItems: () => any[];
  updateItemContent: (id: number, newContent: string) => boolean;
  // Overlap detection fields
  getOverlapRecords: () => any[];
  clearOverlapRecords: () => void;
  hasOverlaps: () => boolean;
}

export interface ClipboardListProps {
  searchQuery?: string;
  searchMode?: "fuzzy" | "regex";
  lineHeight?: "small" | "medium" | "large";
  tabId?: number | null; // Current active tab ID
  onEdit?: (item: {
    id: number;
    content: string;
    type: "text" | "image" | "file";
  }) => void;
  refreshTrigger?: number;
  onMultiSelectChange?: (
    selectedIds: Set<number>,
    selectedItems: any[],
  ) => void;
}
