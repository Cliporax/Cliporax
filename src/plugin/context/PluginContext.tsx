/**
 * Plugin Context - React Context for plugin state management
 */

import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useMemo,
} from "react";
import { pluginApi } from "../api/pluginApi";
import { createLogger } from "../../utils/logger";
import { PluginSandbox } from "../sandbox/PluginSandbox";
import type {
  PluginInfo,
  PluginDetail,
  LoadResult,
  Permission,
  PluginState,
} from "../types";

const logger = createLogger("PluginContext");

// Sandbox instances storage (secure execution in Web Workers)
const sandboxes = new Map<string, PluginSandbox>();

/**
 * Plugin context state
 */
interface PluginContextState {
  /** All discovered plugins */
  plugins: PluginInfo[];

  /** Loading state */
  isLoading: boolean;

  /** Error message */
  error: string | null;

  /** Permission definitions */
  permissionDefinitions: Permission[];

  /** Selected plugin ID */
  selectedPluginId: string | null;

  /** Selected plugin detail */
  selectedPlugin: PluginDetail | null;
}

/**
 * Plugin context actions
 */
interface PluginContextActions {
  /** Refresh plugin list */
  refresh: () => Promise<void>;

  /** Select a plugin */
  selectPlugin: (pluginId: string | null) => void;

  /** Load a plugin */
  loadPlugin: (pluginId: string) => Promise<LoadResult>;

  /** Activate a plugin */
  activatePlugin: (pluginId: string) => Promise<void>;

  /** Deactivate a plugin */
  deactivatePlugin: (pluginId: string) => Promise<void>;

  /** Unload a plugin */
  unloadPlugin: (pluginId: string) => Promise<void>;

  /** Grant permission */
  grantPermission: (pluginId: string, permission: string) => Promise<void>;

  /** Get plugin detail */
  getPluginDetail: (pluginId: string) => Promise<PluginDetail>;

  /** Update plugin config */
  updatePluginConfig: (pluginId: string, config: unknown) => Promise<void>;

  /** Clear error */
  clearError: () => void;
}

/**
 * Plugin context value
 */
type PluginContextValue = PluginContextState & PluginContextActions;

/**
 * Plugin context
 */
const PluginContext = createContext<PluginContextValue | null>(null);

/**
 * Plugin provider props
 */
interface PluginProviderProps {
  children: React.ReactNode;
}

/**
 * Plugin provider component
 */
export const PluginProvider: React.FC<PluginProviderProps> = ({ children }) => {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [permissionDefinitions, setPermissionDefinitions] = useState<
    Permission[]
  >([]);
  const [selectedPluginId, setSelectedPluginId] = useState<string | null>(null);
  const [selectedPlugin, setSelectedPlugin] = useState<PluginDetail | null>(
    null,
  );

  /**
   * Load plugins and permission definitions
   */
  const loadInitialData = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const [pluginList, permissions] = await Promise.all([
        pluginApi.getAll(),
        pluginApi.getPermissionDefinitions(),
      ]);

      setPlugins(pluginList);
      setPermissionDefinitions(permissions);
      logger.info("Loaded", pluginList.length, "plugins");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      logger.error("Failed to load initial data:", err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Load initial data on mount
   */
  useEffect(() => {
    loadInitialData();
  }, [loadInitialData]);

  /**
   * Refresh plugin list
   */
  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const pluginList = await pluginApi.getAll();
      setPlugins(pluginList);
      logger.info(
        "Refreshed plugin list:",
        pluginList.map((p) => ({ id: p.id, state: p.state })),
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      logger.error("Failed to refresh plugins:", err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Select a plugin
   */
  const selectPlugin = useCallback(async (pluginId: string | null) => {
    setSelectedPluginId(pluginId);

    if (pluginId) {
      try {
        const detail = await pluginApi.getDetail(pluginId);
        setSelectedPlugin(detail);
      } catch (err) {
        logger.error("Failed to get plugin detail:", err);
        setSelectedPlugin(null);
      }
    } else {
      setSelectedPlugin(null);
    }
  }, []);

  /**
   * Load a plugin
   */
  const loadPlugin = useCallback(
    async (pluginId: string): Promise<LoadResult> => {
      setError(null);

      try {
        const result = await pluginApi.load(pluginId);
        logger.info("Load result:", result);

        // Refresh plugin list from backend to get updated state
        await refresh();

        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        logger.error("Failed to load plugin:", err);
        throw err;
      }
    },
    [],
  );

  /**
   * Activate a plugin.
   * UI extension plugins are executed by ExtensionManager in the window context,
   * because their render functions are expected to create DOM elements.
   */
  const activatePlugin = useCallback(async (pluginId: string) => {
    setError(null);

    try {
      await pluginApi.activate(pluginId);

      const detail = await pluginApi.getDetail(pluginId);
      const hasUiExtensions =
        Array.isArray(detail.manifest.extensions) &&
        detail.manifest.extensions.length > 0;

      // Check if sandbox already exists for this plugin
      if (hasUiExtensions) {
        logger.info(
          "Skipping worker sandbox for UI extension plugin:",
          pluginId,
        );
      } else if (!sandboxes.has(pluginId)) {
        try {
          logger.info("Loading plugin script via sandbox:", pluginId);

          // Read script content via IPC
          const scriptContent = await pluginApi.readScript(pluginId);
          logger.info("Script content loaded, length:", scriptContent.length);

          // Create sandbox with permissions
          const sandbox = new PluginSandbox({
            pluginId,
            permissions: detail.grantedPermissions.map((p: string) => ({
              permission: p,
              reason: "",
              required: false,
            })),
            manifest: detail.manifest,
          });

          // Set log handler
          sandbox.setLogHandler((level, message) => {
            logger.info(`[Plugin:${pluginId}] ${level}:`, message);
          });

          // Load script into sandbox
          await sandbox.loadScript(scriptContent);

          // Store sandbox instance
          sandboxes.set(pluginId, sandbox);

          logger.info("Plugin loaded in sandbox successfully:", pluginId);
        } catch (scriptErr) {
          logger.error("Failed to load plugin script in sandbox:", scriptErr);
          // Fail the activation if sandbox fails - no fallback execution
          // First deactivate the plugin from backend
          try {
            await pluginApi.deactivate(pluginId);
          } catch (deactivateErr) {
            logger.error(
              "Failed to deactivate plugin after sandbox error:",
              deactivateErr,
            );
          }
          throw new Error(
            `Plugin sandbox failed to load: ${scriptErr instanceof Error ? scriptErr.message : String(scriptErr)}`,
          );
        }
      } else {
        logger.info("Plugin already loaded in sandbox:", pluginId);
      }

      // Update plugin in list
      setPlugins((prev) =>
        prev.map((p) =>
          p.id === pluginId ? { ...p, state: "active" as PluginState } : p,
        ),
      );

      // Refresh plugin list from backend
      await refresh();

      logger.info("Activated plugin:", pluginId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      logger.error("Failed to activate plugin:", err);
      throw err;
    }
  }, []);

  /**
   * Deactivate a plugin
   */
  const deactivatePlugin = useCallback(async (pluginId: string) => {
    setError(null);

    try {
      await pluginApi.deactivate(pluginId);

      // Destroy sandbox if exists
      const sandbox = sandboxes.get(pluginId);
      if (sandbox) {
        sandbox.destroy();
        sandboxes.delete(pluginId);
        logger.info("Destroyed sandbox for plugin:", pluginId);
      }

      // Refresh plugin list from backend
      await refresh();

      logger.info("Deactivated plugin:", pluginId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      logger.error("Failed to deactivate plugin:", err);
      throw err;
    }
  }, []);

  /**
   * Unload a plugin
   */
  const unloadPlugin = useCallback(async (pluginId: string) => {
    setError(null);

    try {
      await pluginApi.unload(pluginId);

      // Destroy sandbox if exists
      const sandbox = sandboxes.get(pluginId);
      if (sandbox) {
        sandbox.destroy();
        sandboxes.delete(pluginId);
        logger.info("Destroyed sandbox for plugin:", pluginId);
      }

      // Refresh plugin list from backend
      await refresh();

      logger.info("Unloaded plugin:", pluginId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      logger.error("Failed to unload plugin:", err);
      throw err;
    }
  }, []);

  /**
   * Grant permission
   */
  const grantPermission = useCallback(
    async (pluginId: string, permission: string) => {
      setError(null);

      try {
        await pluginApi.grantPermission(pluginId, permission);

        // Update selected plugin if it's the one being modified
        if (selectedPluginId === pluginId && selectedPlugin) {
          setSelectedPlugin({
            ...selectedPlugin,
            grantedPermissions: [
              ...selectedPlugin.grantedPermissions,
              permission,
            ],
          });
        }

        logger.info("Granted permission:", permission, "to plugin:", pluginId);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        logger.error("Failed to grant permission:", err);
        throw err;
      }
    },
    [selectedPluginId, selectedPlugin],
  );

  /**
   * Get plugin detail
   */
  const getPluginDetail = useCallback(async (pluginId: string) => {
    return pluginApi.getDetail(pluginId);
  }, []);

  /**
   * Update plugin config
   */
  const updatePluginConfig = useCallback(
    async (pluginId: string, config: unknown) => {
      setError(null);

      try {
        await pluginApi.updateConfig(pluginId, config);

        // Update selected plugin if it's the one being modified
        if (selectedPluginId === pluginId && selectedPlugin) {
          setSelectedPlugin({
            ...selectedPlugin,
            config,
          });
        }

        logger.info("Updated config for plugin:", pluginId);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        logger.error("Failed to update plugin config:", err);
        throw err;
      }
    },
    [selectedPluginId, selectedPlugin],
  );

  /**
   * Clear error
   */
  const clearError = useCallback(() => {
    setError(null);
  }, []);

  /**
   * Context value
   */
  const value = useMemo<PluginContextValue>(
    () => ({
      plugins,
      isLoading,
      error,
      permissionDefinitions,
      selectedPluginId,
      selectedPlugin,
      refresh,
      selectPlugin,
      loadPlugin,
      activatePlugin,
      deactivatePlugin,
      unloadPlugin,
      grantPermission,
      getPluginDetail,
      updatePluginConfig,
      clearError,
    }),
    [
      plugins,
      isLoading,
      error,
      permissionDefinitions,
      selectedPluginId,
      selectedPlugin,
      refresh,
      selectPlugin,
      loadPlugin,
      activatePlugin,
      deactivatePlugin,
      unloadPlugin,
      grantPermission,
      getPluginDetail,
      updatePluginConfig,
      clearError,
    ],
  );

  return (
    <PluginContext.Provider value={value}>{children}</PluginContext.Provider>
  );
};

/**
 * Hook to use plugin context
 */
export const usePlugin = (): PluginContextValue => {
  const context = useContext(PluginContext);
  if (!context) {
    throw new Error("usePlugin must be used within a PluginProvider");
  }
  return context;
};

export default PluginContext;
