import React, { useState } from "react";
import { X, Puzzle, Loader2, RefreshCw, Keyboard } from "lucide-react";
import { useTranslation } from "react-i18next";
import { createLogger } from "../../utils/logger";
import { usePlugin, PermissionPrompt } from "../../plugin";
import { pluginApi } from "../../plugin/api/pluginApi";
import { PluginDetailModal } from "../../plugin/components/PluginDetailModal";
import type { PluginDetail, PluginState } from "../../plugin/types";
import { useToast } from "../Toast";

const logger = createLogger("PluginsTab");

interface PluginsTabProps {
  isDark: boolean;
}

const PluginsTab: React.FC<PluginsTabProps> = ({ isDark }) => {
  const { t } = useTranslation();
  const toast = useToast();
  const [showPermissionPrompt, setShowPermissionPrompt] = useState(false);
  const [pendingPermissions, setPendingPermissions] = useState<
    Array<{ permission: string; reason: string; required?: boolean }>
  >([]);
  const [pendingPermissionPluginId, setPendingPermissionPluginId] = useState<
    string | null
  >(null);
  const [pluginStateOverrides, setPluginStateOverrides] = useState<
    Record<string, PluginState>
  >({});
  const [selectedPluginDetail, setSelectedPluginDetail] =
    useState<PluginDetail | null>(null);

  const {
    plugins,
    isLoading,
    error,
    permissionDefinitions,
    selectedPlugin,
    selectedPluginId,
    refresh,
    selectPlugin,
    loadPlugin,
    activatePlugin,
    deactivatePlugin,
    unloadPlugin,
    grantPermission,
    getPluginDetail,
    updatePluginConfig,
  } = usePlugin();

  const handleRefresh = async () => {
    setPluginStateOverrides({});
    await refresh();
  };

  const setPluginStateOverride = (pluginId: string, state: PluginState) => {
    setPluginStateOverrides((prev) => ({ ...prev, [pluginId]: state }));
  };

  const handleSelectPlugin = async (pluginId: string | null) => {
    await selectPlugin(pluginId);
    if (pluginId) {
      try {
        const detail = await getPluginDetail(pluginId);
        setSelectedPluginDetail(detail);
      } catch (err) {
        logger.error("Failed to get plugin detail:", err);
      }
    } else {
      setSelectedPluginDetail(null);
    }
  };

  const handleLoadPlugin = async (pluginId: string) => {
    try {
      logger.info("Loading plugin:", pluginId);
      const result = await loadPlugin(pluginId);
      logger.info("Load result:", result);

      // Check if permissions are required
      if (
        "permissionRequired" in result &&
        result.permissionRequired &&
        result.permissionRequired.length > 0
      ) {
        setPendingPermissionPluginId(pluginId);
        setPluginStateOverride(pluginId, "pending-permission");
        setPendingPermissions(result.permissionRequired);
        setShowPermissionPrompt(true);
        await refresh();
        return;
      }

      setPluginStateOverride(pluginId, "loaded");
      await refresh();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      logger.error("Failed to load plugin:", err);
      toast.error(`${t("plugins.error", { message })}`);
    }
  };

  const handleGrantPermission = async (permission: string) => {
    const pluginId = pendingPermissionPluginId || selectedPluginDetail?.manifest.id;
    if (pluginId) {
      await grantPermission(pluginId, permission);
    }
  };

  const handleGrantAll = async () => {
    const pluginId = pendingPermissionPluginId || selectedPluginDetail?.manifest.id;
    if (!pluginId) {
      return;
    }

    for (const perm of pendingPermissions) {
      await grantPermission(pluginId, perm.permission);
    }
    setShowPermissionPrompt(false);
    setPendingPermissions([]);
    setPendingPermissionPluginId(null);

    const result = await loadPlugin(pluginId);
    if (
      "permissionRequired" in result &&
      result.permissionRequired &&
      result.permissionRequired.length > 0
    ) {
      setPendingPermissionPluginId(pluginId);
      setPluginStateOverride(pluginId, "pending-permission");
      setPendingPermissions(result.permissionRequired);
      setShowPermissionPrompt(true);
      await refresh();
      return;
    }

    setPluginStateOverride(pluginId, "loaded");
    await refresh();

    if (selectedPluginDetail?.manifest.id === pluginId) {
      const detail = await getPluginDetail(pluginId);
      setSelectedPluginDetail(detail);
    }
  };

  const handleDenyPermissions = () => {
    setShowPermissionPrompt(false);
    setPendingPermissions([]);
    setPendingPermissionPluginId(null);
  };

  const handleUnloadPlugin = async (pluginId: string) => {
    try {
      await unloadPlugin(pluginId);
      setShowPermissionPrompt(false);
      setPendingPermissions([]);
      if (pendingPermissionPluginId === pluginId) {
        setPendingPermissionPluginId(null);
      }
      setPluginStateOverride(pluginId, "unloaded");

      if (selectedPluginDetail?.manifest.id === pluginId) {
        const detail = await getPluginDetail(pluginId);
        setSelectedPluginDetail(detail);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      logger.error("Failed to unload plugin:", err);
      toast.error(`${t("plugins.error", { message })}`);
    }
  };

  const getStateKey = (state: string | { error: string }): string => {
    if (typeof state === "string") {
      return state;
    }

    if ("error" in state) {
      return "error";
    }

    return Object.keys(state as Record<string, unknown>)[0] || "unknown";
  };

  const getStateLabel = (state: string | { error: string }) => {
    const stateKey = getStateKey(state);

    if (stateKey === "error") {
      return t("common.error");
    }
    switch (stateKey) {
      case "active":
        return t("plugins.state.active");
      case "loaded":
        return t("plugins.state.loaded");
      case "inactive":
        return t("plugins.state.inactive");
      case "pending-permission":
        return t("plugins.state.pendingPermission");
      case "discovered":
      case "unloaded":
        return t("plugins.state.unloaded");
      default:
        return t("plugins.state.unknown");
    }
  };

  const getActionLabel = (isLoaded: boolean, isActive: boolean) => {
    if (!isLoaded) return t("plugins.load");
    return isActive ? t("plugins.deactivate") : t("plugins.enable");
  };

  const syncGlobalShortcutConfig = async (
    detail: PluginDetail,
    nextConfig: Record<string, any>,
  ) => {
    const fields = detail.manifest.configSchema?.fields || [];
    const previousConfig =
      typeof detail.config === "object" && detail.config !== null
        ? (detail.config as Record<string, any>)
        : {};

    for (const field of fields) {
      if (field.type !== "shortcut" || field.global !== true) continue;

      const oldShortcut = previousConfig[field.key] || field.default || null;
      const newShortcut = nextConfig[field.key] || field.default || "";
      if (!newShortcut || oldShortcut === newShortcut) continue;

      await pluginApi.updateShortcut(
        detail.manifest.id,
        oldShortcut,
        newShortcut,
      );
    }
  };

  return (
    <div className="flex flex-col h-full space-y-4">
      {/* Header */}
      <div
        className="flex items-center justify-between p-4 rounded-xl flex-shrink-0"
        style={{
          backgroundColor: isDark
            ? "rgba(255,255,255,0.05)"
            : "rgba(255,255,255,0.6)",
          border: `1px solid ${
            isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"
          }`,
        }}
      >
        <div>
          <p
            className="text-sm font-medium"
            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
          >
            {t("plugins.permissions")}
          </p>
          <p
            className="text-xs mt-0.5"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            {t("settings.tabDescriptions.plugins")}
          </p>
        </div>
        <button
          onClick={handleRefresh}
          disabled={isLoading}
          className="flex items-center gap-2 px-4 py-2 text-xs font-medium rounded-lg transition-colors"
          style={{
            backgroundColor: isDark
              ? "rgba(255,255,255,0.05)"
              : "rgba(255,255,255,0.5)",
            color: isDark ? "#cbd5e1" : "#5a5a58",
            border: `1px solid ${
              isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"
            }`,
          }}
        >
          <RefreshCw size={14} className={isLoading ? "animate-spin" : ""} />
          <span>{t("plugins.refresh")}</span>
        </button>
      </div>

      {/* Error message */}
      {error && (
        <div
          className="p-4 rounded-xl"
          style={{
            backgroundColor: "rgba(239, 68, 68, 0.1)",
            border: "1px solid rgba(239, 68, 68, 0.3)",
          }}
        >
          <p className="text-sm text-red-400">{error}</p>
        </div>
      )}

      {/* Loading state */}
      {isLoading && plugins.length === 0 && (
        <div className="flex flex-col items-center justify-center py-12">
          <Loader2 size={32} className="animate-spin text-blue-400" />
          <p
            className="text-sm mt-4"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            {t("plugins.loading")}
          </p>
        </div>
      )}

      {/* Empty state */}
      {!isLoading && plugins.length === 0 && (
        <div className="flex flex-col items-center justify-center py-12">
          <Puzzle size={48} style={{ color: isDark ? "#4b5563" : "#9ca3af" }} />
          <p
            className="text-sm mt-4"
            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
          >
            {t("plugins.noPlugins")}
          </p>
          <p
            className="text-xs mt-1"
            style={{ color: isDark ? "#64748b" : "#9a9a98" }}
          >
            {t("plugins.noPluginsHint")}
          </p>
        </div>
      )}

      {/* Plugin list */}
      {plugins.length > 0 && (
        <div className="flex-1 min-h-0 overflow-hidden">
          <div className="overflow-y-auto h-full">
            <div className="space-y-2">
              {plugins.map((plugin) => {
                const effectiveState =
                  pluginStateOverrides[plugin.id] || plugin.state;
                const stateKey = getStateKey(effectiveState);
                const isActive = stateKey === "active";
                const isLoaded =
                  stateKey === "loaded" ||
                  stateKey === "active" ||
                  stateKey === "inactive" ||
                  stateKey === "pending-permission";
                const canUnload = isLoaded;

                return (
                  <div
                    key={plugin.id}
                    onClick={() => handleSelectPlugin(plugin.id)}
                    className="p-4 rounded-xl cursor-pointer transition-colors"
                    style={{
                      backgroundColor: isDark
                        ? "rgba(255,255,255,0.05)"
                        : "rgba(255,255,255,0.6)",
                      border: `1px solid ${
                        isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"
                      }`,
                    }}
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex-1">
                        <div className="flex items-center gap-2 flex-wrap">
                          <h3
                            className="text-sm font-medium"
                            style={{
                              color: isDark ? "#cbd5e1" : "#5a5a58",
                            }}
                          >
                            {plugin.name}
                          </h3>
                          <span
                            className="text-[10px] px-2 py-0.5 rounded-lg"
                            style={{
                              backgroundColor: isActive
                                ? "rgba(34, 197, 94, 0.15)"
                                : isLoaded
                                  ? "rgba(59, 130, 246, 0.15)"
                                  : "rgba(100, 116, 139, 0.15)",
                              color: isActive
                                ? "#22c55e"
                                : isLoaded
                                  ? "#3b82f6"
                                  : "#64748b",
                              border: `1px solid ${
                                isActive
                                  ? "rgba(34, 197, 94, 0.3)"
                                  : isLoaded
                                    ? "rgba(59, 130, 246, 0.3)"
                                    : "rgba(100, 116, 139, 0.2)"
                              }`,
                            }}
                          >
                            {getStateLabel(effectiveState)}
                          </span>
                        </div>
                        <p
                          className="text-xs mt-1"
                          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
                        >
                          {plugin.description}
                        </p>
                        <p
                          className="text-[10px] mt-1"
                          style={{ color: isDark ? "#475569" : "#a1a1aa" }}
                        >
                          v{plugin.version} • {plugin.author}
                        </p>
                      </div>
                      <div className="flex items-center gap-2 ml-2 flex-wrap justify-end">
                        <button
                          onClick={async (e) => {
                            e.stopPropagation();
                            await handleSelectPlugin(plugin.id);
                          }}
                          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg transition-colors"
                          title={t("plugins.configureShortcuts", "Shortcuts")}
                          style={{
                            backgroundColor: isDark
                              ? "rgba(59, 130, 246, 0.12)"
                              : "rgba(59, 130, 246, 0.08)",
                            color: "#3b82f6",
                            border: "1px solid rgba(59, 130, 246, 0.28)",
                          }}
                        >
                          <Keyboard size={13} />
                          <span>{t("plugins.configureShortcuts", "Shortcuts")}</span>
                        </button>
                        <button
                          onClick={async (e) => {
                            e.stopPropagation();
                            if (!isLoaded) {
                              await handleLoadPlugin(plugin.id);
                            } else if (isActive) {
                              await deactivatePlugin(plugin.id);
                              setPluginStateOverride(plugin.id, "inactive");
                              await refresh();
                            } else {
                              await activatePlugin(plugin.id);
                              setPluginStateOverride(plugin.id, "active");
                              await refresh();
                            }
                          }}
                          className="px-3 py-1.5 text-xs font-medium rounded-lg transition-colors"
                          style={{
                            backgroundColor: isActive
                              ? "rgba(239, 68, 68, 0.15)"
                              : "rgba(34, 197, 94, 0.15)",
                            color: isActive ? "#ef4444" : "#22c55e",
                            border: `1px solid ${
                              isActive
                                ? "rgba(239, 68, 68, 0.3)"
                                : "rgba(34, 197, 94, 0.3)"
                            }`,
                          }}
                        >
                          {getActionLabel(isLoaded, isActive)}
                        </button>
                        {canUnload && (
                          <button
                            onClick={async (e) => {
                              e.stopPropagation();
                              await handleUnloadPlugin(plugin.id);
                              await refresh();
                            }}
                            className="px-3 py-1.5 text-xs font-medium rounded-lg transition-colors"
                            style={{
                              backgroundColor: "rgba(239, 68, 68, 0.12)",
                              color: "#ef4444",
                              border: "1px solid rgba(239, 68, 68, 0.28)",
                            }}
                          >
                            {t("plugins.uninstall")}
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}

      {/* Plugin Detail Modal */}
      {selectedPluginDetail && (
        <PluginDetailModal
          detail={selectedPluginDetail}
          onClose={() => setSelectedPluginDetail(null)}
          onSaveConfig={async (config) => {
            if (selectedPluginDetail.manifest.id) {
              await updatePluginConfig(
                selectedPluginDetail.manifest.id,
                config,
              );
              await syncGlobalShortcutConfig(selectedPluginDetail, config);
              // Refresh the detail
              const updatedDetail = await getPluginDetail(
                selectedPluginDetail.manifest.id,
              );
              setSelectedPluginDetail(updatedDetail);
            }
          }}
        />
      )}

      {/* Permission prompt modal */}
      {showPermissionPrompt && pendingPermissions.length > 0 && (
        <PermissionPrompt
          pluginName={
            plugins.find((p) => p.id === pendingPermissionPluginId)?.name ||
            plugins.find((p) => p.id === selectedPluginId)?.name ||
            "Plugin"
          }
          permissions={pendingPermissions}
          permissionDefinitions={permissionDefinitions}
          onGrant={handleGrantPermission}
          onGrantAll={handleGrantAll}
          onDeny={handleDenyPermissions}
        />
      )}
    </div>
  );
};

export default PluginsTab;
