/**
 * Plugin List Component
 */

import React from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw, Loader2, Package } from "lucide-react";
import { usePlugin } from "../context/PluginContext";
import { PluginCard } from "./PluginCard";
import { createLogger } from "../../utils/logger";

const logger = createLogger("PluginList");

interface PluginListProps {
  onSelectPlugin?: (pluginId: string) => void;
  selectedPluginId?: string | null;
}

export const PluginList: React.FC<PluginListProps> = ({
  onSelectPlugin,
  selectedPluginId,
}) => {
  const { t } = useTranslation();
  const { plugins, isLoading, error, refresh, clearError } = usePlugin();

  const handleRefresh = async () => {
    logger.debug("Refreshing plugin list");
    await refresh();
  };

  if (error) {
    return (
      <div style={styles.errorContainer}>
        <p style={styles.errorText}>{error}</p>
        <button style={styles.retryButton} onClick={clearError}>
          {t('common.close', 'Close')}
        </button>
      </div>
    );
  }

  if (isLoading && plugins.length === 0) {
    return (
      <div style={styles.loadingContainer}>
        <Loader2 size={24} style={styles.spinner} />
        <p style={styles.loadingText}>{t('plugins.loading')}</p>
      </div>
    );
  }

  if (plugins.length === 0) {
    return (
      <div style={styles.emptyContainer}>
        <Package size={48} style={styles.emptyIcon} />
        <p style={styles.emptyText}>{t('plugins.noPlugins')}</p>
        <p style={styles.emptySubtext}>
          {t('plugins.noPluginsHint')}
        </p>
        <button style={styles.refreshButton} onClick={handleRefresh}>
          <RefreshCw size={16} />
          <span>{t('plugins.refresh', 'Refresh')}</span>
        </button>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <span style={styles.count}>{t('plugins.count', '{{count}} plugins', { count: plugins.length })}</span>
        <button
          style={styles.refreshButton}
          onClick={handleRefresh}
          disabled={isLoading}
        >
          <RefreshCw
            size={16}
            style={isLoading ? styles.spinning : undefined}
          />
          <span>{t('plugins.refresh', 'Refresh')}</span>
        </button>
      </div>

      <div style={styles.list}>
        {plugins.map((plugin) => (
          <PluginCard
            key={plugin.id}
            plugin={plugin}
            isSelected={selectedPluginId === plugin.id}
            onClick={() => onSelectPlugin?.(plugin.id)}
          />
        ))}
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    flexDirection: "column",
    gap: "12px",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0 4px",
  },
  count: {
    fontSize: "14px",
    color: "#888",
  },
  list: {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
  },
  loadingContainer: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    padding: "48px",
    gap: "12px",
  },
  spinner: {
    animation: "spin 1s linear infinite",
  },
  spinning: {
    animation: "spin 1s linear infinite",
  },
  loadingText: {
    fontSize: "14px",
    color: "#888",
  },
  emptyContainer: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    padding: "48px",
    gap: "8px",
  },
  emptyIcon: {
    color: "#555",
  },
  emptyText: {
    fontSize: "16px",
    fontWeight: 500,
    color: "#ccc",
  },
  emptySubtext: {
    fontSize: "14px",
    color: "#888",
  },
  errorContainer: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    padding: "24px",
    gap: "12px",
    backgroundColor: "rgba(239, 68, 68, 0.1)",
    borderRadius: "8px",
    border: "1px solid rgba(239, 68, 68, 0.3)",
  },
  errorText: {
    fontSize: "14px",
    color: "#ef4444",
    textAlign: "center",
  },
  retryButton: {
    padding: "8px 16px",
    fontSize: "14px",
    backgroundColor: "#3b82f6",
    color: "#fff",
    border: "none",
    borderRadius: "6px",
    cursor: "pointer",
  },
  refreshButton: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    padding: "6px 12px",
    fontSize: "13px",
    backgroundColor: "transparent",
    color: "#888",
    border: "1px solid #333",
    borderRadius: "6px",
    cursor: "pointer",
    transition: "all 0.2s",
  },
};
