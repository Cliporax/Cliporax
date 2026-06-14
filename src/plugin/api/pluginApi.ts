/**
 * Plugin API - IPC wrappers for plugin system
 */

import { invoke } from "@tauri-apps/api/core";
import { createLogger } from "../../utils/logger";
import type {
  PluginInfo,
  PluginDetail,
  LoadResult,
  Permission,
  PluginState,
} from "../types";

const logger = createLogger("PluginAPI");

function dispatchPluginChanged() {
  window.dispatchEvent(new CustomEvent("cliporax:plugin-changed"));
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
    logger.debug("getAll() called");
    try {
      const result = await invoke<PluginInfo[]>("plugin_get_all");
      logger.debug("getAll() returned", result.length, "plugins");
      return result;
    } catch (error) {
      logger.error("getAll() failed:", error);
      throw error;
    }
  },

  /**
   * Get plugin detail
   */
  getDetail: async (pluginId: string): Promise<PluginDetail> => {
    logger.debug("getDetail() called:", pluginId);
    try {
      const result = await invoke<PluginDetail>("plugin_get_detail", {
        pluginId,
      });
      logger.debug("getDetail() returned:", result.manifest.name);
      return result;
    } catch (error) {
      logger.error("getDetail() failed:", error);
      throw error;
    }
  },

  /**
   * Load a plugin
   */
  load: async (pluginId: string): Promise<LoadResult> => {
    logger.debug("load() called:", pluginId);
    try {
      const result = await invoke<LoadResult>("plugin_load", { pluginId });
      if ("success" in result) {
        logger.info("load() success:", pluginId);
      } else {
        logger.info("load() requires permissions:", pluginId);
      }
      return result;
    } catch (error) {
      logger.error("load() failed:", error);
      throw error;
    }
  },

  /**
   * Activate a plugin
   */
  activate: async (pluginId: string): Promise<void> => {
    logger.debug("activate() called:", pluginId);
    try {
      await invoke("plugin_activate", { pluginId });
      logger.info("activate() success:", pluginId);
      dispatchPluginChanged();
    } catch (error) {
      logger.error("activate() failed:", error);
      throw error;
    }
  },

  /**
   * Deactivate a plugin
   */
  deactivate: async (pluginId: string): Promise<void> => {
    logger.debug("deactivate() called:", pluginId);
    try {
      await invoke("plugin_deactivate", { pluginId });
      logger.info("deactivate() success:", pluginId);
      dispatchPluginChanged();
    } catch (error) {
      logger.error("deactivate() failed:", error);
      throw error;
    }
  },

  /**
   * Unload a plugin
   */
  unload: async (pluginId: string): Promise<void> => {
    logger.debug("unload() called:", pluginId);
    try {
      await invoke("plugin_unload", { pluginId });
      logger.info("unload() success:", pluginId);
      dispatchPluginChanged();
    } catch (error) {
      logger.error("unload() failed:", error);
      throw error;
    }
  },

  /**
   * Grant permission to a plugin
   */
  grantPermission: async (
    pluginId: string,
    permission: string,
  ): Promise<void> => {
    logger.debug("grantPermission() called:", pluginId, permission);
    try {
      await invoke("plugin_grant_permission", { pluginId, permission });
      logger.info("grantPermission() success:", pluginId, permission);
    } catch (error) {
      logger.error("grantPermission() failed:", error);
      throw error;
    }
  },

  /**
   * Get plugin configuration
   */
  getConfig: async (pluginId: string): Promise<unknown> => {
    logger.debug("getConfig() called:", pluginId);
    try {
      const result = await invoke("plugin_get_config", { pluginId });
      return result;
    } catch (error) {
      logger.error("getConfig() failed:", error);
      throw error;
    }
  },

  /**
   * Update plugin configuration
   */
  updateConfig: async (pluginId: string, config: unknown): Promise<void> => {
    logger.debug("updateConfig() called:", pluginId);
    try {
      await invoke("plugin_update_config", { pluginId, config });
      logger.info("updateConfig() success:", pluginId);
      dispatchPluginChanged();
    } catch (error) {
      logger.error("updateConfig() failed:", error);
      throw error;
    }
  },

  updateShortcut: async (
    pluginId: string,
    oldShortcut: string | null,
    newShortcut: string,
  ): Promise<void> => {
    logger.debug("updateShortcut() called:", pluginId, oldShortcut, newShortcut);
    try {
      await invoke("plugin_shortcut_update", {
        pluginId,
        oldShortcut: oldShortcut ? toTauriShortcut(oldShortcut) : null,
        newShortcut: toTauriShortcut(newShortcut),
      });
      logger.info("updateShortcut() success:", pluginId);
    } catch (error) {
      logger.error("updateShortcut() failed:", error);
      throw error;
    }
  },

  unregisterShortcut: async (
    pluginId: string,
    shortcut: string,
  ): Promise<void> => {
    logger.debug("unregisterShortcut() called:", pluginId, shortcut);
    try {
      await invoke("plugin_shortcut_unregister", {
        pluginId,
        shortcut: toTauriShortcut(shortcut),
      });
      logger.info("unregisterShortcut() success:", pluginId);
    } catch (error) {
      logger.error("unregisterShortcut() failed:", error);
      throw error;
    }
  },

  /**
   * Get all permission definitions
   */
  getPermissionDefinitions: async (): Promise<Permission[]> => {
    logger.debug("getPermissionDefinitions() called");
    try {
      const result = await invoke<Permission[]>(
        "plugin_get_permission_definitions",
      );
      logger.debug(
        "getPermissionDefinitions() returned",
        result.length,
        "permissions",
      );
      return result;
    } catch (error) {
      logger.error("getPermissionDefinitions() failed:", error);
      throw error;
    }
  },

  /**
   * Discover plugins
   */
  discover: async (): Promise<string[]> => {
    logger.debug("discover() called");
    try {
      const result = await invoke<string[]>("plugin_discover");
      logger.info("discover() found", result.length, "plugins");
      return result;
    } catch (error) {
      logger.error("discover() failed:", error);
      throw error;
    }
  },

  /**
   * Get plugin state
   */
  getState: async (pluginId: string): Promise<PluginState> => {
    logger.debug("getState() called:", pluginId);
    try {
      const result = await invoke<PluginState>("plugin_get_state", {
        pluginId,
      });
      return result;
    } catch (error) {
      logger.error("getState() failed:", error);
      throw error;
    }
  },

  /**
   * Read plugin script content
   */
  readScript: async (pluginId: string): Promise<string> => {
    logger.debug("readScript() called:", pluginId);
    try {
      const result = await invoke<string>("plugin_read_script", {
        pluginId,
      });
      logger.info("readScript() success, length:", result.length);
      return result;
    } catch (error) {
      logger.error("readScript() failed:", error);
      throw error;
    }
  },
};

export default pluginApi;
