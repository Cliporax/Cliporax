import { create } from "zustand";
import { persist } from "zustand/middleware";
import { settings } from "../lib/tauri-api";

export interface AppSettings {
  theme: "light" | "dark" | "system";
  max_items: number;
  max_images: number;
  line_height: "small" | "medium" | "large";
  auto_start: boolean;
  auto_hide: boolean;
  shortcut_toggle_window: string;
}

interface SettingsState extends AppSettings {
  updateSettings: (updates: Partial<AppSettings>) => Promise<void>;
  setTheme: (theme: "light" | "dark" | "system") => Promise<void>;
  setLineHeight: (height: "small" | "medium" | "large") => Promise<void>;
  toggleAutoHide: () => Promise<void>;
}

const defaultSettings: AppSettings = {
  theme: "dark",
  max_items: 1000,
  max_images: 500,
  line_height: "medium",
  auto_start: false,
  auto_hide: true,
  shortcut_toggle_window: "Ctrl+Shift+V",
};

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set, get) => ({
      ...defaultSettings,

      updateSettings: async (updates) => {
        const newSettings = { ...get(), ...updates };
        await settings.update(newSettings);
        set(updates);
      },

      setTheme: async (theme) => {
        await settings.update({ theme });
        set({ theme });
      },
      setLineHeight: async (line_height) => {
        await settings.update({ line_height });
        set({ line_height });
      },
      toggleAutoHide: async () => {
        const newAutoHide = !get().auto_hide;
        await settings.update({ auto_hide: newAutoHide });
        set({ auto_hide: newAutoHide });
      },
    }),
    {
      name: "cliporax-settings",
      onRehydrateStorage: () => () => {
        void settings
          .getAll()
          .then((data) => {
            useSettingsStore.setState({
              theme: (data.theme as "light" | "dark" | "system") || "dark",
              max_items: data.max_items || 1000,
              max_images: data.max_images || 500,
              line_height:
                (data.line_height as "small" | "medium" | "large") ||
                "medium",
              auto_start: data.auto_start || false,
              auto_hide: data.auto_hide !== false,
              shortcut_toggle_window:
                data.shortcut_toggle_window || "Ctrl+Shift+V",
            });
          })
          .catch((e) => {
            console.error("Failed to load settings:", e);
          });
      },
    },
  ),
);
