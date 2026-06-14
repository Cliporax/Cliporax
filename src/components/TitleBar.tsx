import React, { useCallback, useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  Minus,
  Square,
  X,
  Maximize2,
  Pin,
  PinOff,
  Sun,
  Moon,
  Settings,
  ClipboardList,
} from "lucide-react";
import { createLogger } from "../utils/logger";
import {
  CLIPBOARD_COUNT_CHANGED_EVENT,
  clipboard,
  events,
  window as windowApi,
} from "../lib/tauri-api";
import { useTheme } from "../contexts/ThemeContext";
import { useTabStore } from "../stores/tabStore";

const logger = createLogger("TitleBar");

const TitleBar: React.FC = () => {
  const { t } = useTranslation();
  const { resolvedTheme, toggleTheme } = useTheme();
  const { activeTabId } = useTabStore();
  const [isMaximized, setIsMaximized] = useState(false);
  const [isPinned, setIsPinned] = useState(false);
  const [totalCount, setTotalCount] = useState<number>(0);
  const countRequestIdRef = React.useRef(0);
  const isDark = resolvedTheme === "dark";

  const loadTotalCount = useCallback(async () => {
    const requestId = countRequestIdRef.current + 1;
    countRequestIdRef.current = requestId;

    if (activeTabId === null) {
      setTotalCount(0);
      return;
    }

    try {
      const count = await clipboard.getTotalCount(activeTabId);
      if (countRequestIdRef.current === requestId) {
        setTotalCount(count);
      }
    } catch (error) {
      logger.error("Failed to load total count:", error);
    }
  }, [activeTabId]);

  // Load total count of clipboard items
  useEffect(() => {
    loadTotalCount();

    // Refresh count every 10 seconds
    const interval = setInterval(loadTotalCount, 10000);
    return () => clearInterval(interval);
  }, [loadTotalCount]);

  useEffect(() => {
    let unlistenClipboardChanged: (() => void) | undefined;
    let disposed = false;

    const handleCountChanged = (event: Event) => {
      const detail = (event as CustomEvent<{ tabId?: number | null }>).detail;
      if (detail?.tabId && detail.tabId !== activeTabId) {
        return;
      }
      loadTotalCount();
    };

    globalThis.addEventListener(
      CLIPBOARD_COUNT_CHANGED_EVENT,
      handleCountChanged,
    );

    events
      .onClipboardChanged(loadTotalCount)
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlistenClipboardChanged = unlisten;
      })
      .catch((error) => {
        logger.error("Failed to register clipboard count listener:", error);
      });

    return () => {
      disposed = true;
      globalThis.removeEventListener(
        CLIPBOARD_COUNT_CHANGED_EVENT,
        handleCountChanged,
      );
      unlistenClipboardChanged?.();
    };
  }, [activeTabId, loadTotalCount]);

  useEffect(() => {
    const checkWindowState = async () => {
      try {
        const maximized = await windowApi.isMaximized();
        setIsMaximized(maximized);
      } catch (error) {
        logger.error("Failed to check maximized state:", error);
      }
    };
    checkWindowState();
  }, []);

  // Handle mouse up globally to end dragging
  // Only call endDragging if we actually started dragging
  const isDraggingRef = React.useRef(false);

  useEffect(() => {
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
  }, []);

  const handleMinimize = async () => {
    logger.debug("Minimizing window");
    await windowApi.minimize();
  };

  const handleMaximize = async () => {
    logger.debug("Toggling maximize");
    await windowApi.maximize();
    setIsMaximized(!isMaximized);
  };

  const handleClose = async () => {
    logger.debug("Closing window");
    await windowApi.close();
  };

  const handleTogglePin = async () => {
    const newPinned = !isPinned;
    logger.info("Toggling window pin:", newPinned);
    try {
      await windowApi.setAlwaysOnTop(newPinned);
      setIsPinned(newPinned);
      // Set global variable for ClipboardList to check
      (globalThis as any).__clipboardXPinned = newPinned;
      logger.info("Window pin toggled successfully:", newPinned);
    } catch (error) {
      logger.error("Failed to toggle window pin:", error);
    }
  };

  const handleDragStart = async (e: React.MouseEvent) => {
    // Only start drag on left click and not on buttons
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;

    logger.debug("Starting window drag");
    isDraggingRef.current = true;
    try {
      await windowApi.startDragging();
    } catch (error) {
      isDraggingRef.current = false;
      logger.error("Failed to start dragging:", error);
    }
  };

  return (
    <div
      onMouseDown={handleDragStart}
      className="h-8 flex items-center justify-between select-none backdrop-blur-md transition-colors duration-300 drag-region"
      style={{
        backgroundColor: isDark
          ? "rgba(255,255,255,0.05)"
          : "rgba(255,255,255,0.5)",
        borderBottom: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
      }}
    >
      <div className="flex items-center px-3 gap-2">
        <img
          src="/icon.png"
          alt=""
          className="w-3.5 h-3.5 rounded-md shadow-lg shadow-blue-500/20"
          draggable={false}
        />
        <span
          className="text-xs font-medium tracking-wide transition-colors duration-300"
          style={{ color: isDark ? "#94a3b8" : "#8a8a88" }}
        >
          Cliporax
        </span>
      </div>

      <div className="flex items-center h-full pr-1.5 no-drag">
        {/* Status Info */}
        <div
          className="flex items-center gap-1.5 px-2 mr-1 text-[10px]"
          style={{
            color: isDark ? "#64748b" : "#8a8a88",
          }}
        >
          <ClipboardList size={10} />
          <span>{totalCount}</span>
        </div>

        <div
          className="w-px h-3 mx-1"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.1)",
          }}
        />

        {/* Settings Button */}
        <button
          onClick={async () => {
            logger.debug("Opening settings window");
            try {
              await windowApi.openSettings();
            } catch (error) {
              logger.error("Failed to open settings window:", error);
            }
          }}
          className="h-6 w-6 mx-0.5 rounded-md transition-all flex items-center justify-center"
          style={{
            color: isDark ? "#94a3b8" : "#8a8a88",
          }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.backgroundColor = isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.04)")
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.backgroundColor = "transparent")
          }
          title={t("settings.title", "Settings")}
        >
          <Settings size={12} />
        </button>

        <div
          className="w-px h-3 mx-1"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.1)",
          }}
        />

        {/* Theme Toggle */}
        <button
          onClick={() => {
            console.log(
              "[TitleBar] Theme toggle clicked, current isDark:",
              isDark,
            );
            toggleTheme();
          }}
          className="h-6 w-6 mx-0.5 rounded-md transition-all flex items-center justify-center"
          style={{
            color: isDark ? "#94a3b8" : "#8a8a88",
          }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.backgroundColor = isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.04)")
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.backgroundColor = "transparent")
          }
          title={t("titleBar.toggleTheme", {
            mode: isDark ? t("titleBar.lightMode") : t("titleBar.darkMode"),
          })}
        >
          {isDark ? <Sun size={12} /> : <Moon size={12} />}
        </button>

        <div
          className="w-px h-3 mx-1"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.1)",
          }}
        />

        <button
          onClick={handleTogglePin}
          className="h-6 w-6 mx-0.5 rounded-md transition-all flex items-center justify-center"
          style={{
            backgroundColor: isPinned
              ? isDark
                ? "rgba(59,130,246,0.2)"
                : "rgba(59,130,246,0.15)"
              : "transparent",
            color: isPinned ? "#3b82f6" : isDark ? "#94a3b8" : "#8a8a88",
          }}
          onMouseEnter={(e) => {
            if (!isPinned)
              e.currentTarget.style.backgroundColor = isDark
                ? "rgba(255,255,255,0.1)"
                : "rgba(0,0,0,0.04)";
          }}
          onMouseLeave={(e) => {
            if (!isPinned)
              e.currentTarget.style.backgroundColor = "transparent";
          }}
          title={isPinned ? t("titleBar.unpinWindow") : t("titleBar.pinWindow")}
        >
          {isPinned ? <PinOff size={12} /> : <Pin size={12} />}
        </button>

        <div
          className="w-px h-3 mx-1"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.1)",
          }}
        />

        <button
          onClick={handleMinimize}
          className="h-6 w-6 mx-0.5 rounded-md transition-all flex items-center justify-center"
          style={{ color: isDark ? "#94a3b8" : "#71717a" }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.backgroundColor = isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.05)")
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.backgroundColor = "transparent")
          }
          title={t("titleBar.minimize")}
        >
          <Minus size={12} />
        </button>

        <button
          onClick={handleMaximize}
          className="h-6 w-6 mx-0.5 rounded-md transition-all flex items-center justify-center"
          style={{ color: isDark ? "#94a3b8" : "#71717a" }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.backgroundColor = isDark
              ? "rgba(255,255,255,0.1)"
              : "rgba(0,0,0,0.05)")
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.backgroundColor = "transparent")
          }
          title={isMaximized ? t("titleBar.restore") : t("titleBar.maximize")}
        >
          {isMaximized ? <Maximize2 size={10} /> : <Square size={10} />}
        </button>

        <button
          onClick={handleClose}
          className="h-6 w-6 mx-0.5 rounded-md transition-all flex items-center justify-center"
          style={{ color: isDark ? "#94a3b8" : "#71717a" }}
          onMouseEnter={(e) => {
            e.currentTarget.style.backgroundColor = "rgba(239,68,68,0.2)";
            e.currentTarget.style.color = "#ef4444";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.backgroundColor = "transparent";
            e.currentTarget.style.color = isDark ? "#94a3b8" : "#71717a";
          }}
          title={t("titleBar.close")}
        >
          <X size={12} />
        </button>
      </div>
    </div>
  );
};

export default TitleBar;
