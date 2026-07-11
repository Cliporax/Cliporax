import React, {
  useState,
  useCallback,
  useRef,
  useEffect,
  useLayoutEffect,
} from "react";
import { Plus, X, Edit2, ListTodo, Puzzle, Trash2 } from "lucide-react";
import { useTabStore } from "../stores/tabStore";
import { useUIStore } from "../stores/uiStore";
import { createLogger } from "../utils/logger";
import { useToast } from "./Toast";
import { useConfirm } from "./ConfirmDialog";
import { useContentTabExtensions } from "../plugin/extensions";

const logger = createLogger("TabBar");
const CONTEXT_MENU_PADDING = 8;
const CONTEXT_MENU_MIN_WIDTH = 128;
const CONTEXT_MENU_FALLBACK_HEIGHT = 36;
const OPEN_FILE_SYNC_EVENT = "cliporax:open-file-sync";
const FILE_SYNC_TAB_ID = "plugin:com.cliporax.file-sync:FileSyncView";

interface ContextMenuPosition {
  x: number;
  y: number;
}

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), Math.max(min, max));

const getViewportSize = () => ({
  width: window.innerWidth || document.documentElement.clientWidth,
  height: window.innerHeight || document.documentElement.clientHeight,
});

const clampContextMenuPosition = (
  x: number,
  y: number,
  width = CONTEXT_MENU_MIN_WIDTH,
  height = CONTEXT_MENU_FALLBACK_HEIGHT,
): ContextMenuPosition => {
  const viewport = getViewportSize();

  return {
    x: clamp(
      x,
      CONTEXT_MENU_PADDING,
      viewport.width - width - CONTEXT_MENU_PADDING,
    ),
    y: clamp(
      y,
      CONTEXT_MENU_PADDING,
      viewport.height - height - CONTEXT_MENU_PADDING,
    ),
  };
};

function renderPluginTabIcon(icon: string | undefined, iconDataUrl?: string) {
  if (iconDataUrl) {
    return (
      <img
        src={iconDataUrl}
        alt=""
        aria-hidden="true"
        className="mr-1.5 h-3.5 w-3.5 shrink-0 object-contain"
      />
    );
  }
  if (icon === "list-todo") {
    return <ListTodo size={14} className="mr-1.5 shrink-0" />;
  }
  if (icon && icon.length <= 2) {
    return <span className="mr-1.5 shrink-0 text-xs leading-none">{icon}</span>;
  }
  return <Puzzle size={14} className="mr-1.5 shrink-0" />;
}

export function TabBar() {
  const {
    tabs,
    activeTabId,
    activePluginTabId,
    isLoading,
    loadTabs,
    createTab,
    deleteTab,
    renameTab,
    setActiveTab,
    setActivePluginTab,
  } = useTabStore();
  const pluginTabs = useContentTabExtensions();
  const { setSearchQuery } = useUIStore();
  const toast = useToast();
  const { confirm: askConfirm } = useConfirm();
  const [isCreating, setIsCreating] = useState(false);
  const [newTabName, setNewTabName] = useState("");
  const [inputKey, setInputKey] = useState(0); // Force re-render of input
  const [renamingTabId, setRenamingTabId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    tabId: number;
  } | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (
      activePluginTabId &&
      !pluginTabs.some((tab) => tab.id === activePluginTabId)
    ) {
      setActivePluginTab(null);
    }
  }, [activePluginTabId, pluginTabs, setActivePluginTab]);

  const closeContextMenu = useCallback(() => {
    setContextMenu(null);
  }, []);

  useEffect(() => {
    const openFileSync = () => {
      if (!pluginTabs.some((tab) => tab.id === FILE_SYNC_TAB_ID)) return;
      closeContextMenu();
      setActivePluginTab(FILE_SYNC_TAB_ID);
      setSearchQuery("");
    };

    window.addEventListener(OPEN_FILE_SYNC_EVENT, openFileSync);
    return () => window.removeEventListener(OPEN_FILE_SYNC_EVENT, openFileSync);
  }, [closeContextMenu, pluginTabs, setActivePluginTab, setSearchQuery]);

  const isInsideContextMenu = useCallback((target: EventTarget | null) => {
    return Boolean(
      target instanceof Node && contextMenuRef.current?.contains(target),
    );
  }, []);

  // Auto-focus rename input
  useEffect(() => {
    if (renamingTabId && renameInputRef.current) {
      renameInputRef.current.focus();
      renameInputRef.current.select();
    }
  }, [renamingTabId]);

  // Close context menu on outside click
  useEffect(() => {
    if (!contextMenu) return;

    const handleClickOutside = (e: Event) => {
      if (!isInsideContextMenu(e.target)) {
        closeContextMenu();
      }
    };

    const handleViewportChange = () => {
      closeContextMenu();
    };

    document.addEventListener("pointerdown", handleClickOutside, true);
    document.addEventListener("mousedown", handleClickOutside, true);
    document.addEventListener("click", handleClickOutside, true);
    window.addEventListener("scroll", handleViewportChange, true);
    window.addEventListener("resize", handleViewportChange);

    return () => {
      document.removeEventListener("pointerdown", handleClickOutside, true);
      document.removeEventListener("mousedown", handleClickOutside, true);
      document.removeEventListener("click", handleClickOutside, true);
      window.removeEventListener("scroll", handleViewportChange, true);
      window.removeEventListener("resize", handleViewportChange);
    };
  }, [closeContextMenu, contextMenu, isInsideContextMenu]);

  // Close context menu on escape
  useEffect(() => {
    if (!contextMenu) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeContextMenu();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [closeContextMenu, contextMenu]);

  useLayoutEffect(() => {
    if (!contextMenu || !contextMenuRef.current) return;

    const rect = contextMenuRef.current.getBoundingClientRect();
    const nextPosition = clampContextMenuPosition(
      contextMenu.x,
      contextMenu.y,
      rect.width,
      rect.height,
    );

    if (
      Math.abs(nextPosition.x - contextMenu.x) > 0.5 ||
      Math.abs(nextPosition.y - contextMenu.y) > 0.5
    ) {
      setContextMenu({ ...contextMenu, ...nextPosition });
    }
  }, [contextMenu]);

  const handleTabClick = useCallback(
    (tabId: number) => {
      closeContextMenu();
      // If a plugin tab is active, always allow switching to a native tab.
      // Without the activePluginTabId check, clicking the same native tab
      // that was active before the plugin tab was selected would return
      // early (activeTabId hasn't been cleared), leaving the plugin tab active.
      if (tabId === activeTabId && !activePluginTabId) return;

      setActiveTab(tabId);
      setSearchQuery(""); // Clear search on tab switch
      logger.info("Tab switched to:", tabId);
    },
    [activeTabId, activePluginTabId, closeContextMenu, setActiveTab, setSearchQuery],
  );

  const handleCreateTab = useCallback(async () => {
    if (!newTabName.trim()) return;

    try {
      await createTab(newTabName.trim());
      setNewTabName("");
      setInputKey((k) => k + 1);
      setIsCreating(false);
      logger.info("Tab created:", newTabName);
    } catch (error) {
      logger.error("Failed to create tab:", error);
    }
  }, [newTabName, createTab]);

  const handleDeleteTab = useCallback(
    async (e: React.MouseEvent, tabId: number) => {
      e.stopPropagation();

      const tab = tabs.find((t) => t.id === tabId);
      if (tab?.is_default || tab?.is_trash) {
        logger.warn("Cannot delete default tab");
        return;
      }

      // Show confirmation dialog
      const confirmed = await askConfirm({
        title: "Delete Tab",
        message: `Are you sure you want to delete "${tab?.name}"? This will also delete all clipboard items in this tab. This action cannot be undone.`,
        confirmText: "Delete",
        cancelText: "Cancel",
      });

      if (!confirmed) {
        logger.info("Delete tab cancelled");
        return;
      }

      try {
        await deleteTab(tabId);
        logger.info("Tab deleted:", tabId);
        toast.success(`Tab "${tab?.name}" and its items have been deleted`);
      } catch (error) {
        logger.error("Failed to delete tab:", error);
        toast.error("Failed to delete tab. Please try again.");
      }
    },
    [tabs, deleteTab, askConfirm, toast],
  );

  const handleInputKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        handleCreateTab();
      } else if (e.key === "Escape") {
        setNewTabName("");
        setInputKey((k) => k + 1);
        setIsCreating(false);
      }
    },
    [handleCreateTab],
  );

  const handleStartRename = useCallback(
    (tabId: number, currentName: string) => {
      setRenamingTabId(tabId);
      setRenameValue(currentName);
    },
    [],
  );

  const handleSaveRename = useCallback(async () => {
    if (!renamingTabId || !renameValue.trim()) return;

    try {
      await renameTab(renamingTabId, renameValue.trim());
      setRenamingTabId(null);
      setRenameValue("");
      logger.info("Tab renamed:", renamingTabId);
    } catch (error) {
      logger.error("Failed to rename tab:", error);
      // Show error message to user
      toast.error(error instanceof Error ? error.message : String(error));
      // Reset to original name
      setRenamingTabId(null);
      setRenameValue("");
    }
  }, [renamingTabId, renameValue, renameTab]);

  const handleRenameKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        handleSaveRename();
      } else if (e.key === "Escape") {
        setRenamingTabId(null);
        setRenameValue("");
      }
    },
    [handleSaveRename],
  );

  // Get display name for tab (show "Default" for reserved tabs)
  const getTabDisplayName = useCallback((tab: (typeof tabs)[0]) => {
    const reservedNames = ["Default", "System Clipboard"];
    if (reservedNames.includes(tab.name) || tab.is_default) {
      return "Default";
    }
    return tab.name;
  }, []);

  const handleTabContextMenu = useCallback(
    (e: React.MouseEvent, tabId: number) => {
      e.preventDefault();
      e.stopPropagation();

      // Check if this is a default tab
      const tab = tabs.find((t) => t.id === tabId);
      if (tab?.is_default || tab?.is_trash) {
        logger.warn("Cannot rename protected tab");
        closeContextMenu();
        return;
      }

      setContextMenu({
        ...clampContextMenuPosition(e.clientX, e.clientY),
        tabId,
      });
    },
    [closeContextMenu, tabs],
  );

  const handleContextMenuRename = useCallback(() => {
    if (!contextMenu) return;
    const tab = tabs.find((t) => t.id === contextMenu.tabId);
    if (tab) {
      handleStartRename(contextMenu.tabId, tab.name);
    }
    closeContextMenu();
  }, [closeContextMenu, contextMenu, tabs, handleStartRename]);

  if (isLoading && tabs.length === 0) {
    return (
      <div className="flex items-center justify-center h-8 px-4 border-t border-gray-200 dark:border-gray-700">
        <div className="text-xs text-gray-500 dark:text-gray-400">
          Loading tabs...
        </div>
      </div>
    );
  }

  return (
    <div
      className="flex items-center h-8 px-2 border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800"
      onPointerDownCapture={(e) => {
        if (!isInsideContextMenu(e.target)) {
          closeContextMenu();
        }
      }}
      onMouseDownCapture={(e) => {
        if (!isInsideContextMenu(e.target)) {
          closeContextMenu();
        }
      }}
    >
      {/* Tab List */}
      <div className="flex-1 flex items-center space-x-1 overflow-x-auto">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            onClick={() => handleTabClick(tab.id!)}
            onDoubleClick={(e) => {
              e.stopPropagation();
              // Don't allow rename for default tabs
              if (!tab.is_default && !tab.is_trash) {
                handleStartRename(tab.id!, tab.name);
              }
            }}
            onContextMenu={(e) => handleTabContextMenu(e, tab.id!)}
            className={`
              group flex items-center px-2.5 py-1 rounded-md cursor-pointer transition-colors
              text-xs font-medium select-none
              ${
                tab.id === activeTabId && activePluginTabId === null
                  ? "bg-indigo-500 dark:bg-indigo-600 text-white"
                  : "text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
              }
            `}
          >
            {renamingTabId === tab.id ? (
              <input
                ref={renameInputRef}
                type="text"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  handleRenameKeyDown(e);
                }}
                onClick={(e) => e.stopPropagation()}
                className="px-1.5 py-0.5 text-xs border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-indigo-500 w-24"
                autoFocus
              />
            ) : (
              <span className="flex items-center truncate max-w-28">
                {tab.is_trash ? (
                  <Trash2 size={12} className="mr-1 shrink-0" />
                ) : null}
                {getTabDisplayName(tab)}
              </span>
            )}

            {/* Delete button (not for default tab) */}
            {!tab.is_default && !tab.is_trash && renamingTabId !== tab.id && (
              <button
                onClick={(e) => handleDeleteTab(e, tab.id!)}
                className={`
                  ml-1.5 p-0.5 rounded transition-colors
                  ${
                    tab.id === activeTabId
                      ? "hover:bg-indigo-600 dark:hover:bg-indigo-700"
                      : "hover:bg-gray-200 dark:hover:bg-gray-600"
                  }
                `}
                aria-label={`Delete tab ${tab.name}`}
              >
                <X size={10} />
              </button>
            )}
          </div>
        ))}

        {pluginTabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => {
              closeContextMenu();
              setActivePluginTab(tab.id);
              setSearchQuery("");
            }}
            className={`
              flex items-center px-2.5 py-1 rounded-md cursor-pointer transition-colors
              text-xs font-medium select-none whitespace-nowrap
              ${
                tab.id === activePluginTabId
                  ? "bg-indigo-500 dark:bg-indigo-600 text-white"
                  : "text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
              }
            `}
          >
            {renderPluginTabIcon(tab.icon, tab.iconDataUrl)}
            <span className="truncate">{tab.title}</span>
          </button>
        ))}

        {/* Add Tab Button */}
        {isCreating ? (
          <div className="flex items-center">
            <input
              key={inputKey}
              type="text"
              value={newTabName}
              onChange={(e) => setNewTabName(e.target.value)}
              onKeyDown={handleInputKeyDown}
              placeholder="Tab name..."
              className="px-2 py-0.5 text-xs border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-indigo-500 w-28"
              autoFocus
            />
          </div>
        ) : (
          <button
            onClick={() => setIsCreating(true)}
            className="flex items-center justify-center p-1 rounded-md text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
            aria-label="Add new tab"
          >
            <Plus size={14} />
          </button>
        )}
      </div>

      {/* Tab Context Menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="fixed z-50 min-w-32 py-0.5 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700"
          style={{
            left: `${contextMenu.x}px`,
            top: `${contextMenu.y}px`,
            maxWidth: `calc(100vw - ${CONTEXT_MENU_PADDING * 2}px)`,
          }}
        >
          <button
            type="button"
            onClick={handleContextMenuRename}
            className="w-full flex items-center px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
          >
            <Edit2 size={12} className="mr-2" />
            <span className="truncate">Rename</span>
          </button>
        </div>
      )}
    </div>
  );
}
