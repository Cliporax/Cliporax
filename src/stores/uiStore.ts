import { create } from "zustand";

export interface EditingItem {
  id: number;
  content: string;
  type: "text" | "image" | "file";
}

interface UIState {
  // Search
  showSearch: boolean;
  searchQuery: string;
  searchMode: "fuzzy" | "regex";
  searchScope: "current" | "global";
  setShowSearch: (show: boolean) => void;
  setSearchQuery: (query: string) => void;
  setSearchMode: (mode: "fuzzy" | "regex") => void;
  setSearchScope: (scope: "current" | "global") => void;
  toggleSearch: () => void;

  // Settings
  isSettingsOpen: boolean;
  openSettings: () => void;
  closeSettings: () => void;

  // Editor
  editingItem: EditingItem | null;
  openEditor: (item: EditingItem) => void;
  closeEditor: () => void;

  // Multi-select
  isMultiSelectMode: boolean;
  selectedItemIds: Set<number>;
  enterMultiSelectMode: () => void;
  exitMultiSelectMode: () => void;
  toggleItemSelection: (itemId: number) => void;
  clearSelection: () => void;
  isSelected: (itemId: number) => boolean;
}

export const useUIStore = create<UIState>()((set, get) => ({
  // Search
  showSearch: false,
  searchQuery: "",
  searchMode: "fuzzy",
  searchScope: "current",
  setShowSearch: (showSearch) => set({ showSearch }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setSearchMode: (searchMode) => set({ searchMode }),
  setSearchScope: (searchScope) => set({ searchScope }),
  toggleSearch: () =>
    set((state) => ({
      showSearch: !state.showSearch,
      searchQuery: state.showSearch ? "" : state.searchQuery,
    })),

  // Settings
  isSettingsOpen: false,
  openSettings: () => set({ isSettingsOpen: true }),
  closeSettings: () => set({ isSettingsOpen: false }),

  // Editor
  editingItem: null,
  openEditor: (editingItem) => set({ editingItem }),
  closeEditor: () => set({ editingItem: null }),

  // Multi-select
  isMultiSelectMode: false,
  selectedItemIds: new Set<number>(),
  enterMultiSelectMode: () => set({ isMultiSelectMode: true }),
  exitMultiSelectMode: () =>
    set({ isMultiSelectMode: false, selectedItemIds: new Set() }),
  toggleItemSelection: (itemId) => {
    const { selectedItemIds } = get();
    const newSet = new Set(selectedItemIds);
    if (newSet.has(itemId)) {
      newSet.delete(itemId);
    } else {
      newSet.add(itemId);
    }
    set({ selectedItemIds: newSet });
  },
  clearSelection: () => set({ selectedItemIds: new Set() }),
  isSelected: (itemId) => get().selectedItemIds.has(itemId),
}));
