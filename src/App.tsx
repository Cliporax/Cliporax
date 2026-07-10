import { useEffect, useState, useRef, useCallback } from "react";
import { Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import ClipboardList, { ClipboardListRef } from "./components/ClipboardList";
import TitleBar from "./components/TitleBar";
import SettingsModal, {
  loadSettings,
  GeneralSettings,
  ShortcutSettings,
} from "./components/Settings";
import SettingsWindow from "./components/SettingsWindow";
import PreviewWindow from "./components/PreviewWindow";
import ContentEditor from "./components/ContentEditor";
import { ToastProvider } from "./components/Toast";
import { ConfirmDialogProvider } from "./components/ConfirmDialog";
import { useTheme } from "./contexts/ThemeContext";
import {
  PluginProvider,
  ExtensionManagerProvider,
  PluginContentTab,
  PluginSidebarExtensions,
} from "./plugin";
import { createLogger } from "./utils/logger";
import { events, window as windowApi } from "./lib/tauri-api";
import { useUIStore } from "./stores/uiStore";
import { useTabStore } from "./stores/tabStore";
import { TabBar } from "./components/TabBar";
import { ResizeHandles } from "./components/ResizeHandles";
import { useSettingsSync } from "./hooks/useSettingsSync";
import { installLongTaskObserver, perfLog, perfMeasure } from "./utils/perf";

const logger = createLogger("App");

function App() {
  const appStartRef = useRef(performance.now());
  const { resolvedTheme } = useTheme();
  const { t } = useTranslation();

  // Check whether this is the settings window via URL pathname
  const isSettingsWindow = window.location.pathname === "/settings";
  const isPreviewWindow = window.location.pathname === "/preview";
  const [backendReady, setBackendReady] = useState(false);
  const [backendReadyError, setBackendReadyError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    invoke<boolean>("app_ready")
      .then(() => {
        if (!cancelled) {
          setBackendReady(true);
          setBackendReadyError(null);
        }
      })
      .catch((error) => {
        logger.error("[App] Backend readiness check failed:", error);
        if (!cancelled) {
          setBackendReadyError(String(error));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  // Initialize tabs on mount
  const { loadTabs, activeTabId, activePluginTabId } = useTabStore();
  
  useEffect(() => {
    if (backendReady && !isSettingsWindow && !isPreviewWindow) {
      loadTabs();
    }
  }, [backendReady, isPreviewWindow, isSettingsWindow, loadTabs]);

  // Get state from the store
  const {
    showSearch,
    searchQuery,
    searchMode,
    searchScope,
    isSettingsOpen,
    editingItem,
    isMultiSelectMode,
    selectedItemIds,
    setShowSearch,
    setSearchQuery,
    setSearchMode,
    setSearchScope,
    openSettings,
    closeSettings,
    openEditor,
    closeEditor,
    toggleSearch,
    exitMultiSelectMode,
  } = useUIStore();

  // Settings stay local to the window and are synchronized through useSettingsSync.
  const [generalSettings, setGeneralSettings] = useState<GeneralSettings>(
    () => loadSettings().general,
  );
  const [shortcutSettings, setShortcutSettings] = useState<ShortcutSettings>(
    () => loadSettings().shortcuts,
  );
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [listRefreshTrigger, setListRefreshTrigger] = useState(0);
  const clipboardListRef = useRef<ClipboardListRef>(null);

  const isDark = resolvedTheme === "dark";

  useEffect(() => {
    if (!backendReady || isSettingsWindow || isPreviewWindow) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;

    events
      .onSyncCompleted(async () => {
        if (disposed) return;
        await loadTabs();
        if (!disposed) {
          setListRefreshTrigger((prev) => prev + 1);
        }
      })
      .then((dispose) => {
        if (disposed) {
          dispose();
        } else {
          unlisten = dispose;
        }
      })
      .catch((error) => {
        logger.error("[App] Failed to listen for sync completion:", error);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [backendReady, isPreviewWindow, isSettingsWindow, loadTabs]);

  // Remote settings change callback - settings sync from the Settings window
  // Only update local state and do not sync to the backend again, or it would loop
  const handleRemoteSettingsChange = useCallback(
    (general: GeneralSettings, shortcuts: ShortcutSettings) => {
      logger.info(
        "[App] REMOTE settings changed, lineHeight:",
        general.lineHeight,
        "updating local state only",
      );
      setGeneralSettings(general);
      setShortcutSettings(shortcuts);
    },
    [],
  );

  // Listen for settings changes from other windows
  useSettingsSync({
    onRemoteSettingsChange: handleRemoteSettingsChange,
    enabled: !isPreviewWindow,
  });

  useEffect(() => {
    if (isPreviewWindow) return;

    logger.debug("App mounted");
    perfMeasure("App", "mounted", appStartRef.current, undefined, {
      minIntervalMs: 0,
      warnAtMs: 100,
    });

    const uninstallLongTaskObserver = installLongTaskObserver();
    const frameId = requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        perfMeasure("App", "first-stable-frame", appStartRef.current, undefined, {
          minIntervalMs: 0,
          warnAtMs: 250,
        });
      });
    });

    // macOS special handling: do not call show() immediately on mount to avoid conflicts with window initialization
    // Linux still needs show() to help focus because X11/Wayland window managers are unreliable
    const isMacOS = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
    
    if (!isMacOS) {
      // Request window focus on mount for Linux reliability
      // On some Linux window managers, the backend set_focus() may not work reliably
      windowApi.show().catch(() => {
        // Ignore errors - window might already be visible
      });
      perfLog("App", "window-show-requested", { platform: "linux-or-windows" }, {
        minIntervalMs: 0,
      });
      logger.debug("[App] Called window.show() for Linux focus workaround");
    } else {
      perfLog("App", "window-show-skipped", { platform: "macos" }, {
        minIntervalMs: 0,
      });
      logger.debug("[App] Skipping window.show() on macOS to avoid initialization conflicts");
    }

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        logger.info("Toggling search bar");
        toggleSearch();
      }
      if (e.key === "Escape") {
        if (showSearch) {
          logger.debug("Closing search bar");
          setShowSearch(false);
          setSearchQuery("");
        } else if (isSettingsOpen) {
          logger.debug("Closing settings");
          closeSettings();
        } else if (editingItem) {
          logger.debug("Closing editor");
          closeEditor();
        } else if (isMultiSelectMode) {
          logger.debug("Exiting multi-select mode");
          exitMultiSelectMode();
          if (clipboardListRef.current) {
            clipboardListRef.current.exitMultiSelectMode();
          }
        } else {
          // Nothing open - hide window
          logger.debug("Hiding window via Esc");
          windowApi.hide();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      cancelAnimationFrame(frameId);
      uninstallLongTaskObserver();
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [
    isPreviewWindow,
    showSearch,
    isSettingsOpen,
    editingItem,
    isMultiSelectMode,
    toggleSearch,
    setShowSearch,
    setSearchQuery,
    closeSettings,
    closeEditor,
    exitMultiSelectMode,
  ]);

  useEffect(() => {
    const handleSelectStart = (event: Event) => {
      const target = event.target as HTMLElement | null;
      if (
        target?.closest(
          'input, textarea, [contenteditable="true"], .allow-text-select',
        )
      ) {
        return;
      }

      event.preventDefault();
    };

    document.addEventListener("selectstart", handleSelectStart);
    return () => document.removeEventListener("selectstart", handleSelectStart);
  }, []);

  // Prevent auto-hide when right-click context menu is open
  useEffect(() => {
    const handleContextMenu = () => {
      windowApi.setContextMenuOpen(true);
    };
    const handleClick = () => {
      windowApi.setContextMenuOpen(false);
    };

    document.addEventListener("contextmenu", handleContextMenu);
    document.addEventListener("click", handleClick);
    return () => {
      document.removeEventListener("contextmenu", handleContextMenu);
      document.removeEventListener("click", handleClick);
    };
  }, []);

  // Native select/combobox popups can briefly move focus outside the Tauri
  // window on some platforms. Treat them like menus so auto-hide does not
  // close Cliporax while the popup is open.
  useEffect(() => {
    let releaseTimer: ReturnType<typeof setTimeout> | null = null;

    const isSelectTarget = (target: EventTarget | null) =>
      target instanceof Element && Boolean(target.closest("select"));

    const holdAutoHide = (event: Event) => {
      if (!isSelectTarget(event.target)) return;
      if (releaseTimer) {
        clearTimeout(releaseTimer);
        releaseTimer = null;
      }
      windowApi.setContextMenuOpen(true);
    };

    const releaseAutoHideSoon = (event: Event) => {
      if (!isSelectTarget(event.target)) return;
      if (releaseTimer) clearTimeout(releaseTimer);
      releaseTimer = setTimeout(() => {
        windowApi.setContextMenuOpen(false);
        releaseTimer = null;
      }, 500);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !isSelectTarget(event.target)) return;
      releaseAutoHideSoon(event);
    };

    document.addEventListener("pointerdown", holdAutoHide, true);
    document.addEventListener("focusin", holdAutoHide, true);
    document.addEventListener("change", releaseAutoHideSoon, true);
    document.addEventListener("focusout", releaseAutoHideSoon, true);
    document.addEventListener("keydown", handleKeyDown, true);

    return () => {
      if (releaseTimer) clearTimeout(releaseTimer);
      document.removeEventListener("pointerdown", holdAutoHide, true);
      document.removeEventListener("focusin", holdAutoHide, true);
      document.removeEventListener("change", releaseAutoHideSoon, true);
      document.removeEventListener("focusout", releaseAutoHideSoon, true);
      document.removeEventListener("keydown", handleKeyDown, true);
      windowApi.setContextMenuOpen(false);
    };
  }, []);

  useEffect(() => {
    if (showSearch && searchInputRef.current) {
      searchInputRef.current.focus();
    }
  }, [showSearch]);

  useEffect(() => {
    if (searchQuery.toLowerCase().startsWith("regx:")) {
      setSearchMode("regex");
    } else {
      setSearchMode("fuzzy");
    }
  }, [searchQuery, setSearchMode]);

  const handleSearchInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setSearchQuery(value);
    logger.debug("Search input:", value, "mode:", searchMode);
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && searchMode === "regex") {
      logger.info("Executing regex search:", searchQuery);
    }
  };

  // Multi-select callbacks
  const handleMultiSelectChange = useCallback(
    (ids: Set<number>, items: unknown[]) => {
      // Update selected state in the store
      const store = useUIStore.getState();
      if (ids.size > 0) {
        store.enterMultiSelectMode();
      }
    },
    [],
  );

  const handleMultiSelectCancel = useCallback(() => {
    exitMultiSelectMode();
    if (clipboardListRef.current) {
      clipboardListRef.current.exitMultiSelectMode();
    }
  }, [exitMultiSelectMode]);

  const handleMultiSelectDelete = useCallback(() => {
    if (clipboardListRef.current) {
      clipboardListRef.current.exitMultiSelectMode();
    }
    handleMultiSelectCancel();
    setListRefreshTrigger((prev) => prev + 1);
  }, [handleMultiSelectCancel]);

  const readinessContent = backendReadyError ? (
    <div className="flex h-screen w-screen items-center justify-center text-xs text-red-500">
      Backend initialization failed: {backendReadyError}
    </div>
  ) : (
    <div className="flex h-screen w-screen items-center justify-center text-xs opacity-70">
      Starting Cliporax...
    </div>
  );

  const content = !backendReady ? readinessContent : isPreviewWindow ? (
    <PreviewWindow />
  ) : isSettingsWindow ? (
    <SettingsWindow />
  ) : (
    <ExtensionManagerProvider>
      <div
        data-testid="app-shell"
        className="flex flex-col h-screen w-screen transition-colors duration-300"
        style={{
          border: isDark
            ? "1px solid rgba(255, 255, 255, 0.08)"
            : "1px solid rgba(0, 0, 0, 0.08)",
          boxShadow: isDark
            ? "0 8px 32px rgba(0, 0, 0, 0.4)"
            : "0 8px 32px rgba(0, 0, 0, 0.12)",
          overflow: "hidden",
        }}
      >
        <ResizeHandles />
        <div
          className="flex flex-col h-full w-full overflow-hidden"
          style={{
            background: isDark
              ? "linear-gradient(135deg, #0f172a 0%, #1e293b 50%, #0f172a 100%)"
              : "linear-gradient(135deg, #f8f6f3 0%, #f0ede8 50%, #f8f6f3 100%)",
            color: isDark ? "#e2e8f0" : "#4a4a48",
          }}
        >
        <TitleBar />

        {showSearch && (
          <div
            className="flex items-center px-3 h-10 border-b flex-shrink-0 backdrop-blur-xl transition-colors duration-300"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.6)",
              borderColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(0,0,0,0.04)",
            }}
          >
            <div className="relative flex items-center gap-2 flex-1 w-full max-w-2xl mx-auto">
              <div
                className="flex h-7 flex-shrink-0 overflow-hidden rounded-lg border p-0.5"
                style={{
                  backgroundColor: isDark
                    ? "rgba(255,255,255,0.05)"
                    : "rgba(255,255,255,0.75)",
                  borderColor: isDark
                    ? "rgba(255,255,255,0.1)"
                    : "rgba(0,0,0,0.06)",
                }}
              >
                {(["current", "global"] as const).map((scope) => {
                  const active = searchScope === scope;
                  return (
                    <button
                      key={scope}
                      type="button"
                      className="h-full rounded-md px-2 text-[10px] font-medium transition-colors"
                      style={{
                        color: active
                          ? isDark
                            ? "#e2e8f0"
                            : "#27272a"
                          : isDark
                            ? "#94a3b8"
                            : "#71717a",
                        backgroundColor: active
                          ? isDark
                            ? "rgba(59,130,246,0.28)"
                            : "rgba(59,130,246,0.14)"
                          : "transparent",
                      }}
                      onClick={() => setSearchScope(scope)}
                    >
                      {t(`app.searchScope.${scope}`)}
                    </button>
                  );
                })}
              </div>
              <div className="relative flex-1 min-w-0">
                <Search
                  className="absolute left-3 top-1/2 -translate-y-1/2 transition-colors duration-300"
                  size={14}
                  style={{ color: isDark ? "#94a3b8" : "#8a8a88" }}
                />
                <input
                  data-testid="search-input"
                  ref={searchInputRef}
                  type="text"
                  placeholder={t("app.searchPlaceholder")}
                  className="w-full border rounded-lg py-1.5 pl-9 pr-20 text-xs transition-all outline-none"
                  style={{
                    backgroundColor: isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(255,255,255,0.8)",
                    borderColor: isDark
                      ? "rgba(255,255,255,0.1)"
                      : "rgba(0,0,0,0.06)",
                    color: isDark ? "#e2e8f0" : "#4a4a48",
                  }}
                  value={searchQuery}
                  onChange={handleSearchInput}
                  onKeyDown={handleSearchKeyDown}
                />
                {searchQuery && (
                  <span
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-[9px] font-medium px-2 py-0.5 rounded-md border"
                    style={{
                      color: isDark ? "#94a3b8" : "#71717a",
                      backgroundColor: isDark
                        ? "rgba(255,255,255,0.1)"
                        : "rgba(0,0,0,0.05)",
                      borderColor: isDark
                        ? "rgba(255,255,255,0.05)"
                        : "rgba(0,0,0,0.05)",
                    }}
                  >
                    {searchMode === "regex" ? "REGEX" : "FUZZY"}
                  </span>
                )}
              </div>
            </div>
          </div>
        )}

        <main className="flex-1 overflow-hidden relative">
          <PluginSidebarExtensions theme={isDark ? "dark" : "light"} />
          {activePluginTabId ? (
            <PluginContentTab
              tabId={activePluginTabId}
              theme={isDark ? "dark" : "light"}
            />
          ) : (
            <ClipboardList
              ref={clipboardListRef}
              tabId={activeTabId}
              searchQuery={searchQuery}
              searchMode={searchMode}
              searchScope={searchScope}
              lineHeight={generalSettings.lineHeight}
              refreshTrigger={listRefreshTrigger}
              onEdit={(item) => {
                logger.info("Opening editor for item:", item.id);
                openEditor(item);
              }}
              onMultiSelectChange={handleMultiSelectChange}
            />
          )}
        </main>

        <TabBar />

        {isSettingsOpen && (
          <PluginProvider>
            <SettingsModal
              key="settings-modal"
              onClose={() => closeSettings()}
              initialGeneralSettings={generalSettings}
              initialShortcutSettings={shortcutSettings}
              onSettingsChange={(general, shortcuts) => {
                setGeneralSettings(general);
                setShortcutSettings(shortcuts);
              }}
            />
          </PluginProvider>
        )}

        {editingItem && (
          <ContentEditor
            key="content-editor"
            id={editingItem.id}
            content={editingItem.content}
            type={editingItem.type}
            onClose={() => closeEditor()}
            onSave={(newContent) => {
              logger.info(
                "Content saved, updating cache for item:",
                editingItem.id,
              );
              // Use incremental updates to avoid UI flicker
              if (clipboardListRef.current) {
                clipboardListRef.current.updateItemContent(
                  editingItem.id,
                  newContent,
                );
              }
            }}
          />
        )}
        </div>
      </div>
    </ExtensionManagerProvider>
  );

  return (
    <ToastProvider>
    <ConfirmDialogProvider>
      {content}
    </ConfirmDialogProvider>
    </ToastProvider>
  );
}

export default App;
