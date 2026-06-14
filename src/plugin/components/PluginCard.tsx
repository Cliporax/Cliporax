/**
 * Plugin Card Component
 */

import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import {
  Play,
  Pause,
  Power,
  Settings,
  AlertCircle,
  CheckCircle,
  Clock,
  Shield,
} from "lucide-react";
import type { PluginInfo, PluginState } from "../types";
import { usePlugin } from "../context/PluginContext";
import { createLogger } from "../../utils/logger";

const logger = createLogger("PluginCard");

interface PluginCardProps {
  plugin: PluginInfo;
  isSelected?: boolean;
  onClick?: () => void;
}

export const PluginCard: React.FC<PluginCardProps> = ({
  plugin,
  isSelected,
  onClick,
}) => {
  const { t } = useTranslation();
  const { loadPlugin, activatePlugin, deactivatePlugin } = usePlugin();
  const [isLoading, setIsLoading] = useState(false);

  const handleAction = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsLoading(true);

    try {
      if (plugin.state === "discovered" || plugin.state === "unloaded") {
        const result = await loadPlugin(plugin.id);
        if ("permissionRequired" in result) {
          logger.info(
            "Plugin requires permissions:",
            result.permissionRequired,
          );
        }
      } else if (plugin.state === "loaded" || plugin.state === "inactive") {
        await activatePlugin(plugin.id);
      } else if (plugin.state === "active") {
        await deactivatePlugin(plugin.id);
      }
    } catch (err) {
      logger.error("Action failed:", err);
    } finally {
      setIsLoading(false);
    }
  };

  const stateInfo = getStateInfo(plugin.state, t);
  const StateIcon = stateInfo.icon;

  return (
    <div
      style={{
        ...styles.card,
        ...(isSelected && styles.cardSelected),
        borderLeftColor: stateInfo.color,
      }}
      onClick={onClick}
    >
      <div style={styles.header}>
        <div style={styles.titleRow}>
          <h3 style={styles.name}>{plugin.name}</h3>
          <span style={styles.version}>v{plugin.version}</span>
        </div>
        <div style={styles.meta}>
          <span style={styles.author}>{plugin.author}</span>
          <span style={styles.type}>{plugin.type}</span>
        </div>
      </div>

      <p style={styles.description}>{plugin.description}</p>

      <div style={styles.footer}>
        <div style={styles.state}>
          <StateIcon size={14} style={{ color: stateInfo.color }} />
          <span style={{ ...styles.stateText, color: stateInfo.color }}>
            {stateInfo.label}
          </span>
        </div>

        <div style={styles.actions}>
          {plugin.permissions.length > 0 && (
            <div style={styles.permissionBadge}>
              <Shield size={12} />
              <span>{plugin.permissions.length}</span>
            </div>
          )}

          <button
            style={{
              ...styles.actionButton,
              ...(isLoading && styles.actionButtonLoading),
            }}
            onClick={handleAction}
            disabled={isLoading}
            title={stateInfo.actionLabel}
          >
            {isLoading ? (
              <Clock size={16} style={styles.spinning} />
            ) : stateInfo.state === "active" ? (
              <Pause size={16} />
            ) : (
              <Play size={16} />
            )}
          </button>

          {isSelected && (
            <button
              style={styles.settingsButton}
              onClick={(e) => e.stopPropagation()}
              title={t('plugins.settings')}
            >
              <Settings size={16} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
};

/**
 * Get state info for display
 */
function getStateInfo(state: PluginState, t: TFunction): {
  icon: typeof CheckCircle;
  label: string;
  color: string;
  actionLabel: string;
  state: string;
} {
  if (typeof state === "object" && "error" in state) {
    return {
      icon: AlertCircle,
      label: t('common.error', 'Error'),
      color: "#ef4444",
      actionLabel: t('common.retry', 'Retry'),
      state: "error",
    };
  }

  switch (state) {
    case "discovered":
      return {
        icon: Clock,
        label: t('plugins.state.discovered', 'Discovered'),
        color: "#888",
        actionLabel: t('plugins.load', 'Load'),
        state: "discovered",
      };
    case "validated":
      return {
        icon: CheckCircle,
        label: t('plugins.state.validated', 'Validated'),
        color: "#3b82f6",
        actionLabel: t('plugins.load', 'Load'),
        state: "validated",
      };
    case "loaded":
      return {
        icon: CheckCircle,
        label: t('plugins.state.loaded', 'Loaded'),
        color: "#22c55e",
        actionLabel: t('plugins.activate', 'Activate'),
        state: "loaded",
      };
    case "pending-permission":
      return {
        icon: Shield,
        label: t('plugins.state.pendingPermission', 'Pending Permission'),
        color: "#f59e0b",
        actionLabel: t('plugins.grantPermission', 'Grant Permission'),
        state: "pending-permission",
      };
    case "active":
      return {
        icon: Play,
        label: t('plugins.state.active', 'Active'),
        color: "#22c55e",
        actionLabel: t('plugins.deactivate', 'Deactivate'),
        state: "active",
      };
    case "inactive":
      return {
        icon: Pause,
        label: t('plugins.state.inactive', 'Inactive'),
        color: "#888",
        actionLabel: t('plugins.activate', 'Activate'),
        state: "inactive",
      };
    case "unloaded":
      return {
        icon: Power,
        label: t('plugins.state.unloaded', 'Unloaded'),
        color: "#888",
        actionLabel: t('plugins.load', 'Load'),
        state: "unloaded",
      };
    default:
      return {
        icon: Clock,
        label: t('plugins.state.unknown', 'Unknown'),
        color: "#888",
        actionLabel: t('plugins.load', 'Load'),
        state: "unknown",
      };
  }
}

const styles: Record<string, React.CSSProperties> = {
  card: {
    padding: "16px",
    backgroundColor: "#1f2937",
    borderRadius: "8px",
    border: "1px solid #333",
    borderLeftWidth: "4px",
    borderLeftColor: "#888",
    cursor: "pointer",
    transition: "all 0.2s",
  },
  cardSelected: {
    backgroundColor: "#374151",
    borderColor: "#3b82f6",
  },
  header: {
    marginBottom: "8px",
  },
  titleRow: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    marginBottom: "4px",
  },
  name: {
    fontSize: "16px",
    fontWeight: 600,
    color: "#fff",
    margin: 0,
  },
  version: {
    fontSize: "12px",
    color: "#888",
    padding: "2px 6px",
    backgroundColor: "#374151",
    borderRadius: "4px",
  },
  meta: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
  },
  author: {
    fontSize: "13px",
    color: "#888",
  },
  type: {
    fontSize: "12px",
    color: "#888",
    textTransform: "capitalize",
  },
  description: {
    fontSize: "14px",
    color: "#aaa",
    margin: "0 0 12px 0",
    lineHeight: 1.5,
  },
  footer: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  state: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
  },
  stateText: {
    fontSize: "13px",
    fontWeight: 500,
  },
  actions: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
  },
  permissionBadge: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
    padding: "4px 8px",
    fontSize: "12px",
    color: "#f59e0b",
    backgroundColor: "rgba(245, 158, 11, 0.1)",
    borderRadius: "4px",
  },
  actionButton: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: "32px",
    height: "32px",
    padding: 0,
    backgroundColor: "#3b82f6",
    color: "#fff",
    border: "none",
    borderRadius: "6px",
    cursor: "pointer",
    transition: "all 0.2s",
  },
  actionButtonLoading: {
    opacity: 0.7,
    cursor: "wait",
  },
  settingsButton: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: "32px",
    height: "32px",
    padding: 0,
    backgroundColor: "transparent",
    color: "#888",
    border: "1px solid #333",
    borderRadius: "6px",
    cursor: "pointer",
    transition: "all 0.2s",
  },
  spinning: {
    animation: "spin 1s linear infinite",
  },
};
