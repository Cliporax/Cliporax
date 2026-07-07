/**
 * Plugin API - IPC wrappers for plugin system
 */

import { emit } from "@tauri-apps/api/event";
import { invokeIpc } from "../../utils/ipc-command";
import { createLogger } from "../../utils/logger";
import type {
  PluginInfo,
  PluginDetail,
  LoadResult,
  Permission,
  PluginState,
  PluginMarketSource,
  MarketRefreshResult,
  InstallPluginResult,
  MarketInstallStatus,
} from "../types";

const logger = createLogger("PluginAPI");
const PLUGIN_CHANGED_EVENT = "cliporax:plugin-changed";

async function notifyPluginChanged(pluginId: string, action: string) {
  window.dispatchEvent(
    new CustomEvent(PLUGIN_CHANGED_EVENT, {
      detail: { pluginId, action },
    }),
  );

  try {
    await emit(PLUGIN_CHANGED_EVENT, { pluginId, action });
  } catch (error) {
    logger.warn("Failed to emit plugin change event:", error);
  }
}

function toTauriShortcut(shortcut: string): string {
  return shortcut
    .replace(/\bCtrl\b/g, "CmdOrControl")
    .replace(/\bCmd\b/g, "CmdOrControl");
}

/**
 * Plugin API namespace
 */
export const pluginApi = {
  /**
   * Get all discovered plugins
   */
  getAll: async (): Promise<PluginInfo[]> => {
    return invokeIpc<PluginInfo[]>({
      logger,
      label: "getAll",
      command: "plugin_get_all",
      onSuccess: (result) =>
        logger.debug("getAll() returned", result.length, "plugins"),
    });
  },

  /**
   * Get plugin detail
   */
  getDetail: async (pluginId: string): Promise<PluginDetail> => {
    return invokeIpc<PluginDetail>({
      logger,
      label: "getDetail",
      command: "plugin_get_detail",
      args: {
        pluginId,
      },
      logArgs: [pluginId],
      onSuccess: (result) =>
        logger.debug("getDetail() returned:", result.manifest.name),
    });
  },

  /**
   * Load a plugin
   */
  load: async (pluginId: string): Promise<LoadResult> => {
    return invokeIpc<LoadResult>({
      logger,
      label: "load",
      command: "plugin_load",
      args: { pluginId },
      logArgs: [pluginId],
      onSuccess: (result) => {
        if ("success" in result) {
          logger.info("load() success:", pluginId);
        } else {
          logger.info("load() requires permissions:", pluginId);
        }
      },
    });
  },

  /**
   * Activate a plugin
   */
  activate: async (pluginId: string): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "activate",
      command: "plugin_activate",
      args: { pluginId },
      logArgs: [pluginId],
      onSuccess: () => logger.info("activate() success:", pluginId),
    });
    await notifyPluginChanged(pluginId, "activate");
  },

  /**
   * Deactivate a plugin
   */
  deactivate: async (pluginId: string): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "deactivate",
      command: "plugin_deactivate",
      args: { pluginId },
      logArgs: [pluginId],
      onSuccess: () => logger.info("deactivate() success:", pluginId),
    });
    await notifyPluginChanged(pluginId, "deactivate");
  },

  /**
   * Unload a plugin
   */
  unload: async (pluginId: string): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "unload",
      command: "plugin_unload",
      args: { pluginId },
      logArgs: [pluginId],
      onSuccess: () => logger.info("unload() success:", pluginId),
    });
    await notifyPluginChanged(pluginId, "unload");
  },

  /**
   * Grant permission to a plugin
   */
  grantPermission: async (
    pluginId: string,
    permission: string,
  ): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "grantPermission",
      command: "plugin_grant_permission",
      args: { pluginId, permission },
      logArgs: [pluginId, permission],
      onSuccess: () =>
        logger.info("grantPermission() success:", pluginId, permission),
    });
  },

  /**
   * Get plugin configuration
   */
  getConfig: async (pluginId: string): Promise<unknown> => {
    return invokeIpc<unknown>({
      logger,
      label: "getConfig",
      command: "plugin_get_config",
      args: { pluginId },
      logArgs: [pluginId],
    });
  },

  /**
   * Update plugin configuration
   */
  updateConfig: async (pluginId: string, config: unknown): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "updateConfig",
      command: "plugin_update_config",
      args: { pluginId, config },
      logArgs: [pluginId],
      onSuccess: () => logger.info("updateConfig() success:", pluginId),
    });
    await notifyPluginChanged(pluginId, "update-config");
  },

  updateShortcut: async (
    pluginId: string,
    oldShortcut: string | null,
    newShortcut: string,
  ): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "updateShortcut",
      command: "plugin_shortcut_update",
      args: {
        pluginId,
        oldShortcut: oldShortcut ? toTauriShortcut(oldShortcut) : null,
        newShortcut: toTauriShortcut(newShortcut),
      },
      logArgs: [pluginId, oldShortcut, newShortcut],
      onSuccess: () => logger.info("updateShortcut() success:", pluginId),
    });
  },

  unregisterShortcut: async (
    pluginId: string,
    shortcut: string,
  ): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "unregisterShortcut",
      command: "plugin_shortcut_unregister",
      args: {
        pluginId,
        shortcut: toTauriShortcut(shortcut),
      },
      logArgs: [pluginId, shortcut],
      onSuccess: () => logger.info("unregisterShortcut() success:", pluginId),
    });
  },

  /**
   * Get all permission definitions
   */
  getPermissionDefinitions: async (): Promise<Permission[]> => {
    return invokeIpc<Permission[]>({
      logger,
      label: "getPermissionDefinitions",
      command: "plugin_get_permission_definitions",
      onSuccess: (result) =>
        logger.debug(
          "getPermissionDefinitions() returned",
          result.length,
          "permissions",
        ),
    });
  },

  /**
   * Discover plugins
   */
  discover: async (): Promise<string[]> => {
    return invokeIpc<string[]>({
      logger,
      label: "discover",
      command: "plugin_discover",
      onSuccess: (result) =>
        logger.info("discover() found", result.length, "plugins"),
    });
  },

  /**
   * Get plugin state
   */
  getState: async (pluginId: string): Promise<PluginState> => {
    return invokeIpc<PluginState>({
      logger,
      label: "getState",
      command: "plugin_get_state",
      args: { pluginId },
      logArgs: [pluginId],
    });
  },

  /**
   * Read plugin script content
   */
  readScript: async (pluginId: string): Promise<string> => {
    return invokeIpc<string>({
      logger,
      label: "readScript",
      command: "plugin_read_script",
      args: { pluginId },
      logArgs: [pluginId],
      onSuccess: (result) =>
        logger.info("readScript() success, length:", result.length),
    });
  },

  getMarketSources: async (): Promise<PluginMarketSource[]> => {
    return invokeIpc<PluginMarketSource[]>({
      logger,
      label: "getMarketSources",
      command: "plugin_market_get_sources",
    });
  },

  refreshMarket: async (): Promise<MarketRefreshResult> => {
    return invokeIpc<MarketRefreshResult>({
      logger,
      label: "refreshMarket",
      command: "plugin_market_refresh",
      onSuccess: (result) =>
        logger.info("refreshMarket() returned", result.plugins.length, "plugins"),
    });
  },

  getMarketPlugins: async (): Promise<MarketRefreshResult> => {
    return invokeIpc<MarketRefreshResult>({
      logger,
      label: "getMarketPlugins",
      command: "plugin_market_get_plugins",
    });
  },

  installFromMarket: async (pluginId: string): Promise<InstallPluginResult> => {
    const result = await invokeIpc<InstallPluginResult>({
      logger,
      label: "installFromMarket",
      command: "plugin_market_install",
      args: {
        request: { pluginId },
      },
      logArgs: [pluginId],
    });
    await notifyPluginChanged(pluginId, "install");
    return result;
  },

  uninstallFromMarket: async (pluginId: string): Promise<void> => {
    await invokeIpc<void>({
      logger,
      label: "uninstallFromMarket",
      command: "plugin_market_uninstall",
      args: { pluginId },
      logArgs: [pluginId],
    });
    await notifyPluginChanged(pluginId, "uninstall");
  },

  getMarketInstallStatus: async (
    pluginId: string,
  ): Promise<MarketInstallStatus> => {
    return invokeIpc<MarketInstallStatus>({
      logger,
      label: "getMarketInstallStatus",
      command: "plugin_market_get_install_status",
      args: { pluginId },
      logArgs: [pluginId],
    });
  },
};

export default pluginApi;
