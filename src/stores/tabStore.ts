import { create } from "zustand";
import { tabs as tabsApi } from "../lib/tauri-api";
import type { Tab } from "../types/generated/api";

interface TabState {
  // State
  tabs: Tab[];
  activeTabId: number | null;
  activePluginTabId: string | null;
  isLoading: boolean;

  // Actions
  loadTabs: () => Promise<void>;
  createTab: (name: string) => Promise<void>;
  deleteTab: (id: number) => Promise<void>;
  renameTab: (id: number, name: string) => Promise<void>;
  setActiveTab: (tabId: number) => void;
  setActivePluginTab: (tabId: string | null) => void;
  getAutoCaptureTabs: () => Tab[];
}

export const useTabStore = create<TabState>((set, get) => ({
  // Initial state
  tabs: [],
  activeTabId: null,
  activePluginTabId: null,
  isLoading: false,

  // Actions
  loadTabs: async () => {
    set({ isLoading: true });
    try {
      const tabs = await tabsApi.getAll();
      set({ tabs, isLoading: false });

      // Set default active tab if not set
      const state = get();
      if (!state.activeTabId && tabs.length > 0) {
        const defaultTab = tabs.find((t) => t.is_default);
        set({ activeTabId: defaultTab?.id ?? tabs[0]?.id ?? null });
      }
    } catch (error) {
      console.error("[tabStore] Failed to load tabs:", error);
      set({ isLoading: false });
    }
  },

  createTab: async (name: string) => {
    try {
      await tabsApi.create(name);
      // Reload tabs after creation
      await get().loadTabs();
    } catch (error) {
      console.error("[tabStore] Failed to create tab:", error);
      throw error;
    }
  },

  deleteTab: async (id: number) => {
    try {
      await tabsApi.delete(id);

      // Update state after deletion
      set((state) => {
        const newTabs = state.tabs.filter((t) => t.id !== id);
        let newActiveTabId = state.activeTabId;

        // If active tab was deleted, switch to default tab
        if (state.activeTabId === id && newTabs.length > 0) {
          const defaultTab = newTabs.find((t) => t.is_default);
          newActiveTabId = defaultTab?.id ?? newTabs[0]?.id ?? null;
        }

        return { tabs: newTabs, activeTabId: newActiveTabId };
      });
    } catch (error) {
      console.error("[tabStore] Failed to delete tab:", error);
      throw error;
    }
  },

  renameTab: async (id: number, name: string) => {
    try {
      // Frontend validation
      if (!name || name.trim().length === 0) {
        throw new Error("Tab name cannot be empty");
      }

      // Check reserved names
      const reservedNames = ["Default", "System Clipboard"];
      const trimmedName = name.trim();
      if (
        reservedNames.some((n) => n.toLowerCase() === trimmedName.toLowerCase())
      ) {
        throw new Error(`Tab name '${trimmedName}' is reserved`);
      }

      await tabsApi.rename(id, trimmedName);

      // Update state after rename
      set((state) => {
        const newTabs = state.tabs.map((t) =>
          t.id === id ? { ...t, name: trimmedName } : t,
        );
        return { tabs: newTabs };
      });
    } catch (error) {
      console.error("[tabStore] Failed to rename tab:", error);
      throw error;
    }
  },

  setActiveTab: (tabId: number) => {
    set({ activeTabId: tabId, activePluginTabId: null });
  },

  setActivePluginTab: (tabId: string | null) => {
    set({ activePluginTabId: tabId });
  },

  getAutoCaptureTabs: () => {
    const state = get();
    return state.tabs.filter((t) => t.auto_capture);
  },
}));
