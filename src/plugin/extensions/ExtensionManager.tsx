/**
 * Extension Point Manager
 * Manages plugin extension points and renders extensions
 */

import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { ImagePlus } from "lucide-react";
import { pluginApi } from "../api/pluginApi";
import { createLogger } from "../../utils/logger";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const logger = createLogger("ExtensionManager");

/**
 * Extension point types
 */
export type ExtensionPointType =
  | "card"
  | "settings-panel"
  | "sidebar"
  | "preview"
  | "context-menu"
  | "toolbar";

/**
 * Extension data for card extension point
 */
export interface CardExtensionData {
  item: {
    id: number;
    content: string;
    type: "text" | "image" | "file";
    is_pinned?: boolean;
  };
  position: "badge" | "action" | "header" | "footer";
}

/**
 * Extension context
 */
export interface ExtensionContext {
  theme: "light" | "dark";
  settings: Record<string, unknown>;
  plugin: {
    id: string;
    name: string;
    version: string;
  };
}

/**
 * Registered extension
 */
export interface RegisteredExtension {
  id: string;
  pluginId: string;
  pointType: ExtensionPointType;
  component: string;
  config: Record<string, unknown>;
  priority: number;
  condition?: string;
}

/**
 * Extension manager state
 */
interface ExtensionManagerState {
  extensions: Map<string, RegisteredExtension[]>;
  loadedPlugins: Set<string>;
}

/**
 * Extension manager context
 */
interface ExtensionManagerContextValue {
  getExtensions: (pointType: ExtensionPointType) => RegisteredExtension[];
  renderExtensions: (
    pointType: ExtensionPointType,
    data: unknown,
    context: ExtensionContext,
  ) => React.ReactNode[];
  refresh: () => Promise<void>;
  loadPluginScript: (pluginId: string, mainPath: string) => Promise<void>;
}

const ExtensionManagerContext =
  createContext<ExtensionManagerContextValue | null>(null);

interface RuntimePlugin {
  onActivate?: (context: ExtensionContext) => void;
  onDeactivate?: () => void;
  extensions?: Record<
    string,
    {
      render?: (props: {
        data: unknown;
        context: ExtensionContext;
        config: Record<string, unknown>;
      }) => HTMLElement | null;
      shouldShow?: (props: {
        data: unknown;
        context: ExtensionContext;
        config: Record<string, unknown>;
      }) => boolean;
    }
  >;
}

declare global {
  interface Window {
    CliporaxPlugins?: Record<string, RuntimePlugin>;
  }
}

/**
 * Evaluate a simple condition expression
 */
function evaluateCondition(
  condition: string | undefined,
  data: unknown,
): boolean {
  if (!condition) return true;

  const cardData = data as Partial<CardExtensionData> | undefined;
  const itemType = cardData?.item?.type;
  const position = cardData?.position;
  const normalized = condition.trim();

  const itemTypeMatch = normalized.match(/^item\.type\s*===\s*['"](\w+)['"]$/);
  if (itemTypeMatch) {
    return itemType === itemTypeMatch[1];
  }

  const positionMatch = normalized.match(
    /^position\s*===\s*['"](badge|action|header|footer)['"]$/,
  );
  if (positionMatch) {
    return position === positionMatch[1];
  }

  logger.warn("Unsupported plugin condition:", condition);
  return false;
}

const actionButtonStyle = (theme: "light" | "dark"): React.CSSProperties => ({
  width: 22,
  height: 22,
  borderRadius: 6,
  border: "none",
  background: theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(255,255,255,0.7)",
  cursor: "pointer",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  color: theme === "dark" ? "#e2e8f0" : "#52525b",
});

const loadedScriptIds = new Set<string>();
const registeredPluginShortcuts = new Map<string, string>();

type PluginShortcutEvent = {
  pluginId: string;
  shortcut: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const PluginDomExtension: React.FC<{
  ext: RegisteredExtension;
  data: unknown;
  context: ExtensionContext;
}> = ({ ext, data, context }) => {
  const hostRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    host.replaceChildren();

    const plugin = window.CliporaxPlugins?.[ext.pluginId];
    const component = plugin?.extensions?.[ext.component];

    if (!component?.render) {
      logger.warn("Plugin extension component is not registered:", {
        pluginId: ext.pluginId,
        component: ext.component,
      });
      return;
    }

    const extensionContext: ExtensionContext = {
      ...context,
      plugin: {
        ...context.plugin,
        id: ext.pluginId,
      },
    };

    try {
      const props = { data, context: extensionContext, config: ext.config };
      if (component.shouldShow && !component.shouldShow(props)) {
        return;
      }

      const element = component.render(props);
      if (element) {
        host.appendChild(element);
      }
    } catch (error) {
      logger.error("Failed to render plugin DOM extension:", ext.id, error);
    }

    return () => {
      host.replaceChildren();
    };
  }, [context, data, ext.component, ext.config, ext.id, ext.pluginId]);

  return (
    <span
      ref={hostRef}
      style={{
        display: ext.pointType === "sidebar" ? "block" : "inline-flex",
        width: ext.pointType === "sidebar" ? "100%" : undefined,
      }}
    />
  );
};

const SidebarExtensions: React.FC<{ theme: "light" | "dark" }> = ({
  theme,
}) => {
  const { renderExtensions } = useExtensionManager();
  const extensions = renderExtensions(
    "sidebar",
    { position: "sidebar" },
    {
      theme,
      settings: {},
      plugin: { id: "", name: "", version: "" },
    },
  );

  if (extensions.length === 0) return null;

  return (
    <div
      className="absolute right-3 top-3 z-20 flex w-40 flex-col gap-2"
      style={{ pointerEvents: "auto" }}
    >
      {extensions}
    </div>
  );
};

/**
 * Extension Manager Provider
 */
export const ExtensionManagerProvider: React.FC<{
  children: React.ReactNode;
}> = ({ children }) => {
  const [state, setState] = useState<ExtensionManagerState>({
    extensions: new Map(),
    loadedPlugins: new Set(),
  });

  /**
   * Load plugin script dynamically
   */
  const loadPluginScript = useCallback(
    async (pluginId: string, mainPath: string): Promise<void> => {
      logger.info("Ignoring imperative plugin script load:", pluginId, mainPath);
      setState((prev) => {
        if (prev.loadedPlugins.has(pluginId)) {
          return prev;
        }

        const newLoaded = new Set(prev.loadedPlugins);
        newLoaded.add(pluginId);
        return { ...prev, loadedPlugins: newLoaded };
      });
    },
    [],
  );

  /**
   * Load plugin script via IPC and execute it
   */
  const loadPluginScriptViaIPC = useCallback(async (pluginId: string) => {
    if (!loadedScriptIds.has(pluginId)) {
      const scriptContent = await pluginApi.readScript(pluginId);
      const script = document.createElement("script");
      script.dataset.cliporaxPluginId = pluginId;
      script.textContent = scriptContent;
      document.head.appendChild(script);
      loadedScriptIds.add(pluginId);

      const plugin = window.CliporaxPlugins?.[pluginId];
      if (plugin?.onActivate) {
        plugin.onActivate({
          theme: "dark",
          settings: {},
          plugin: {
            id: pluginId,
            name: pluginId,
            version: "",
          },
        });
      }
      logger.info("Loaded plugin script:", pluginId);
    }

    setState((prev) => {
      if (prev.loadedPlugins.has(pluginId)) {
        return prev;
      }

      const newLoaded = new Set(prev.loadedPlugins);
      newLoaded.add(pluginId);
      return { ...prev, loadedPlugins: newLoaded };
    });
  }, []);

  /**
   * Load extensions from active plugins
   * Script loading is non-blocking to avoid delaying extension registration
   */
  const loadExtensions = useCallback(async () => {
    try {
      const plugins = await pluginApi.getAll();
      logger.info(
        "Found plugins:",
        plugins.map((p) => ({ id: p.id, state: p.state })),
      );

      const extensions = new Map<string, RegisteredExtension[]>();

      for (const plugin of plugins) {
        // Only load extensions from active plugins
        if (plugin.state !== "active") {
          logger.debug("Skipping non-active plugin:", plugin.id, plugin.state);
          continue;
        }

        await loadPluginScriptViaIPC(plugin.id);

        // Get plugin detail to access extensions
        try {
          const detail = await pluginApi.getDetail(plugin.id);
          const manifest = detail.manifest;

          const pluginConfig = isRecord(detail.config) ? detail.config : {};

          if (manifest.configSchema?.fields) {
            for (const field of manifest.configSchema.fields) {
              if (field.type !== "shortcut" || field.global !== true) {
                continue;
              }

              const shortcut = String(
                pluginConfig[field.key] ?? field.default ?? "",
              ).trim();
              if (!shortcut) continue;

              const shortcutKey = `${plugin.id}:${field.key}`;
              const oldShortcut = registeredPluginShortcuts.get(shortcutKey);
              if (oldShortcut !== shortcut) {
                try {
                  await pluginApi.updateShortcut(
                    plugin.id,
                    oldShortcut ?? null,
                    shortcut,
                  );
                  registeredPluginShortcuts.set(shortcutKey, shortcut);
                } catch (error) {
                  logger.warn("Failed to register plugin shortcut:", {
                    pluginId: plugin.id,
                    shortcut,
                    error,
                  });
                }
              }
            }
          }

          if (manifest.extensions && Array.isArray(manifest.extensions)) {
            for (const ext of manifest.extensions) {
              const registered: RegisteredExtension = {
                id: `${plugin.id}:${ext.point}`,
                pluginId: plugin.id,
                pointType: ext.point as ExtensionPointType,
                component: ext.component,
                config: pluginConfig,
                priority: ext.priority || 0,
                condition: ext.condition,
              };

              const key = ext.point;
              if (!extensions.has(key)) {
                extensions.set(key, []);
              }
              extensions.get(key)!.push(registered);
              logger.info("Registered extension:", registered.id);
            }
          }
        } catch (e) {
          logger.warn("Failed to get plugin detail:", plugin.id, e);
        }
      }

      // Sort extensions by priority (higher first)
      for (const [key, exts] of extensions) {
        exts.sort((a, b) => b.priority - a.priority);
      }

      const activePluginIds = new Set(
        plugins
          .filter((plugin) => plugin.state === "active")
          .map((plugin) => plugin.id),
      );
      for (const [shortcutKey, shortcut] of registeredPluginShortcuts) {
        const pluginId = shortcutKey.split(":")[0];
        if (activePluginIds.has(pluginId)) {
          continue;
        }

        try {
          await pluginApi.unregisterShortcut(pluginId, shortcut);
        } catch (error) {
          logger.warn("Failed to unregister plugin shortcut:", {
            pluginId,
            shortcut,
            error,
          });
        }
        registeredPluginShortcuts.delete(shortcutKey);
      }

      setState((prev) => ({ ...prev, extensions }));
      logger.info("Loaded extensions:", extensions.size, "extension points");
    } catch (e) {
      logger.error("Failed to load extensions:", e);
    }
  }, [loadPluginScriptViaIPC]);

  /**
   * Get extensions for a point type
   */
  const getExtensions = useCallback(
    (pointType: ExtensionPointType): RegisteredExtension[] => {
      return state.extensions.get(pointType) || [];
    },
    [state.extensions],
  );

  const renderDeclarativeExtension = useCallback(
    (
      ext: RegisteredExtension,
      data: unknown,
      context: ExtensionContext,
    ): React.ReactNode | null => {
      const cardData = data as Partial<CardExtensionData>;
      const item = cardData.item;
      const position = cardData.position;

      if (ext.pointType !== "card" || position !== "action" || !item) {
        return null;
      }

      if (ext.component === "PreviewButton" && item.type === "image") {
        return (
          <button
            key={ext.id}
            type="button"
            title="Preview Image"
            style={actionButtonStyle(context.theme)}
            onClick={async (event) => {
              event.stopPropagation();
              try {
                await invoke("preview_create_window", {
                  imageData: item.content,
                  title: `Image Preview - #${item.id}`,
                });
              } catch (error) {
                logger.error("Failed to open image preview:", error);
              }
            }}
          >
            <ImagePlus size={14} />
          </button>
        );
      }

      return (
        <PluginDomExtension
          key={ext.id}
          ext={ext}
          data={data}
          context={context}
        />
      );
    },
    [],
  );

  /**
   * Render extensions for a point type
   */
  const renderExtensions = useCallback(
    (
      pointType: ExtensionPointType,
      data: unknown,
      context: ExtensionContext,
    ): React.ReactNode[] => {
      const extensions = getExtensions(pointType);
      const results: React.ReactNode[] = [];

      for (const ext of extensions) {
        // Check condition
        if (!evaluateCondition(ext.condition, data)) {
          //   logger.debug("Condition not met:", ext.condition, "data:", data);
          continue;
        }

        try {
          const element = renderDeclarativeExtension(ext, data, context);

          if (element) {
            results.push(<React.Fragment key={ext.id}>{element}</React.Fragment>);
          }
        } catch (e) {
          logger.error("Failed to render extension:", ext.id, e);
        }
      }

      return results;
    },
    [getExtensions, renderDeclarativeExtension],
  );

  /**
   * Refresh extensions
   */
  const refresh = useCallback(async () => {
    await loadExtensions();
  }, [loadExtensions]);

  /**
   * Load extensions on mount and when plugins change
   */
  useEffect(() => {
    loadExtensions();

    // Listen for plugin state changes
    const handlePluginChange = () => {
      logger.info("Plugin state changed, refreshing extensions");
      loadExtensions();
    };

    window.addEventListener("cliporax:plugin-changed", handlePluginChange);

    // REMOVED: Periodic polling was causing performance issues and window freezing
    // Extensions are now refreshed only on explicit plugin state changes
    // const interval = setInterval(() => {
    //   loadExtensions();
    // }, 2000);

    return () => {
      window.removeEventListener(
        "cliporax:plugin-changed",
        handlePluginChange,
      );
      // clearInterval(interval);
    };
  }, [loadExtensions]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<PluginShortcutEvent>("plugin:shortcut", (event) => {
      window.dispatchEvent(
        new CustomEvent("cliporax:plugin-shortcut", {
          detail: event.payload,
        }),
      );
    })
      .then((cleanup) => {
        unlisten = cleanup;
      })
      .catch((error) => {
        logger.error("Failed to listen for plugin shortcuts:", error);
      });

    return () => {
      unlisten?.();
    };
  }, []);

  const value = useMemo(
    () => ({
      getExtensions,
      renderExtensions,
      refresh,
      loadPluginScript,
    }),
    [getExtensions, renderExtensions, refresh, loadPluginScript],
  );

  return (
    <ExtensionManagerContext.Provider value={value}>
      {children}
    </ExtensionManagerContext.Provider>
  );
};

/**
 * Hook to use extension manager
 */
export const useExtensionManager = (): ExtensionManagerContextValue => {
  const context = useContext(ExtensionManagerContext);
  if (!context) {
    return {
      getExtensions: () => [],
      renderExtensions: () => [],
      refresh: async () => {},
      loadPluginScript: async () => {},
    };
  }
  return context;
};

/**
 * Hook to render card extensions
 */
export const useCardExtensions = (
  item: CardExtensionData["item"],
  position: CardExtensionData["position"],
  theme: "light" | "dark",
): React.ReactNode[] => {
  const { renderExtensions, getExtensions } = useExtensionManager();

  const data: CardExtensionData = { item, position };
  const context: ExtensionContext = {
    theme,
    settings: {},
    plugin: { id: "", name: "", version: "" },
  };

  // Debug logging (commented to reduce noise)
  // React.useEffect(() => {
  //   const extensions = getExtensions("card");
  //   if (extensions.length > 0) {
  //     logger.debug(
  //       "Card extensions:",
  //       extensions.length,
  //       "item:",
  //       item?.id,
  //       "type:",
  //       item?.type,
  //     );
  //   }
  // }, [item?.id, item?.type, getExtensions]);

  return renderExtensions("card", data, context);
};

export const PluginSidebarExtensions: React.FC<{
  theme: "light" | "dark";
}> = ({ theme }) => <SidebarExtensions theme={theme} />;

export default ExtensionManagerContext;
