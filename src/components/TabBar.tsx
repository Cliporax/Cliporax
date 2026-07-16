import React, {
  useState,
  useCallback,
  useRef,
  useEffect,
  useLayoutEffect,
} from "react";
import { PanelLeftClose, Plus, X, Edit2, Trash2 } from "lucide-react";
import { useTabStore } from "../stores/tabStore";
import { useUIStore } from "../stores/uiStore";
import { createLogger } from "../utils/logger";
import { useToast } from "./Toast";
import { useConfirm } from "./ConfirmDialog";

const logger = createLogger("TabBar");
const CONTEXT_MENU_PADDING = 8;
const CONTEXT_MENU_MIN_WIDTH = 128;
const CONTEXT_MENU_FALLBACK_HEIGHT = 36;
const SIDEBAR_HIDE_THRESHOLD = 52;
const MAX_SIDEBAR_WIDTH = 384;

interface ContextMenuPosition {
  x: number;
  y: number;
}

interface TabPointerDrag {
  pointerId: number;
  tabId: number;
  startX: number;
  startY: number;
  active: boolean;
  target: { tabId: number; after: boolean } | null;
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

interface ClipboardTabSidebarProps {
  width: number;
  onWidthChange: (width: number) => void;
  onCollapse: () => void;
}

export function ClipboardTabSidebar({
  width,
  onWidthChange,
  onCollapse,
}: ClipboardTabSidebarProps) {
  const {
    tabs,
    activeTabId,
    isLoading,
    isReordering,
    createTab,
    reorderTabs,
    deleteTab,
    renameTab,
    setActiveTab,
  } = useTabStore();
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
  const [draggedTabId, setDraggedTabId] = useState<number | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    tabId: number;
    after: boolean;
  } | null>(null);
  const pointerDragRef = useRef<TabPointerDrag | null>(null);
  const suppressTabClickRef = useRef(false);
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => () => resizeCleanupRef.current?.(), []);

  const handleResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      const startX = event.clientX;
      const startWidth = width;
      const maximumWidth = Math.max(
        SIDEBAR_HIDE_THRESHOLD,
        Math.min(MAX_SIDEBAR_WIDTH, window.innerWidth - 240),
      );

      const handlePointerMove = (moveEvent: PointerEvent) => {
        const nextWidth = startWidth + moveEvent.clientX - startX;
        if (nextWidth <= SIDEBAR_HIDE_THRESHOLD) {
          cleanup();
          onCollapse();
          return;
        }
        onWidthChange(clamp(nextWidth, SIDEBAR_HIDE_THRESHOLD, maximumWidth));
      };
      const cleanup = () => {
        window.removeEventListener("pointermove", handlePointerMove);
        window.removeEventListener("pointerup", cleanup);
        resizeCleanupRef.current = null;
      };

      resizeCleanupRef.current?.();
      resizeCleanupRef.current = cleanup;
      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", cleanup, { once: true });
    },
    [onWidthChange, width],
  );

  const closeContextMenu = useCallback(() => {
    setContextMenu(null);
  }, []);

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
      if (suppressTabClickRef.current) return;
      closeContextMenu();
      if (tabId === activeTabId) return;

      setActiveTab(tabId);
      setSearchQuery(""); // Clear search on tab switch
      logger.info("Tab switched to:", tabId);
    },
    [activeTabId, closeContextMenu, setActiveTab, setSearchQuery],
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

  const clearTabDrag = useCallback(() => {
    pointerDragRef.current = null;
    setDraggedTabId(null);
    setDropTarget(null);
  }, []);

  const persistTabDrop = useCallback(
    async (draggedId: number, target: { tabId: number; after: boolean }) => {
      const reorderedIds = tabs
        .map((tab) => tab.id)
        .filter((id): id is number => id !== null && id !== draggedId);
      const targetIndex = reorderedIds.indexOf(target.tabId);
      if (targetIndex < 0) return;
      reorderedIds.splice(
        targetIndex + (target.after ? 1 : 0),
        0,
        draggedId,
      );

      try {
        await reorderTabs(reorderedIds);
      } catch (error) {
        logger.error("Failed to reorder tabs:", error);
        toast.error("Failed to save tab order. Please try again.");
      }
    },
    [reorderTabs, tabs, toast],
  );

  const handleTabPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>, tabId: number) => {
      if (
        event.button !== 0 ||
        isReordering ||
        renamingTabId !== null ||
        (event.target as HTMLElement).closest("button, input")
      ) {
        return;
      }
      pointerDragRef.current = {
        pointerId: event.pointerId,
        tabId,
        startX: event.clientX,
        startY: event.clientY,
        active: false,
        target: null,
      };
      event.currentTarget.setPointerCapture?.(event.pointerId);
      closeContextMenu();
    },
    [closeContextMenu, isReordering, renamingTabId],
  );

  const handleTabPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = pointerDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;

      if (!drag.active) {
        const distance = Math.hypot(
          event.clientX - drag.startX,
          event.clientY - drag.startY,
        );
        if (distance < 5) return;
        drag.active = true;
        setDraggedTabId(drag.tabId);
      }

      event.preventDefault();
      const targetElement = Array.from(
        document.querySelectorAll<HTMLElement>("[data-native-tab-id]"),
      ).find((element) => {
        const bounds = element.getBoundingClientRect();
        const withinX =
          event.clientX >= bounds.left &&
          event.clientX <= bounds.left + bounds.width;
        const withinY =
          bounds.height === 0 ||
          (event.clientY >= bounds.top &&
            event.clientY <= bounds.top + bounds.height);
        return withinX && withinY;
      });
      const targetId = Number(targetElement?.dataset.nativeTabId);
      if (!targetElement || !Number.isSafeInteger(targetId) || targetId === drag.tabId) {
        drag.target = null;
        setDropTarget(null);
        return;
      }
      const bounds = targetElement.getBoundingClientRect();
      const nextTarget = {
        tabId: targetId,
        after: event.clientY >= bounds.top + bounds.height / 2,
      };
      drag.target = nextTarget;
      setDropTarget(nextTarget);
    },
    [],
  );

  const handleTabPointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = pointerDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }

      if (drag.active) {
        suppressTabClickRef.current = true;
        window.setTimeout(() => {
          suppressTabClickRef.current = false;
        }, 0);
      }
      const target = drag.target;
      const draggedId = drag.tabId;
      clearTabDrag();
      if (event.type === "pointerup" && drag.active && target) {
        void persistTabDrop(draggedId, target);
      }
    },
    [clearTabDrag, persistTabDrop],
  );

  if (isLoading && tabs.length === 0) {
    return (
      <div
        className="flex h-full shrink-0 items-center justify-center border-r border-gray-200 dark:border-gray-700"
        style={{ width }}
      >
        <div className="text-xs text-gray-500 dark:text-gray-400">
          Loading tabs...
        </div>
      </div>
    );
  }

  return (
    <div
      data-testid="clipboard-tab-sidebar"
      className="relative flex h-full shrink-0 flex-col border-r border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800"
      style={{ width }}
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
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-gray-200 px-2 dark:border-gray-700">
        <span className="text-[11px] font-semibold text-gray-600 dark:text-gray-300">Collections</span>
        <button
          type="button"
          onClick={onCollapse}
          className="flex size-7 items-center justify-center rounded-md text-gray-500 transition-colors hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
          aria-label="Hide clipboard collections"
          title="Hide clipboard collections"
        >
          <PanelLeftClose size={15} aria-hidden="true" />
        </button>
      </div>

      {/* Tab List */}
      <div
        className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto p-2"
        role="tablist"
        aria-label="Clipboard collections"
      >
        {tabs.map((tab) => (
          <div
            key={tab.id}
            role="tab"
            data-native-tab-id={tab.id}
            aria-selected={tab.id === activeTabId}
            onPointerDown={(event) => handleTabPointerDown(event, tab.id!)}
            onPointerMove={handleTabPointerMove}
            onPointerUp={handleTabPointerEnd}
            onPointerCancel={handleTabPointerEnd}
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
              group relative flex min-h-8 w-full touch-none items-center px-2.5 py-1 rounded-md cursor-grab active:cursor-grabbing transition-colors
              text-xs font-medium select-none
              ${draggedTabId === tab.id ? "opacity-50" : ""}
              ${
                tab.id === activeTabId
                  ? "bg-indigo-500 dark:bg-indigo-600 text-white"
                  : "text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
              }
            `}
          >
            {dropTarget?.tabId === tab.id ? (
              <span
                data-testid="tab-drop-indicator"
                data-position={dropTarget.after ? "after" : "before"}
                aria-hidden="true"
                className={`pointer-events-none absolute right-0.5 left-0.5 z-10 h-0.5 rounded-full bg-indigo-500 shadow-[0_0_4px_rgba(99,102,241,0.8)] ${
                  dropTarget.after ? "-bottom-0.5" : "-top-0.5"
                }`}
              />
            ) : null}
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
            className="flex min-h-8 w-full items-center justify-center rounded-md p-1 text-gray-500 transition-colors hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
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
      <div
        data-testid="clipboard-sidebar-resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize clipboard collections"
        tabIndex={0}
        onPointerDown={handleResizeStart}
        onKeyDown={(event) => {
          if (event.key === "ArrowRight") {
            event.preventDefault();
            onWidthChange(clamp(width + 16, SIDEBAR_HIDE_THRESHOLD, MAX_SIDEBAR_WIDTH));
          } else if (event.key === "ArrowLeft") {
            event.preventDefault();
            if (width - 16 <= SIDEBAR_HIDE_THRESHOLD) {
              onCollapse();
            } else {
              onWidthChange(clamp(width - 16, SIDEBAR_HIDE_THRESHOLD, MAX_SIDEBAR_WIDTH));
            }
          }
        }}
        className="absolute top-0 right-0 z-20 h-full w-1 cursor-ew-resize touch-none"
      />
    </div>
  );
}

// Kept as an alias while callers migrate to the clearer component name.
export const TabBar = (props: Partial<ClipboardTabSidebarProps>) => (
  <ClipboardTabSidebar
    width={props.width ?? 176}
    onWidthChange={props.onWidthChange ?? (() => {})}
    onCollapse={props.onCollapse ?? (() => {})}
  />
);
