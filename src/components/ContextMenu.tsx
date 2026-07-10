import React, {
  useState,
  useCallback,
  useRef,
  useEffect,
  useLayoutEffect,
  useMemo,
} from "react";
import { FolderPlus, Copy, ChevronRight, ListTodo } from "lucide-react";
import { clipboard, ItemType, type ClipboardItem } from "../lib/tauri-api";
import { useTabStore } from "../stores/tabStore";
import { useClipboardStore } from "../stores/clipboardStore";
import {
  useExtensionManager,
  type PluginContextMenuItem,
  type PluginItemApi,
  type PluginTransferItem,
  type RegisteredExtension,
} from "../plugin/extensions";
import { createLogger } from "../utils/logger";

const logger = createLogger("ContextMenu");

// Custom event to coordinate which context menu is open —
// only one menu should be visible at a time.
const CONTEXT_MENU_OPENED = "cliporax:contextmenu-opened";
const CONTEXT_MENU_CLOSE_REQUESTED = "cliporax:contextmenu-close-requested";
const VIEWPORT_PADDING = 8;
const MENU_MIN_WIDTH = 160;
const MAIN_MENU_FALLBACK_HEIGHT = 64;
const SUBMENU_GAP = 4;
const SUBMENU_ITEM_HEIGHT = 28;
const SUBMENU_VERTICAL_PADDING = 4;

interface ContextMenuProps {
  item: ClipboardItem;
  itemId: number;
  currentTabId?: number | null;
  batchItemIds?: Set<number>;
  onBatchActionComplete?: () => void;
  children: React.ReactNode;
}

interface MenuPosition {
  x: number;
  y: number;
}

type SubMenuType = "move" | "copy";

interface SubMenuState {
  type: SubMenuType;
  side: "left" | "right";
  top: number;
}

interface RuntimeContextMenuEntry {
  extension: RegisteredExtension;
  item: PluginContextMenuItem;
}

const getViewportSize = () => ({
  width: window.innerWidth || document.documentElement.clientWidth,
  height: window.innerHeight || document.documentElement.clientHeight,
});

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), Math.max(min, max));

const clampMenuPosition = (
  x: number,
  y: number,
  width = MENU_MIN_WIDTH,
  height = MAIN_MENU_FALLBACK_HEIGHT,
): MenuPosition => {
  const viewport = getViewportSize();

  return {
    x: clamp(x, VIEWPORT_PADDING, viewport.width - width - VIEWPORT_PADDING),
    y: clamp(y, VIEWPORT_PADDING, viewport.height - height - VIEWPORT_PADDING),
  };
};

const getSubMenuLayout = (
  triggerRect: DOMRect,
  itemCount: number,
): Pick<SubMenuState, "side" | "top"> => {
  const viewport = getViewportSize();
  const estimatedHeight =
    Math.max(1, itemCount) * SUBMENU_ITEM_HEIGHT + SUBMENU_VERTICAL_PADDING * 2;
  const side =
    triggerRect.right + SUBMENU_GAP + MENU_MIN_WIDTH >
    viewport.width - VIEWPORT_PADDING
      ? "left"
      : "right";
  const overflowBottom =
    triggerRect.top + estimatedHeight - (viewport.height - VIEWPORT_PADDING);
  const top =
    overflowBottom > 0
      ? Math.max(-overflowBottom, VIEWPORT_PADDING - triggerRect.top)
      : 0;

  return { side, top };
};

function hasPermission(extension: RegisteredExtension, permission: string) {
  const granted = new Set(extension.grantedPermissions);
  return (
    granted.has(permission) ||
    granted.has(`${permission.split(":")[0]}:*`) ||
    granted.has("*")
  );
}

function toTransferItem(item: ClipboardItem): PluginTransferItem {
  return {
    id: item.id,
    type: item.type,
    content: item.content,
    source: "clipboard",
  };
}

function evaluateContextMenuCondition(
  condition: string | undefined,
  item: ClipboardItem | undefined,
) {
  if (!condition) return true;

  const normalized = condition.trim();
  const itemTypeMatch = normalized.match(/^item\.type\s*===\s*['"](\w+)['"]$/);
  if (itemTypeMatch) {
    return item?.type === itemTypeMatch[1];
  }

  logger.warn("Unsupported plugin context-menu condition:", condition);
  return false;
}

function renderPluginMenuIcon(icon: string | undefined) {
  if (!icon) return null;
  if (icon === "list-todo") {
    return <ListTodo size={14} className="mr-2 shrink-0" />;
  }
  if (icon.length <= 2) {
    return <span className="mr-2 shrink-0 text-xs leading-none">{icon}</span>;
  }
  return null;
}

export function ContextMenu({
  item,
  itemId,
  currentTabId,
  batchItemIds,
  onBatchActionComplete,
  children,
}: ContextMenuProps) {
  const [position, setPosition] = useState<MenuPosition | null>(null);
  const [subMenu, setSubMenu] = useState<SubMenuState | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const subMenuRef = useRef<HTMLDivElement>(null);
  // Unique ID for this menu instance, used to distinguish itself from others
  // when the global "menu opened" event fires.
  const menuInstanceId = useRef(
    `menu-${Math.random().toString(36).slice(2, 9)}`,
  ).current;

  const { tabs } = useTabStore();
  const { items, removeItem, updateItem } = useClipboardStore();
  const { getExtensions } = useExtensionManager();
  const availableTabs = tabs.filter((t) => t.id !== currentTabId);
  const contextMenuExtensions = getExtensions("context-menu");
  const batchIds = useMemo(
    () =>
      batchItemIds?.has(itemId) && batchItemIds.size > 1
        ? Array.from(batchItemIds)
        : [],
    [batchItemIds, itemId],
  );

  const selectedItems = useMemo(() => {
    const ids = batchIds.length > 1 ? new Set(batchIds) : new Set([itemId]);
    const storeItems = items.filter(
      (clipboardItem) =>
        clipboardItem.id !== null && ids.has(clipboardItem.id),
    );

    if (storeItems.length > 0) {
      return storeItems;
    }

    return item.id !== null && ids.has(item.id) ? [item] : [];
  }, [batchIds, item, itemId, items]);

  const pluginContextMenuEntries = useMemo<RuntimeContextMenuEntry[]>(() => {
    const theme = document.documentElement.classList.contains("dark")
      ? "dark"
      : "light";
    const primaryItem = selectedItems[0];

    return contextMenuExtensions.flatMap((extension) => {
      if (!hasPermission(extension, "ui:context-menu")) {
        return [];
      }

      if (!evaluateContextMenuCondition(extension.condition, primaryItem)) {
        return [];
      }

      const plugin = window.CliporaxPlugins?.[extension.pluginId];
      const component = plugin?.extensions?.[extension.component];
      if (!component?.getMenuItems) return [];

      const props = {
        data: {
          item: primaryItem
            ? {
                id: primaryItem.id,
                type: primaryItem.type,
                is_pinned: primaryItem.is_pinned,
                is_sensitive: primaryItem.is_sensitive,
              }
            : null,
          itemId,
          itemIds: selectedItems
            .map((item) => item.id)
            .filter((id): id is number => id !== null),
          items: selectedItems.map((item) => ({
            id: item.id,
            type: item.type,
            is_pinned: item.is_pinned,
            is_sensitive: item.is_sensitive,
          })),
        },
        context: {
          theme: theme as "light" | "dark",
          settings: extension.config,
          plugin: {
            id: extension.pluginId,
            name: extension.pluginName,
            version: extension.pluginVersion,
          },
        },
        config: extension.config,
      };

      if (component.shouldShow && !component.shouldShow(props)) return [];

      try {
        return component.getMenuItems(props).map((menuItem) => ({
          extension,
          item: menuItem,
        }));
      } catch (error) {
        logger.error("Failed to get plugin context menu items:", {
          pluginId: extension.pluginId,
          component: extension.component,
          error,
        });
        return [];
      }
    });
  }, [contextMenuExtensions, itemId, selectedItems]);

  const closeMenu = useCallback(() => {
    setPosition(null);
    setSubMenu(null);
  }, []);

  const isInsideOpenMenu = useCallback((target: EventTarget | null) => {
    if (!(target instanceof Node)) return false;

    return Boolean(
      menuRef.current?.contains(target) || subMenuRef.current?.contains(target),
    );
  }, []);

  // Close menu on outside click
  useEffect(() => {
    if (!position) return;

    const handleOutsidePointer = (e: Event) => {
      if (!isInsideOpenMenu(e.target)) {
        closeMenu();
      }
    };

    const handleViewportChange = () => {
      closeMenu();
    };

    document.addEventListener("pointerdown", handleOutsidePointer, true);
    document.addEventListener("mousedown", handleOutsidePointer, true);
    document.addEventListener("click", handleOutsidePointer, true);
    window.addEventListener("scroll", handleViewportChange, true);
    window.addEventListener("resize", handleViewportChange);

    return () => {
      document.removeEventListener("pointerdown", handleOutsidePointer, true);
      document.removeEventListener("mousedown", handleOutsidePointer, true);
      document.removeEventListener("click", handleOutsidePointer, true);
      window.removeEventListener("scroll", handleViewportChange, true);
      window.removeEventListener("resize", handleViewportChange);
    };
  }, [closeMenu, isInsideOpenMenu, position]);

  // Close menu on escape
  useEffect(() => {
    if (!position) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeMenu();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [closeMenu, position]);

  // Listen for other context menus opening — close this one if it's not the source
  useEffect(() => {
    const handler = (e: Event) => {
      const customEvent = e as CustomEvent<{ sourceId: string }>;
      if (customEvent.detail?.sourceId !== menuInstanceId) {
        closeMenu();
      }
    };
    window.addEventListener(CONTEXT_MENU_OPENED, handler);
    return () => window.removeEventListener(CONTEXT_MENU_OPENED, handler);
  }, [closeMenu, menuInstanceId]);

  // Clicking any clipboard item should dismiss a previously opened item menu.
  useEffect(() => {
    const handler = (e: Event) => {
      const customEvent = e as CustomEvent<{ sourceId: string }>;
      if (customEvent.detail?.sourceId !== menuInstanceId) {
        closeMenu();
      }
    };
    window.addEventListener(CONTEXT_MENU_CLOSE_REQUESTED, handler);
    return () =>
      window.removeEventListener(CONTEXT_MENU_CLOSE_REQUESTED, handler);
  }, [closeMenu, menuInstanceId]);

  useLayoutEffect(() => {
    if (!position || !menuRef.current) return;

    const rect = menuRef.current.getBoundingClientRect();
    const nextPosition = clampMenuPosition(
      position.x,
      position.y,
      rect.width,
      rect.height,
    );

    if (
      Math.abs(nextPosition.x - position.x) > 0.5 ||
      Math.abs(nextPosition.y - position.y) > 0.5
    ) {
      setPosition(nextPosition);
    }
  }, [availableTabs.length, position]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (isInsideOpenMenu(e.target)) return;

      // Notify all other context menu instances to close themselves
      window.dispatchEvent(
        new CustomEvent(CONTEXT_MENU_OPENED, {
          detail: { sourceId: menuInstanceId },
        }),
      );
      setPosition(clampMenuPosition(e.clientX, e.clientY));
      setSubMenu(null);
    },
    [isInsideOpenMenu, menuInstanceId],
  );

  const handleItemPointerDownCapture = useCallback(
    (e: React.PointerEvent | React.MouseEvent) => {
      if (isInsideOpenMenu(e.target)) return;

      window.dispatchEvent(
        new CustomEvent(CONTEXT_MENU_CLOSE_REQUESTED, {
          detail: { sourceId: menuInstanceId },
        }),
      );

      if (position) {
        closeMenu();
      }
    },
    [closeMenu, isInsideOpenMenu, menuInstanceId, position],
  );

  const handleMoveToTab = useCallback(
    async (targetTabId: number) => {
      try {
        if (batchIds.length > 1) {
          const moved = await clipboard.moveToTabBatch(batchIds, targetTabId);
          logger.info(`Moved ${moved} items to tab ${targetTabId}`);
          onBatchActionComplete?.();
          closeMenu();
          return;
        }

        await clipboard.moveToTab(itemId, targetTabId);

        // Update local state
        removeItem(itemId);

        // Notify ClipboardList to remove this item incrementally (no full refresh)
        window.dispatchEvent(
          new CustomEvent("clipboard:action", {
            detail: { action: "move", itemId },
          }),
        );

        logger.info(`Item ${itemId} moved to tab ${targetTabId}`);
        closeMenu();
      } catch (error) {
        logger.error("Failed to move item:", error);
      }
    },
    [batchIds, closeMenu, itemId, onBatchActionComplete, removeItem],
  );

  const handleCopyToTab = useCallback(
    async (targetTabId: number) => {
      try {
        if (batchIds.length > 1) {
          const copied = await clipboard.copyToTabBatch(batchIds, targetTabId);
          logger.info(`Copied ${copied} items to tab ${targetTabId}`);
          onBatchActionComplete?.();
          closeMenu();
          return;
        }

        const newId = await clipboard.copyToTab(itemId, targetTabId);

        // If copying to current visible tab, notify ClipboardList
        if (targetTabId === currentTabId) {
          window.dispatchEvent(
            new CustomEvent("clipboard:action", {
              detail: { action: "copy", itemId, newId },
            }),
          );
        }

        logger.info(
          `Item ${itemId} copied to tab ${targetTabId} as new item ${newId}`,
        );
        closeMenu();
      } catch (error) {
        logger.error("Failed to copy item:", error);
      }
    },
    [batchIds, closeMenu, itemId, currentTabId, onBatchActionComplete],
  );

  const createPluginItemApi = useCallback(
    (extension: RegisteredExtension): PluginItemApi => {
      const requirePermission = (permission: string) => {
        if (!hasPermission(extension, permission)) {
          throw new Error(
            `Plugin ${extension.pluginId} requires ${permission} permission`,
          );
        }
      };

      return {
        getItems: () => {
          requirePermission("data:read");
          return selectedItems.map(toTransferItem);
        },
        createText: async (content, options) => {
          requirePermission("data:write");
          return clipboard.create({
            type: ItemType.Text,
            content,
            content_hash: null,
            metadata: null,
            tags: null,
            tab_id: options?.tabId ?? currentTabId ?? null,
            is_sensitive: false,
            is_pinned: false,
          });
        },
        updateContent: async (id, content) => {
          requirePermission("data:write");
          await clipboard.updateContent(id, content);
          updateItem(id, { content });
        },
        updateTags: async (id, tags) => {
          requirePermission("data:write");
          await clipboard.updateTags(id, tags);
          updateItem(id, { tags: JSON.stringify(tags) });
        },
        setPinned: async (id, pinned) => {
          requirePermission("data:write");
          await clipboard.togglePin(id, pinned ? 1 : 0);
          updateItem(id, { is_pinned: pinned });
        },
        deleteItems: async (ids) => {
          requirePermission("data:delete");
          const deleted = await clipboard.deleteByIds(ids);
          ids.forEach((id) => removeItem(id));
          if (ids.length === 1) {
            window.dispatchEvent(
              new CustomEvent("clipboard:action", {
                detail: { action: "delete", itemId: ids[0] },
              }),
            );
          } else {
            onBatchActionComplete?.();
          }
          return deleted;
        },
      };
    },
    [
      currentTabId,
      onBatchActionComplete,
      removeItem,
      selectedItems,
      updateItem,
    ],
  );

  const handlePluginContextMenuAction = useCallback(
    async (entry: RuntimeContextMenuEntry) => {
      try {
        await entry.item.action(createPluginItemApi(entry.extension));
        closeMenu();
      } catch (error) {
        logger.error("Plugin context menu action failed:", {
          pluginId: entry.extension.pluginId,
          actionId: entry.item.id,
          error,
        });
      }
    },
    [closeMenu, createPluginItemApi],
  );

  const handleSubMenuHover = useCallback(
    (menuType: SubMenuType, e: React.MouseEvent<HTMLDivElement>) => {
      setSubMenu({
        ...getSubMenuLayout(
          e.currentTarget.getBoundingClientRect(),
          availableTabs.length,
        ),
        type: menuType,
      });
    },
    [availableTabs.length],
  );

  if (!position) {
    return (
      <div
        onPointerDownCapture={handleItemPointerDownCapture}
        onMouseDownCapture={handleItemPointerDownCapture}
        onContextMenu={handleContextMenu}
      >
        {children}
      </div>
    );
  }

  return (
    <div
      onPointerDownCapture={handleItemPointerDownCapture}
      onMouseDownCapture={handleItemPointerDownCapture}
      onContextMenu={handleContextMenu}
    >
      {children}

      {/* Main Context Menu */}
      <div
        ref={menuRef}
        className="fixed z-50 min-w-40 py-0.5 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700"
        style={{
          left: `${position.x}px`,
          top: `${position.y}px`,
          maxWidth: `calc(100vw - ${VIEWPORT_PADDING * 2}px)`,
        }}
      >
        {pluginContextMenuEntries.length > 0 && (
          <>
            {pluginContextMenuEntries.map((entry) => (
              <button
                key={`${entry.extension.id}:${entry.item.id}`}
                type="button"
                disabled={entry.item.disabled}
                onClick={() => handlePluginContextMenuAction(entry)}
                className="w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 disabled:cursor-not-allowed disabled:opacity-50 truncate whitespace-nowrap"
                title={entry.item.label}
              >
                <span className="flex min-w-0 items-center">
                  {renderPluginMenuIcon(entry.item.icon)}
                  <span className="truncate">{entry.item.label}</span>
                </span>
              </button>
            ))}
            <div className="my-0.5 border-t border-gray-200 dark:border-gray-700" />
          </>
        )}

        {/* Move To Submenu Trigger */}
        <div
          className="relative group"
          onMouseEnter={(e) => handleSubMenuHover("move", e)}
        >
          <button
            type="button"
            className="w-full flex items-center justify-between px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
          >
            <div className="flex items-center min-w-0">
              <FolderPlus size={14} className="mr-2" />
              <span className="truncate">Move to</span>
            </div>
            <ChevronRight size={12} className="ml-2 shrink-0" />
          </button>

          {/* Move To Submenu */}
          {subMenu?.type === "move" && (
            <div
              ref={subMenuRef}
              className="absolute min-w-40 py-0.5 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700"
              style={{
                top: `${subMenu.top}px`,
                maxWidth: `calc(100vw - ${VIEWPORT_PADDING * 2}px)`,
                maxHeight: `calc(100vh - ${VIEWPORT_PADDING * 2}px)`,
                overflowY: "auto",
                ...(subMenu.side === "right"
                  ? { left: `calc(100% + ${SUBMENU_GAP}px)` }
                  : { right: `calc(100% + ${SUBMENU_GAP}px)` }),
              }}
            >
              {availableTabs.length === 0 ? (
                <div className="px-3 py-1.5 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap">
                  No tabs available
                </div>
              ) : (
                availableTabs.map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    onClick={() => handleMoveToTab(tab.id!)}
                    className="w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 truncate whitespace-nowrap"
                    title={tab.name}
                  >
                    {tab.name}
                  </button>
                ))
              )}
            </div>
          )}
        </div>

        {/* Copy To Submenu Trigger */}
        <div
          className="relative group"
          onMouseEnter={(e) => handleSubMenuHover("copy", e)}
        >
          <button
            type="button"
            className="w-full flex items-center justify-between px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
          >
            <div className="flex items-center min-w-0">
              <Copy size={14} className="mr-2" />
              <span className="truncate">Copy to</span>
            </div>
            <ChevronRight size={12} className="ml-2 shrink-0" />
          </button>

          {/* Copy To Submenu */}
          {subMenu?.type === "copy" && (
            <div
              ref={subMenuRef}
              className="absolute min-w-40 py-0.5 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700"
              style={{
                top: `${subMenu.top}px`,
                maxWidth: `calc(100vw - ${VIEWPORT_PADDING * 2}px)`,
                maxHeight: `calc(100vh - ${VIEWPORT_PADDING * 2}px)`,
                overflowY: "auto",
                ...(subMenu.side === "right"
                  ? { left: `calc(100% + ${SUBMENU_GAP}px)` }
                  : { right: `calc(100% + ${SUBMENU_GAP}px)` }),
              }}
            >
              {availableTabs.length === 0 ? (
                <div className="px-3 py-1.5 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap">
                  No tabs available
                </div>
              ) : (
                availableTabs.map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    onClick={() => handleCopyToTab(tab.id!)}
                    className="w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 truncate whitespace-nowrap"
                    title={tab.name}
                  >
                    {tab.name}
                  </button>
                ))
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
