import { create } from "zustand";

interface SelectionRange {
  start: number;
  end: number;
}

interface SelectionState {
  selectedId: number | null;
  setSelectedId: (id: number | null) => void;

  checkedIds: Set<number>;
  toggleChecked: (id: number) => void;
  setCheckedIds: (ids: Set<number>) => void;
  clearSelection: () => void;

  // Range selection for Shift+click
  selectionRange: SelectionRange | null;
  setSelectionRange: (range: SelectionRange | null) => void;

  // Last checked index for range selection anchor
  lastCheckedIndex: number | null;
  setLastCheckedIndex: (index: number | null) => void;

  focusedIndex: number;
  setFocusedIndex: (index: number) => void;
}

export const useSelectionStore = create<SelectionState>()((set, get) => ({
  selectedId: null,
  checkedIds: new Set(),
  focusedIndex: 0,
  selectionRange: null,
  lastCheckedIndex: null,

  setSelectedId: (id) => set({ selectedId: id }),

  toggleChecked: (id) =>
    set((state) => {
      const newSet = new Set(state.checkedIds);
      if (newSet.has(id)) {
        newSet.delete(id);
      } else {
        newSet.add(id);
      }
      return { checkedIds: newSet };
    }),

  setCheckedIds: (ids) => set({ checkedIds: new Set(ids) }),

  clearSelection: () =>
    set({
      selectedId: null,
      checkedIds: new Set(),
      selectionRange: null,
      lastCheckedIndex: null,
      focusedIndex: 0,
    }),

  setSelectionRange: (range) => set({ selectionRange: range }),

  setLastCheckedIndex: (index) => set({ lastCheckedIndex: index }),

  setFocusedIndex: (index) => set({ focusedIndex: index }),
}));

// Helper function to check if index is in range (outside store for performance)
export function isIndexInRange(
  index: number,
  selectionRange: SelectionRange | null,
): boolean {
  if (!selectionRange) return false;
  const start = Math.min(selectionRange.start, selectionRange.end);
  const end = Math.max(selectionRange.start, selectionRange.end);
  return index >= start && index <= end;
}
