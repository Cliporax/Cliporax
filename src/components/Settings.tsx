import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-shell";
import i18n from "../i18n";
import {
  X,
  Sun,
  Moon,
  Monitor,
  Download,
  Keyboard,
  Info,
  Puzzle,
  FolderSync,
  AlertCircle,
  TestTube,
  Trash2,
  Loader2,
  Hash,
  List,
  Pencil,
  Pin,
  ListTodo,
  Languages,
} from "lucide-react";
import { createLogger } from "../utils/logger";
import { withTrace } from "../utils/traced-invoke";
import {
  shortcut as shortcutApi,
  settings as settingsApi,
  test as testApi,
  window as windowApi,
  AppSettings,
} from "../lib/tauri-api";
import { useTheme } from "../contexts/ThemeContext";
import { PluginProvider, useExtensionManager } from "../plugin";
import PluginsTab from "./Settings/PluginsTab";
import CloudSyncTab from "./Settings/CloudSyncTab";
import { ResizeHandles } from "./ResizeHandles";

// Export AppSettings type for SettingsWindow
export type { AppSettings } from "../lib/tauri-api";

const logger = createLogger("Settings");

const STORAGE_KEY = "cliporax-settings";
const GITHUB_REPO_URL = "https://github.com/Cliporax/Cliporax";
const GITHUB_RELEASES_URL = `${GITHUB_REPO_URL}/releases`;
const GITHUB_CURRENT_RELEASE_URL = `${GITHUB_RELEASES_URL}/tag/v${__APP_VERSION__}`;
const GITHUB_LATEST_RELEASE_URL = `${GITHUB_RELEASES_URL}/latest`;
const GITHUB_DOCS_URL = `${GITHUB_REPO_URL}/blob/master/docs/README.md`;
const GITHUB_ISSUES_URL = `${GITHUB_REPO_URL}/issues/new`;

type SettingsTab = "general" | "shortcuts" | "plugins" | "sync" | "about";

export interface GeneralSettings {
  theme: "light" | "dark" | "system";
  maxItems: number;
  maxImages: number;
  lineHeight: "small" | "medium" | "large";
  autoStart: boolean;
  autoHide: boolean;
  excludedTextPatterns: string[];
  showItemIndex: boolean;
  showLineCount: boolean;
  showSourceHost: boolean;
  showActionButtons: boolean;
  showEditButton: boolean;
  showPinButton: boolean;
  showPluginActionButtons: boolean;
  pluginActionVisibility: Record<string, boolean>;
}

export interface ShortcutSettings {
  toggleWindow: string;
  search: string;
  togglePin: string;
  deleteItem: string;
}

const defaultGeneralSettings: GeneralSettings = {
  theme: "dark",
  maxItems: 1000,
  maxImages: 500,
  lineHeight: "medium",
  autoStart: false,
  autoHide: true,
  excludedTextPatterns: [],
  showItemIndex: true,
  showLineCount: true,
  showSourceHost: true,
  showActionButtons: true,
  showEditButton: true,
  showPinButton: true,
  showPluginActionButtons: true,
  pluginActionVisibility: {},
};

const defaultShortcutSettings: ShortcutSettings = {
  toggleWindow: "Ctrl+Shift+V",
  search: "Ctrl+F",
  togglePin: "Ctrl+P",
  deleteItem: "Delete",
};

const toTauriShortcut = (shortcut: string) =>
  shortcut
    .replace(/\bMeta\b/g, "Cmd")
    .replace(/\bCommand\b/g, "Cmd")
    .replace(/\bControl\b/g, "Ctrl");

const fromTauriShortcut = (shortcut?: string) => {
  const fallback = defaultShortcutSettings.toggleWindow;
  if (!shortcut) return fallback;

  const platformModifier =
    navigator.platform.toUpperCase().includes("MAC") ? "Cmd" : "Ctrl";

  return shortcut
    .replace(/\bCmdOrControl\b/g, platformModifier)
    .replace(/\bCommandOrControl\b/g, platformModifier)
    .replace(/\bMeta\b/g, "Cmd")
    .replace(/\bCommand\b/g, "Cmd")
    .replace(/\bControl\b/g, "Ctrl");
};

export const backendToFrontendSettings = (
  backend: AppSettings,
  existing = loadSettings(),
) => ({
  general: {
    theme: backend.theme as GeneralSettings["theme"],
    maxItems: backend.max_items,
    maxImages: backend.max_images,
    lineHeight: backend.line_height as GeneralSettings["lineHeight"],
    autoStart: backend.auto_start,
    autoHide: backend.auto_hide,
    excludedTextPatterns: backend.excluded_text_patterns ?? [],
    showItemIndex: backend.show_item_index,
    showLineCount: backend.show_line_count,
    showSourceHost: backend.show_source_host,
    showActionButtons: backend.show_action_buttons,
    showEditButton: backend.show_edit_button,
    showPinButton: backend.show_pin_button,
    showPluginActionButtons: backend.show_plugin_action_buttons,
    pluginActionVisibility: backend.plugin_action_visibility ?? {},
  },
  shortcuts: {
    ...defaultShortcutSettings,
    ...existing.shortcuts,
    toggleWindow: fromTauriShortcut(backend.shortcut_toggle_window),
  },
});

// Load settings from localStorage
export const loadSettings = () => {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    logger.debug("Loading settings from localStorage:", stored);
    if (stored) {
      const parsed = JSON.parse(stored);
      const result = {
        general: { ...defaultGeneralSettings, ...parsed.general },
        shortcuts: { ...defaultShortcutSettings, ...parsed.shortcuts },
      };
      logger.debug("Loaded settings:", result);
      return result;
    }
  } catch (error) {
    logger.error("Failed to load settings:", error);
  }
  return {
    general: defaultGeneralSettings,
    shortcuts: defaultShortcutSettings,
  };
};

// Save settings to localStorage
export const saveSettings = (
  general: GeneralSettings,
  shortcuts: ShortcutSettings,
) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ general, shortcuts }));
    logger.info("Settings saved");
  } catch (error) {
    logger.error("Failed to save settings:", error);
  }
};

/**
 * Sync General settings to the backend Rust SettingsManager
 * Use debouncing so rapid consecutive clicks execute only the last request
 */
let syncDebounceTimer: ReturnType<typeof setTimeout> | null = null;
let latestSettings: {
  general: GeneralSettings;
  shortcut?: ShortcutSettings;
} | null = null;

export const syncSettingsToBackend = (
  general: GeneralSettings,
  shortcut?: ShortcutSettings,
) => {
  latestSettings = { general, shortcut };
  logger.info(
    "[syncSettingsToBackend] QUEUED, lineHeight:",
    general.lineHeight,
  );

  if (syncDebounceTimer) {
    clearTimeout(syncDebounceTimer);
  }

  syncDebounceTimer = setTimeout(async () => {
    syncDebounceTimer = null;
    const settings = latestSettings;
    if (!settings) return;
    latestSettings = null;

    try {
      const g = settings.general;
      const s = settings.shortcut ?? loadSettings().shortcuts;
      logger.info(
        "[syncSettingsToBackend] CALLING backend update, lineHeight:",
        g.lineHeight,
      );

      // Use withTrace to enable trace context for this operation
      await withTrace("Settings", "sync_to_backend", async () => {
        await settingsApi.updateFull({
          theme: g.theme,
          max_items: g.maxItems,
          max_images: g.maxImages,
          line_height: g.lineHeight,
          auto_start: g.autoStart,
          auto_hide: g.autoHide,
          excluded_text_patterns: g.excludedTextPatterns,
          show_item_index: g.showItemIndex,
          show_line_count: g.showLineCount,
          show_source_host: g.showSourceHost,
          show_action_buttons: g.showActionButtons,
          show_edit_button: g.showEditButton,
          show_pin_button: g.showPinButton,
          show_plugin_action_buttons: g.showPluginActionButtons,
          plugin_action_visibility: g.pluginActionVisibility,
          shortcut_toggle_window: toTauriShortcut(s.toggleWindow),
        });
      });

      logger.info("[syncSettingsToBackend] COMPLETED");
    } catch (error) {
      logger.error("Failed to sync settings to backend:", error);
    }
  }, 150);
};

// Info about shortcuts (for descriptions)
const getShortcutInfo = (t: any) => ({
  toggleWindow: {
    label: t("settings.shortcuts.toggleWindow"),
    description: t("settings.shortcuts.toggleWindowDesc"),
    global: true,
  },
  search: {
    label: t("settings.shortcuts.search"),
    description: t("settings.shortcuts.searchDesc"),
    global: false,
  },
  togglePin: {
    label: t("settings.shortcuts.togglePin"),
    description: t("settings.shortcuts.togglePinDesc"),
    global: false,
  },
  deleteItem: {
    label: t("settings.shortcuts.deleteItem"),
    description: t("settings.shortcuts.deleteItemDesc"),
    global: false,
  },
});

interface SettingsProps {
  onClose?: () => void;
  initialGeneralSettings?: GeneralSettings;
  initialShortcutSettings?: ShortcutSettings;
  onSettingsChange?: (
    general: GeneralSettings,
    shortcuts: ShortcutSettings,
  ) => void;
  isWindow?: boolean; // Whether this is standalone window mode
}

const Settings: React.FC<SettingsProps> = ({
  onClose,
  initialGeneralSettings,
  initialShortcutSettings,
  onSettingsChange,
  isWindow = false, // Defaults to non-standalone window mode
}) => {
  const { resolvedTheme, setTheme } = useTheme();
  const { getExtensions } = useExtensionManager();
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [generalSettings, setGeneralSettings] = useState<GeneralSettings>(
    initialGeneralSettings || defaultGeneralSettings,
  );
  const [shortcutSettings, setShortcutSettings] = useState<ShortcutSettings>(
    initialShortcutSettings || defaultShortcutSettings,
  );
  const isDark = resolvedTheme === "dark";
  const pluginActionExtensions = [
    ...getExtensions("card"),
    ...getExtensions("context-menu"),
  ].filter(
    (extension, index, extensions) =>
      extensions.findIndex((candidate) => candidate.id === extension.id) === index,
  );
  const { t } = useTranslation();
  const shortcutInfo = getShortcutInfo(t);
  const [editingShortcut, setEditingShortcut] = useState<
    keyof ShortcutSettings | null
  >(null);
  const [conflictInfo, setConflictInfo] = useState<{
    key: keyof ShortcutSettings;
    message: string;
  } | null>(null);

  // Track the shortcut that was paused, so we can resume it when editing is cancelled
  // This is needed to distinguish between "user set new shortcut" and "user cancelled editing"
  const pausedShortcutRef = useRef<string | null>(null);

  // Test data loading state
  const [isLoading, setIsLoading] = useState(false);
  const [testResult, setTestResult] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  const handleOpenExternal = useCallback(async (url: string) => {
    try {
      await open(url);
    } catch (error) {
      logger.error("[Settings] Failed to open external URL:", error);
    }
  }, []);

  // Debug logging
  useEffect(() => {
    logger.debug(
      "Settings mounted with initial shortcuts:",
      initialShortcutSettings,
    );
  }, [initialShortcutSettings]);

  // Handle global shortcut resume when exiting shortcut recording mode
  // Resume is only needed when user cancels editing (not when they set a new shortcut)
  useEffect(() => {
    // When editingShortcut changes from "toggleWindow" to null, check if we need to resume
    if (editingShortcut !== "toggleWindow" && pausedShortcutRef.current) {
      // User cancelled editing - resume the paused shortcut
      const shortcutToResume = pausedShortcutRef.current;
      pausedShortcutRef.current = null;

      (async () => {
        try {
          logger.info(
            "[Settings] Resuming global shortcut after recording cancelled:",
            shortcutToResume,
          );
          await shortcutApi.resume(shortcutToResume);
        } catch (error) {
          logger.error("[Settings] Failed to resume global shortcut:", error);
        }
      })();
    }
  }, [editingShortcut]);

  // Cleanup on unmount - resume shortcut if still in editing mode
  useEffect(() => {
    return () => {
      if (pausedShortcutRef.current) {
        const shortcutToResume = pausedShortcutRef.current;
        logger.info(
          "[Settings] Component unmounting, resuming shortcut:",
          shortcutToResume,
        );
        // Fire and forget since component is unmounting
        shortcutApi.resume(shortcutToResume).catch((error) => {
          logger.error(
            "[Settings] Failed to resume shortcut on unmount:",
            error,
          );
        });
      }
    };
  }, []);

  // Sync internal state when external initialGeneralSettings changes
  // Do not trigger onSettingsChange to avoid loops
  useEffect(() => {
    if (initialGeneralSettings) {
      setGeneralSettings((prev) => {
        // Update only when values actually differ to avoid unnecessary rerenders
        if (
          prev.theme === initialGeneralSettings.theme &&
          prev.maxItems === initialGeneralSettings.maxItems &&
          prev.maxImages === initialGeneralSettings.maxImages &&
          prev.lineHeight === initialGeneralSettings.lineHeight &&
          prev.autoStart === initialGeneralSettings.autoStart &&
          prev.autoHide === initialGeneralSettings.autoHide &&
          prev.excludedTextPatterns.join("\n") ===
            initialGeneralSettings.excludedTextPatterns.join("\n")
        ) {
          logger.debug("[Settings] initialGeneralSettings unchanged, skipping");
          return prev;
        }
        logger.info(
          "[Settings] Syncing from initialGeneralSettings, lineHeight:",
          initialGeneralSettings.lineHeight,
        );
        return initialGeneralSettings;
      });
    }
  }, [initialGeneralSettings]);

  // Sync internal state when external initialShortcutSettings changes
  useEffect(() => {
    if (initialShortcutSettings) {
      setShortcutSettings((prev) => {
        if (
          prev.toggleWindow === initialShortcutSettings.toggleWindow &&
          prev.search === initialShortcutSettings.search &&
          prev.togglePin === initialShortcutSettings.togglePin &&
          prev.deleteItem === initialShortcutSettings.deleteItem
        ) {
          return prev;
        }
        return initialShortcutSettings;
      });
    }
  }, [initialShortcutSettings]);

  // Save settings to localStorage whenever they change
  // Note: do not call onSettingsChange here because button clicks already call syncSettingsToBackend explicitly
  useEffect(() => {
    logger.debug(
      "[Settings] saveSettings effect triggered, lineHeight:",
      generalSettings.lineHeight,
    );
    saveSettings(generalSettings, shortcutSettings);
  }, [generalSettings, shortcutSettings]);

  // Check if a shortcut conflicts with existing shortcuts
  const checkShortcutConflict = (
    newShortcut: string,
    currentKey: keyof ShortcutSettings,
  ): {
    hasConflict: boolean;
    conflictKey?: keyof ShortcutSettings;
    conflictLabel?: string;
  } => {
    const normalizedNew = newShortcut.toLowerCase().replace(/\s/g, "");

    for (const [existingKey, existingShortcut] of Object.entries(
      shortcutSettings,
    )) {
      if (existingKey === currentKey) continue;

      const normalizedExisting = existingShortcut
        .toLowerCase()
        .replace(/\s/g, "");
      if (normalizedNew === normalizedExisting) {
        return {
          hasConflict: true,
          conflictKey: existingKey as keyof ShortcutSettings,
          conflictLabel:
            shortcutInfo[existingKey as keyof ShortcutSettings].label,
        };
      }
    }

    return { hasConflict: false };
  };

  const handleShortcutCapture = async (
    key: keyof ShortcutSettings,
    e: React.KeyboardEvent,
  ) => {
    e.preventDefault();

    // Handle Escape key to cancel editing
    if (e.key === "Escape") {
      logger.info("[Settings] Shortcut recording cancelled by Escape key");
      setEditingShortcut(null);
      setConflictInfo(null);
      return;
    }

    const modifiers: string[] = [];
    if (e.ctrlKey) modifiers.push("Ctrl");
    if (e.altKey) modifiers.push("Alt");
    if (e.shiftKey) modifiers.push("Shift");
    if (e.metaKey) modifiers.push("Cmd");

    const keyName = e.key.toUpperCase();
    if (["CONTROL", "ALT", "SHIFT", "META"].includes(keyName)) return;

    const newShortcut =
      modifiers.length > 0 ? `${modifiers.join("+")}+${keyName}` : keyName;
    const oldShortcut = shortcutSettings[key];

    // Check for conflicts with other shortcuts
    const conflict = checkShortcutConflict(newShortcut, key);
    if (conflict.hasConflict) {
      logger.warn(
        `Shortcut conflict detected: "${newShortcut}" conflicts with "${conflict.conflictLabel}" (${conflict.conflictKey})`,
      );
      // Show inline conflict warning
      setConflictInfo({
        key,
        message: t("settings.shortcuts.conflict", {
          label: conflict.conflictLabel,
        }),
      });
      // Auto clear conflict after 2 seconds and restore editing state
      setTimeout(() => {
        setConflictInfo(null);
        setEditingShortcut(null);
      }, 2000);
      return;
    }

    // Update global shortcut in main process if it's the toggleWindow shortcut
    if (key === "toggleWindow") {
      const oldTauriShortcut = toTauriShortcut(oldShortcut);
      const newTauriShortcut = toTauriShortcut(newShortcut);

      try {
        // Keep explicit Ctrl and Cmd distinct. CmdOrControl is only for defaults
        // where the user did not choose a platform-specific modifier.
        logger.debug(
          "Converting shortcuts - old:",
          oldTauriShortcut,
          "new:",
          newTauriShortcut,
        );
        const success = await shortcutApi.update(
          oldTauriShortcut,
          newTauriShortcut,
        );
        if (success) {
          // Clear the paused shortcut ref since backend registered the new one.
          pausedShortcutRef.current = null;
          setShortcutSettings((prev) => {
            const updated = { ...prev, [key]: newShortcut };
            onSettingsChange?.(generalSettings, updated);
            return updated;
          });
          setEditingShortcut(null);
          logger.info("Shortcut updated:", key, newShortcut);
          logger.info(
            "Global shortcut updated successfully:",
            newTauriShortcut,
          );
          // Also save to database for persistence across restarts
          try {
            await settingsApi.updateToggleWindowShortcut(newTauriShortcut);
            logger.info("Shortcut saved to database:", newTauriShortcut);
          } catch (dbError) {
            logger.error("Failed to save shortcut to database:", dbError);
          }
        } else {
          logger.error("Failed to update global shortcut");
          await shortcutApi.resume(oldTauriShortcut);
          pausedShortcutRef.current = null;
          setEditingShortcut(null);
        }
      } catch (error) {
        logger.error("Failed to update global shortcut:", error);
        try {
          await shortcutApi.resume(oldTauriShortcut);
        } catch (resumeError) {
          logger.error("Failed to restore previous global shortcut:", resumeError);
        }
        pausedShortcutRef.current = null;
        setEditingShortcut(null);
      }
      return;
    }

    setShortcutSettings((prev) => {
      const updated = { ...prev, [key]: newShortcut };
      // Notify the parent component about shortcut changes
      onSettingsChange?.(generalSettings, updated);
      return updated;
    });
    setEditingShortcut(null);
    logger.info("Shortcut updated:", key, newShortcut);
  };

  // Handle starting shortcut editing - immediately pause global shortcut
  const handleStartEditing = async (key: keyof ShortcutSettings) => {
    setEditingShortcut(key);

    // If editing the global toggle shortcut, pause it immediately
    // This must be done synchronously (before useEffect) to prevent race condition
    if (key === "toggleWindow") {
      const currentShortcut = toTauriShortcut(shortcutSettings.toggleWindow);
      pausedShortcutRef.current = currentShortcut;

      try {
        logger.info(
          "[Settings] Immediately pausing global shortcut for recording:",
          currentShortcut,
        );
        await shortcutApi.pause(currentShortcut);
      } catch (error) {
        logger.error("[Settings] Failed to pause global shortcut:", error);
        pausedShortcutRef.current = null;
      }
    }
  };

  // Handle window drag start for settings window title bar
  const isDraggingRef = React.useRef(false);

  const handleDragStart = async (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;

    isDraggingRef.current = true;
    try {
      await windowApi.startDragging();
    } catch (error) {
      isDraggingRef.current = false;
      logger.error("Failed to start dragging:", error);
    }
  };

  // Handle mouse up for ending window drag (settings window mode)
  // Only call endDragging if we actually started dragging
  useEffect(() => {
    if (!isWindow) return;

    const handleGlobalMouseUp = async () => {
      if (!isDraggingRef.current) return;
      isDraggingRef.current = false;
      try {
        await windowApi.endDragging();
      } catch (error) {
        // Ignore errors - might be called multiple times
      }
    };
    window.addEventListener("mouseup", handleGlobalMouseUp);
    return () => {
      window.removeEventListener("mouseup", handleGlobalMouseUp);
    };
  }, [isWindow]);

  const tabs: { id: SettingsTab; label: string; icon: React.ReactNode }[] = [
    {
      id: "general",
      label: t("settings.tabs.general"),
      icon: <Monitor size={16} />,
    },
    {
      id: "shortcuts",
      label: t("settings.tabs.shortcuts"),
      icon: <Keyboard size={16} />,
    },
    {
      id: "plugins",
      label: t("settings.tabs.plugins"),
      icon: <Puzzle size={16} />,
    },
    {
      id: "sync",
      label: t("settings.tabs.sync"),
      icon: <FolderSync size={16} />,
    },
    { id: "about", label: t("settings.tabs.about"), icon: <Info size={16} /> },
  ];

  const renderGeneralSettings = () => (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <div className="order-1 space-y-3 rounded-xl border p-4 lg:col-span-2" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.appearance")}</p>
        <label
          className="text-sm font-medium transition-colors duration-300"
          style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
        >
          {t("settings.general.theme")}
        </label>
        <div className="flex gap-2">
          {[
            {
              value: "light",
              icon: <Sun size={16} />,
              label: t("settings.general.themeOptions.light"),
            },
            {
              value: "dark",
              icon: <Moon size={16} />,
              label: t("settings.general.themeOptions.dark"),
            },
            {
              value: "system",
              icon: <Monitor size={16} />,
              label: t("settings.general.themeOptions.system"),
            },
          ].map((option) => (
            <button
              key={option.value}
              data-testid={`settings-theme-${option.value}`}
              onClick={() => {
                const themeValue = option.value as GeneralSettings["theme"];
                const newGeneral = { ...generalSettings, theme: themeValue };
                setGeneralSettings(newGeneral);
                setTheme(themeValue);
                syncSettingsToBackend(newGeneral, shortcutSettings);
                onSettingsChange?.(newGeneral, shortcutSettings);
                logger.info("Theme changed via Settings:", themeValue);
              }}
              className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm transition-all border"
              style={{
                backgroundColor:
                  generalSettings.theme === option.value
                    ? isDark
                      ? "rgba(59,130,246,0.15)"
                      : "rgba(59,130,246,0.08)"
                    : isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(255,255,255,0.6)",
                borderColor:
                  generalSettings.theme === option.value
                    ? isDark
                      ? "rgba(59,130,246,0.3)"
                      : "rgba(59,130,246,0.3)"
                    : isDark
                      ? "rgba(255,255,255,0.1)"
                      : "rgba(0,0,0,0.06)",
                color:
                  generalSettings.theme === option.value
                    ? "#3b82f6"
                    : isDark
                      ? "#94a3b8"
                      : "#6b6b69",
              }}
            >
              {option.icon}
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="order-5 space-y-3 rounded-xl border p-4 lg:col-span-2" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.captureFilter")}</p>
        <label
          className="text-sm font-medium transition-colors duration-300"
          style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
        >
          {t("settings.general.excludedTextPatterns")}
        </label>
        <textarea
          value={generalSettings.excludedTextPatterns.join("\n")}
          onChange={(e) => {
            const excludedTextPatterns = e.target.value
              .split(/\r?\n/)
              .map((pattern) => pattern.trim())
              .filter(Boolean);
            const newGeneral = { ...generalSettings, excludedTextPatterns };
            setGeneralSettings(newGeneral);
            syncSettingsToBackend(newGeneral, shortcutSettings);
            onSettingsChange?.(newGeneral, shortcutSettings);
          }}
          className="w-full px-4 py-2.5 rounded-xl text-sm outline-none transition-all"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.7)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
            color: isDark ? "#e2e8f0" : "#4a4a48",
          }}
          rows={4}
          placeholder={"(?i)^password:\n(?s)^.{0,1}$"}
        />
        <p
          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          className="text-xs"
        >
          {t("settings.general.excludedTextPatternsHint")}
        </p>
      </div>

      <div className="order-6 space-y-3 rounded-xl border p-4 lg:col-span-2" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.clipboardCards")}</p>
        <div>
          <label className="text-sm font-medium" style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}>
            Display on clipboard cards
          </label>
          <p className="mt-1 text-xs" style={{ color: isDark ? "#64748b" : "#9a9a98" }}>
            Blue controls are shown on items; muted controls stay hidden.
          </p>
        </div>
        <div className="rounded-xl border p-2.5" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.6)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
          <div className="flex h-10 items-center gap-1.5 rounded-lg border px-2" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.04)" : "rgba(255,255,255,0.8)", borderColor: isDark ? "rgba(255,255,255,0.07)" : "rgba(0,0,0,0.05)" }}>
            {generalSettings.showItemIndex && <span className="rounded border border-indigo-500/45 bg-indigo-500/10 px-1 py-0.5 text-[10px] font-semibold text-blue-500">#12</span>}
            <span className="min-w-0 flex-1 truncate text-[11px]" style={{ color: isDark ? "#cbd5e1" : "#52525b" }}>A clipboard item preview</span>
            {generalSettings.showLineCount && <span className="rounded-full border border-indigo-500/45 bg-indigo-500/10 px-1.5 py-0.5 text-[9px] font-medium text-blue-500">3 lines</span>}
            {generalSettings.showSourceHost && <span className="max-w-16 truncate rounded-full border border-indigo-500/45 bg-indigo-500/10 px-1.5 py-0.5 text-[9px] font-medium text-blue-500">⌘ MacBook</span>}
          </div>
          <div className="mt-2 flex gap-2">
            {([
              ["showItemIndex", "Item number", <Hash size={14} key="icon" />],
              ["showLineCount", "Line count", <List size={14} key="icon" />],
              ["showSourceHost", "Source host", <Monitor size={14} key="icon" />],
            ] as const).map(([key, label, icon]) => {
              const enabled = generalSettings[key];
              return <button key={key} type="button" title={label} aria-label={label} aria-pressed={enabled} onClick={() => { const newGeneral = { ...generalSettings, [key]: !enabled }; setGeneralSettings(newGeneral); syncSettingsToBackend(newGeneral, shortcutSettings); onSettingsChange?.(newGeneral, shortcutSettings); }} className="flex h-9 flex-1 items-center justify-center rounded-lg border transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-500" style={{ color: enabled ? "#3b82f6" : isDark ? "#64748b" : "#a1a1aa", backgroundColor: enabled ? (isDark ? "rgba(59,130,246,0.18)" : "rgba(59,130,246,0.1)") : "transparent", borderColor: enabled ? "rgba(59,130,246,0.38)" : (isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)") }}>{icon}</button>;
            })}
          </div>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium" style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}>Action buttons</span>
          <span className="text-[10px]" style={{ color: isDark ? "#64748b" : "#9a9a98" }}>Configure each button</span>
        </div>
        <div className="grid grid-cols-3 gap-2">
          {([
            ["showEditButton", "Edit", <Pencil size={14} key="icon" />],
            ["showPinButton", "Pin", <Pin size={14} key="icon" />],
          ] as const).map(([key, label, icon]) => {
            const enabled = generalSettings[key];
            return <button key={key} type="button" aria-pressed={enabled} onClick={() => { const newGeneral = { ...generalSettings, [key]: !enabled }; setGeneralSettings(newGeneral); syncSettingsToBackend(newGeneral, shortcutSettings); onSettingsChange?.(newGeneral, shortcutSettings); }} className="flex h-11 flex-col items-center justify-center rounded-lg border text-[10px] font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-500" style={{ color: enabled ? "#3b82f6" : isDark ? "#64748b" : "#a1a1aa", backgroundColor: enabled ? (isDark ? "rgba(59,130,246,0.18)" : "rgba(59,130,246,0.1)") : "transparent", borderColor: enabled ? "rgba(59,130,246,0.38)" : (isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)") }}>{icon}<span className="mt-0.5">{label}</span></button>;
          })}
          {pluginActionExtensions.map((extension) => {
            const enabled = generalSettings.pluginActionVisibility[extension.id] !== false;
            const label = extension.pluginName || extension.component;
            const icon = extension.iconDataUrl ? <img src={extension.iconDataUrl} alt="" aria-hidden="true" className="h-3.5 w-3.5 object-contain" /> : extension.icon === "list-todo" ? <ListTodo size={14} /> : extension.pluginId.includes("translate") ? <Languages size={14} /> : <Puzzle size={14} />;
            return <button key={extension.id} type="button" title={label} aria-label={label} aria-pressed={enabled} onClick={() => { const newGeneral = { ...generalSettings, pluginActionVisibility: { ...generalSettings.pluginActionVisibility, [extension.id]: !enabled } }; setGeneralSettings(newGeneral); syncSettingsToBackend(newGeneral, shortcutSettings); onSettingsChange?.(newGeneral, shortcutSettings); }} className="flex h-11 min-w-0 flex-col items-center justify-center rounded-lg border px-1 text-[10px] font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-500" style={{ color: enabled ? "#3b82f6" : isDark ? "#64748b" : "#a1a1aa", backgroundColor: enabled ? (isDark ? "rgba(59,130,246,0.18)" : "rgba(59,130,246,0.1)") : "transparent", borderColor: enabled ? "rgba(59,130,246,0.38)" : (isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)") }}>{icon}<span className="mt-0.5 max-w-full truncate">{label}</span></button>;
          })}
        </div>
      </div>

      {/* Language Selector */}
      <div className="order-2 space-y-3 rounded-xl border p-4" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.language")}</p>
        <label
          className="text-sm font-medium transition-colors duration-300"
          style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
        >
          {t("settings.language.label")}
        </label>
        <div className="flex gap-2">
          {[
            { value: "en", label: t("settings.language.en") },
            { value: "zh", label: t("settings.language.zh") },
          ].map((option) => (
            <button
              key={option.value}
              onClick={() => {
                i18n.changeLanguage(option.value);
              }}
              className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm transition-all border"
              style={{
                backgroundColor:
                  i18n.language === option.value
                    ? isDark
                      ? "rgba(59,130,246,0.15)"
                      : "rgba(59,130,246,0.08)"
                    : isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(255,255,255,0.6)",
                borderColor:
                  i18n.language === option.value
                    ? isDark
                      ? "rgba(59,130,246,0.3)"
                      : "rgba(59,130,246,0.3)"
                    : isDark
                      ? "rgba(255,255,255,0.1)"
                      : "rgba(0,0,0,0.06)",
                color:
                  i18n.language === option.value
                    ? "#3b82f6"
                    : isDark
                      ? "#94a3b8"
                      : "#6b6b69",
              }}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="order-4 space-y-3 rounded-xl border p-4" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.historyLimits")}</p>
        <label
          className="text-sm font-medium transition-colors duration-300"
          style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
        >
          {t("settings.general.maxTextItems")}
        </label>
        <input
          type="number"
          value={generalSettings.maxItems}
          onChange={(e) => {
            const newValue = parseInt(e.target.value) || 1000;
            const newGeneral = { ...generalSettings, maxItems: newValue };
            setGeneralSettings(newGeneral);
            syncSettingsToBackend(newGeneral, shortcutSettings);
            onSettingsChange?.(newGeneral, shortcutSettings);
          }}
          className="w-full px-4 py-2.5 rounded-xl text-sm outline-none transition-all"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.7)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
            color: isDark ? "#e2e8f0" : "#4a4a48",
          }}
          min={100}
          max={10000}
        />
        <p
          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          className="text-xs"
        >
          {t("settings.general.limitHint")}
        </p>
      </div>

      <div className="order-4 space-y-3 rounded-xl border p-4" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <label
          className="text-sm font-medium transition-colors duration-300"
          style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
        >
          {t("settings.general.maxImageItems")}
        </label>
        <input
          type="number"
          value={generalSettings.maxImages}
          onChange={(e) => {
            const newValue = parseInt(e.target.value) || 500;
            const newGeneral = { ...generalSettings, maxImages: newValue };
            setGeneralSettings(newGeneral);
            syncSettingsToBackend(newGeneral, shortcutSettings);
            onSettingsChange?.(newGeneral, shortcutSettings);
          }}
          className="w-full px-4 py-2.5 rounded-xl text-sm outline-none transition-all"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.7)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
            color: isDark ? "#e2e8f0" : "#4a4a48",
          }}
          min={50}
          max={5000}
        />
      </div>

      <div className="order-3 space-y-3 rounded-xl border p-4" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.cardDensity")}</p>
        <label
          className="text-sm font-medium transition-colors duration-300"
          style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
        >
          {t("settings.general.cardSize")}
        </label>
        <div className="flex gap-2">
          {[
            {
              value: "small",
              label: t("settings.general.cardSizeOptions.small"),
            },
            {
              value: "medium",
              label: t("settings.general.cardSizeOptions.medium"),
            },
            {
              value: "large",
              label: t("settings.general.cardSizeOptions.large"),
            },
          ].map((option) => (
            <button
              key={option.value}
              data-testid={`settings-line-height-${option.value}`}
              onClick={async () => {
                const newHeight = option.value as GeneralSettings["lineHeight"];
                const newGeneral = {
                  ...generalSettings,
                  lineHeight: newHeight,
                };
                logger.info("[Settings] Card size button clicked:", newHeight);
                logger.info(
                  "[Settings] Calling setGeneralSettings + syncSettingsToBackend + onSettingsChange",
                );
                setGeneralSettings(newGeneral);

                // Use withTrace for user action
                await withTrace("Settings", "update_card_size", async () => {
                  syncSettingsToBackend(newGeneral, shortcutSettings);
                });

                onSettingsChange?.(newGeneral, shortcutSettings);
              }}
              className="flex-1 px-4 py-2.5 rounded-xl text-sm transition-all border"
              style={{
                backgroundColor:
                  generalSettings.lineHeight === option.value
                    ? isDark
                      ? "rgba(59,130,246,0.15)"
                      : "rgba(59,130,246,0.08)"
                    : isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(255,255,255,0.6)",
                borderColor:
                  generalSettings.lineHeight === option.value
                    ? isDark
                      ? "rgba(59,130,246,0.3)"
                      : "rgba(59,130,246,0.3)"
                    : isDark
                      ? "rgba(255,255,255,0.1)"
                      : "rgba(0,0,0,0.06)",
                color:
                  generalSettings.lineHeight === option.value
                    ? "#3b82f6"
                    : isDark
                      ? "#94a3b8"
                      : "#6b6b69",
              }}
            >
              {option.label}
            </button>
          ))}
        </div>
        <p
          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          className="text-xs"
        >
          {t("settings.general.cardSizeHint")}
        </p>
      </div>

      <div className="order-7 space-y-4 rounded-xl border p-4 lg:col-span-2" style={{ backgroundColor: isDark ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.55)", borderColor: isDark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.06)" }}>
        <p className="text-xs font-semibold uppercase tracking-wide" style={{ color: isDark ? "#94a3b8" : "#71717a" }}>{t("settings.general.groups.windowBehavior")}</p>
        <div
          className="flex items-center justify-between p-4 rounded-xl"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
          }}
        >
          <div>
            <span
              className="text-sm font-medium transition-colors duration-300"
              style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
            >
              {t("settings.general.autoStart")}
            </span>
            <p
              className="text-xs mt-0.5 transition-colors duration-300"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {t("settings.general.autoStartDesc")}
            </p>
          </div>
          <button
            onClick={() => {
              const newAutoStart = !generalSettings.autoStart;
              const newGeneral = {
                ...generalSettings,
                autoStart: newAutoStart,
              };
              setGeneralSettings(newGeneral);
              syncSettingsToBackend(newGeneral, shortcutSettings);
              onSettingsChange?.(newGeneral, shortcutSettings);
            }}
            className="w-11 h-6 rounded-full transition-all"
            style={{
              backgroundColor: generalSettings.autoStart
                ? "#3b82f6"
                : isDark
                  ? "#475569"
                  : "#c4c4c2",
            }}
          >
            <div
              className="w-4 h-4 bg-white rounded-full shadow transition-transform"
              style={{
                transform: generalSettings.autoStart
                  ? "translateX(22px)"
                  : "translateX(4px)",
              }}
            />
          </button>
        </div>

        <div
          className="flex items-center justify-between p-4 rounded-xl"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border:
              "1px solid " +
              (isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"),
          }}
        >
          <div>
            <span
              className="text-sm font-medium transition-colors duration-300"
              style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
            >
              {t("settings.general.autoHide")}
            </span>
            <p
              className="text-xs mt-0.5 transition-colors duration-300"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {t("settings.general.autoHideDesc")}
            </p>
          </div>
          <button
            onClick={() => {
              const newAutoHide = !generalSettings.autoHide;
              const newGeneral = { ...generalSettings, autoHide: newAutoHide };
              setGeneralSettings(newGeneral);
              syncSettingsToBackend(newGeneral, shortcutSettings);
              onSettingsChange?.(newGeneral, shortcutSettings);
            }}
            className="w-11 h-6 rounded-full transition-all"
            style={{
              backgroundColor: generalSettings.autoHide
                ? "#3b82f6"
                : isDark
                  ? "#475569"
                  : "#c4c4c2",
            }}
          >
            <div
              className="w-4 h-4 bg-white rounded-full shadow transition-transform"
              style={{
                transform: generalSettings.autoHide
                  ? "translateX(22px)"
                  : "translateX(4px)",
              }}
            />
          </button>
        </div>
      </div>
    </div>
  );

  const renderShortcutsSettings = () => (
    <div className="space-y-3">
      <p
        style={{ color: isDark ? "#64748b" : "#9a9a98" }}
        className="text-xs mb-4"
      >
        {t("settings.shortcuts.instruction")}
      </p>

      {(Object.keys(shortcutInfo) as Array<keyof ShortcutSettings>).map(
        (key) => {
          const info = shortcutInfo[key];
          const hasConflict = conflictInfo?.key === key;
          return (
            <div
              key={key}
              className="flex items-center justify-between p-4 rounded-xl"
              style={{
                backgroundColor: isDark
                  ? "rgba(255,255,255,0.05)"
                  : "rgba(255,255,255,0.6)",
                border: `1px solid ${
                  hasConflict
                    ? "rgba(239,68,68,0.5)"
                    : isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(0,0,0,0.04)"
                }`,
                transition: "all 0.2s ease",
              }}
            >
              <div>
                <span
                  className="text-sm font-medium"
                  style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
                >
                  {info.label}
                </span>
                <p
                  className="text-xs mt-0.5"
                  style={{ color: isDark ? "#64748b" : "#9a9a98" }}
                >
                  {info.description}
                  {info.global && (
                    <span style={{ color: "#3b82f6" }}>
                      ({t("settings.shortcuts.global")})
                    </span>
                  )}
                </p>
              </div>
              <div className="flex flex-col items-end gap-1.5">
                <button
                  className="min-w-[120px] px-4 py-2 rounded-lg text-xs font-mono transition-all border"
                  style={{
                    backgroundColor: hasConflict
                      ? "rgba(239,68,68,0.1)"
                      : editingShortcut === key
                        ? isDark
                          ? "rgba(59,130,246,0.2)"
                          : "rgba(59,130,246,0.08)"
                        : isDark
                          ? "rgba(255,255,255,0.05)"
                          : "rgba(255,255,255,0.5)",
                    borderColor: hasConflict
                      ? "rgba(239,68,68,0.6)"
                      : editingShortcut === key
                        ? isDark
                          ? "rgba(59,130,246,0.5)"
                          : "rgba(59,130,246,0.3)"
                        : isDark
                          ? "rgba(255,255,255,0.1)"
                          : "rgba(0,0,0,0.06)",
                    color: hasConflict
                      ? "#ef4444"
                      : editingShortcut === key
                        ? "#3b82f6"
                        : isDark
                          ? "#94a3b8"
                          : "#6b6b69",
                    animation: hasConflict ? "shake 0.4s ease-in-out" : "none",
                  }}
                  onClick={() => handleStartEditing(key)}
                  onKeyDown={(e) =>
                    editingShortcut === key && handleShortcutCapture(key, e)
                  }
                  tabIndex={0}
                >
                  {editingShortcut === key && !hasConflict
                    ? t("settings.shortcuts.pressKeys")
                    : shortcutSettings[key]}
                </button>
                {hasConflict && (
                  <div
                    className="flex items-center gap-1 text-xs"
                    style={{ color: "#ef4444" }}
                  >
                    <AlertCircle size={12} />
                    <span>{conflictInfo.message}</span>
                  </div>
                )}
              </div>
            </div>
          );
        },
      )}

      <div
        className="mt-6 pt-4"
        style={{
          borderTop: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
        }}
      >
        <p
          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          className="text-xs mb-3"
        >
          {t("settings.shortcuts.pluginShortcuts")}
        </p>
        <div
          className="flex items-center justify-between p-4 rounded-xl opacity-50"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
          }}
        >
          <div>
            <span
              className="text-sm font-medium"
              style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
            >
              {t("settings.shortcuts.ocrExtract")}
            </span>
            <p
              className="text-xs mt-0.5"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {t("settings.shortcuts.ocrDesc")}
            </p>
          </div>
          <span
            className="px-4 py-2 rounded-lg text-xs font-mono"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.5)",
              color: isDark ? "#64748b" : "#9a9a98",
              border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
            }}
          >
            {t("settings.shortcuts.notSet")}
          </span>
        </div>
        <div
          className="flex items-center justify-between p-4 rounded-xl opacity-50 mt-2"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
          }}
        >
          <div>
            <span
              className="text-sm font-medium"
              style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
            >
              {t("settings.shortcuts.semanticSearch")}
            </span>
            <p
              className="text-xs mt-0.5"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {t("settings.shortcuts.semanticDesc")}
            </p>
          </div>
          <span
            className="px-4 py-2 rounded-lg text-xs font-mono"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.5)",
              color: isDark ? "#64748b" : "#9a9a98",
              border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
            }}
          >
            {t("settings.shortcuts.notSet")}
          </span>
        </div>
      </div>
    </div>
  );

  const renderPluginsSettings = () => <PluginsTab isDark={isDark} />;
  const renderCloudSyncSettings = () => <CloudSyncTab isDark={isDark} />;

  const renderAboutSettings = () => (
    <div className="space-y-6">
      <div className="text-center py-6">
        <img
          src="/icon.png"
          alt=""
          className="w-20 h-20 mx-auto mb-5 rounded-2xl shadow-lg shadow-blue-500/20"
          draggable={false}
        />
        <h2
          className="text-xl font-bold"
          style={{ color: isDark ? "#e2e8f0" : "#4a4a48" }}
        >
          Cliporax
        </h2>
        <p
          className="text-sm mt-1"
          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
        >
          {t("settings.about.version", { version: __APP_VERSION__ })}
        </p>
      </div>

      <div
        className="flex items-center justify-between p-4 rounded-xl"
        style={{
          backgroundColor: isDark
            ? "rgba(255,255,255,0.05)"
            : "rgba(255,255,255,0.6)",
          border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
        }}
      >
        <div>
          <span
            className="text-sm font-medium"
            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
          >
            {t("settings.about.checkUpdate")}
          </span>
          <p
            className="text-xs mt-0.5"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            {t("settings.about.lastChecked")}
          </p>
        </div>
        <button
          type="button"
          onClick={() => handleOpenExternal(GITHUB_LATEST_RELEASE_URL)}
          className="flex items-center gap-2 px-4 py-2 text-xs font-medium bg-blue-500/15 hover:bg-blue-500/25 text-blue-400 border border-blue-500/20 rounded-lg transition-all"
        >
          <Download size={14} />
          {t("settings.about.checkNow")}
        </button>
      </div>

      <div className="space-y-2">
        <button
          type="button"
          onClick={() => handleOpenExternal(GITHUB_CURRENT_RELEASE_URL)}
          className="flex items-center justify-between p-4 rounded-xl transition-all group"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
            width: "100%",
          }}
        >
          <span
            className="text-sm"
            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
          >
            {t("settings.about.changelog")}
          </span>
          <span
            className="text-xs transition-colors"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            →
          </span>
        </button>
        <button
          type="button"
          onClick={() => handleOpenExternal(GITHUB_DOCS_URL)}
          className="flex items-center justify-between p-4 rounded-xl transition-all group"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
            width: "100%",
          }}
        >
          <span
            className="text-sm"
            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
          >
            {t("settings.about.documentation")}
          </span>
          <span
            className="text-xs transition-colors"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            →
          </span>
        </button>
        <button
          type="button"
          onClick={() => handleOpenExternal(GITHUB_ISSUES_URL)}
          className="flex items-center justify-between p-4 rounded-xl transition-all group"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.6)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
            width: "100%",
          }}
        >
          <span
            className="text-sm"
            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
          >
            {t("settings.about.reportIssue")}
          </span>
          <span
            className="text-xs transition-colors"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            →
          </span>
        </button>
      </div>

      <div
        className="text-center pt-6"
        style={{
          borderTop: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
        }}
      >
        <p
          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          className="text-xs"
        >
          {t("settings.about.copyright")}
        </p>
        <p
          style={{ color: isDark ? "#475569" : "#b4b4b2" }}
          className="text-xs mt-1"
        >
          {t("settings.about.license")}
        </p>
      </div>

      {/* Developer Test Section */}
      <div
        className="mt-6 pt-4"
        style={{
          borderTop: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
        }}
      >
        <div className="flex items-center gap-2 mb-3">
          <TestTube
            size={14}
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          />
          <span
            className="text-xs font-medium"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            {t("settings.about.devTest")}
          </span>
        </div>

        <div className="flex gap-2">
          <button
            onClick={async () => {
              if (isLoading) return;
              setIsLoading(true);
              setTestResult(null);
              try {
                const startTime = Date.now();
                const count = await testApi.insertBatch(10000);
                const elapsed = Date.now() - startTime;
                setTestResult({
                  type: "success",
                  message: t("settings.about.insertSuccess", {
                    count,
                    elapsed,
                  }),
                });
              } catch (error) {
                setTestResult({
                  type: "error",
                  message: t("settings.about.insertError", {
                    error: String(error),
                  }),
                });
              } finally {
                setIsLoading(false);
              }
            }}
            disabled={isLoading}
            className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 text-xs font-medium rounded-lg transition-all border"
            style={{
              backgroundColor: isDark
                ? "rgba(59,130,246,0.1)"
                : "rgba(59,130,246,0.08)",
              borderColor: isDark
                ? "rgba(59,130,246,0.2)"
                : "rgba(59,130,246,0.2)",
              color: "#3b82f6",
              opacity: isLoading ? 0.6 : 1,
            }}
          >
            {isLoading ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <TestTube size={14} />
            )}
            {isLoading
              ? t("settings.about.inserting")
              : t("settings.about.insertTestData")}
          </button>

          <button
            onClick={async () => {
              if (isLoading) return;
              setIsLoading(true);
              setTestResult(null);
              try {
                await testApi.clearAll();
                setTestResult({
                  type: "success",
                  message: t("settings.about.clearSuccess"),
                });
              } catch (error) {
                setTestResult({
                  type: "error",
                  message: t("settings.about.clearError", {
                    error: String(error),
                  }),
                });
              } finally {
                setIsLoading(false);
              }
            }}
            disabled={isLoading}
            className="flex items-center justify-center gap-2 px-4 py-2.5 text-xs font-medium rounded-lg transition-all border"
            style={{
              backgroundColor: isDark
                ? "rgba(239,68,68,0.1)"
                : "rgba(239,68,68,0.08)",
              borderColor: isDark
                ? "rgba(239,68,68,0.2)"
                : "rgba(239,68,68,0.2)",
              color: "#ef4444",
              opacity: isLoading ? 0.6 : 1,
            }}
          >
            <Trash2 size={14} />
            {t("settings.about.clear")}
          </button>
        </div>

        {testResult && (
          <div
            className="mt-2 px-3 py-2 rounded-lg text-xs"
            style={{
              backgroundColor:
                testResult.type === "success"
                  ? isDark
                    ? "rgba(34,197,94,0.1)"
                    : "rgba(34,197,94,0.08)"
                  : isDark
                    ? "rgba(239,68,68,0.1)"
                    : "rgba(239,68,68,0.08)",
              color: testResult.type === "success" ? "#22c55e" : "#ef4444",
            }}
          >
            {testResult.message}
          </div>
        )}
      </div>
    </div>
  );

  return (
    <div
      data-testid="settings-panel"
      className={
        isWindow
          ? "w-full h-full flex flex-col overflow-hidden"
          : "fixed inset-0 z-50 flex items-center justify-center backdrop-blur-xl"
      }
      style={
        isWindow
          ? {
              backgroundColor: isDark
                ? "rgba(15,23,42,0.98)"
                : "rgba(252,251,249,0.98)",
            }
          : {
              backgroundColor: isDark ? "rgba(0,0,0,0.4)" : "rgba(0,0,0,0.2)",
            }
      }
      onClick={
        isWindow
          ? undefined
          : (e) => {
              if (e.target === e.currentTarget) {
                onClose?.();
              }
            }
      }
    >
      {isWindow && <ResizeHandles />}
      {/* Title Bar - window mode only */}
      {isWindow && (
        <div
          onMouseDown={handleDragStart}
          className="h-9 flex items-center justify-between select-none backdrop-blur-md transition-colors duration-300 flex-shrink-0 drag-region"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.5)",
            borderBottom: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
          }}
        >
          <div className="flex items-center px-4 gap-3">
            <div className="w-4 h-4 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center shadow-lg shadow-blue-500/20">
              <span className="text-[8px] font-bold text-white">C</span>
            </div>
            <span
              className="text-xs font-medium tracking-wide transition-colors duration-300"
              style={{ color: isDark ? "#94a3b8" : "#8a8a88" }}
            >
              Cliporax Settings
            </span>
          </div>
          <div className="flex items-center h-full pr-2 no-drag">
            <button
              onClick={async () => {
                try {
                  await windowApi.close();
                } catch (error) {
                  logger.error("Failed to close settings window:", error);
                }
              }}
              className="h-7 w-7 mx-0.5 rounded-lg transition-all flex items-center justify-center"
              style={{ color: isDark ? "#94a3b8" : "#71717a" }}
              onMouseEnter={(e) => {
                e.currentTarget.style.backgroundColor = "rgba(239,68,68,0.2)";
                e.currentTarget.style.color = "#ef4444";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = "transparent";
                e.currentTarget.style.color = isDark ? "#94a3b8" : "#71717a";
              }}
            >
              <X size={13} />
            </button>
          </div>
        </div>
      )}

      <div
        className={
          isWindow
            ? "flex-1 flex overflow-hidden"
            : "w-[640px] h-[520px] rounded-2xl shadow-2xl flex overflow-hidden backdrop-blur-xl"
        }
        style={
          isWindow
            ? {
                backgroundColor: isDark
                  ? "rgba(15,23,42,0.98)"
                  : "rgba(252,251,249,0.98)",
              }
            : {
                backgroundColor: isDark
                  ? "rgba(15,23,42,0.95)"
                  : "rgba(252,251,249,0.95)",
                border:
                  "1px solid " +
                  (isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"),
                boxShadow: isDark
                  ? "0 25px 50px -12px rgba(0,0,0,0.5)"
                  : "0 25px 50px -12px rgba(0,0,0,0.08)",
              }
        }
      >
        {/* Sidebar */}
        <div
          className="w-44 border-r py-5 transition-colors duration-300 flex-shrink-0 overflow-hidden"
          style={{
            backgroundColor: isDark
              ? "rgba(0,0,0,0.2)"
              : "rgba(248,246,243,0.8)",
            borderColor: isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)",
          }}
        >
          <div className="px-4 mb-5">
            <h2
              className="text-sm font-semibold transition-colors duration-300"
              style={{ color: isDark ? "#e2e8f0" : "#4a4a48" }}
            >
              {t("settings.title")}
            </h2>
            <p
              className="text-[10px] mt-0.5 transition-colors duration-300"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {t("settings.subtitle")}
            </p>
          </div>
          <nav className="space-y-0.5 px-2">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className="w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-sm transition-colors"
                style={{
                  backgroundColor:
                    activeTab === tab.id
                      ? isDark
                        ? "rgba(59,130,246,0.15)"
                        : "rgba(59,130,246,0.08)"
                      : "transparent",
                  color:
                    activeTab === tab.id
                      ? "#3b82f6"
                      : isDark
                        ? "#94a3b8"
                        : "#8a8a88",
                  border:
                    "1px solid " +
                    (activeTab === tab.id
                      ? isDark
                        ? "rgba(59,130,246,0.2)"
                        : "rgba(59,130,246,0.25)"
                      : "transparent"),
                }}
              >
                <span
                  style={{
                    color:
                      activeTab === tab.id
                        ? "#3b82f6"
                        : isDark
                          ? "#64748b"
                          : "#a1a1aa",
                  }}
                >
                  {tab.icon}
                </span>
                {tab.label}
              </button>
            ))}
          </nav>
        </div>

        {/* Content */}
        <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
          <div
            className="flex items-center justify-between px-6 py-5 border-b transition-colors duration-300 flex-shrink-0"
            style={{
              borderColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(0,0,0,0.04)",
            }}
          >
            <div>
              <h3
                className="text-sm font-medium capitalize transition-colors duration-300"
                style={{ color: isDark ? "#e2e8f0" : "#4a4a48" }}
              >
                {t("settings.tabs." + activeTab)}
              </h3>
              <p
                className="text-[10px] mt-0.5 transition-colors duration-300"
                style={{ color: isDark ? "#64748b" : "#9a9a98" }}
              >
                {t("settings.tabDescriptions." + activeTab)}
              </p>
            </div>
            {!isWindow && (
              <button
                onClick={() => onClose?.()}
                className="p-2 rounded-xl transition-all"
                onMouseEnter={(e) =>
                  (e.currentTarget.style.backgroundColor = isDark
                    ? "rgba(255,255,255,0.1)"
                    : "rgba(0,0,0,0.04)")
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.backgroundColor = "transparent")
                }
              >
                <X
                  size={16}
                  style={{ color: isDark ? "#94a3b8" : "#8a8a88" }}
                />
              </button>
            )}
          </div>

          <div className="flex-1 overflow-y-auto overflow-x-hidden p-6 settings-scroll-area">
            {activeTab === "general" && renderGeneralSettings()}
            {activeTab === "shortcuts" && renderShortcutsSettings()}
            {activeTab === "plugins" && renderPluginsSettings()}
            {activeTab === "sync" && renderCloudSyncSettings()}
            {activeTab === "about" && renderAboutSettings()}
          </div>
        </div>
      </div>
    </div>
  );
};

export default Settings;
