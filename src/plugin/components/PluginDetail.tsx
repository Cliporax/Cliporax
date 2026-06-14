/**
 * Plugin Detail Component
 */

import React from "react";
import { useTranslation } from "react-i18next";
import {
  X,
  ExternalLink,
  Shield,
  Activity,
  Settings,
  Info,
  Calendar,
  Clock,
} from "lucide-react";
import type { PluginDetail } from "../types";
import {
  getCategoryLabel,
  getRiskLevelColor,
  getRiskLevelLabel,
} from "../types/permission";
import { createLogger } from "../../utils/logger";

const logger = createLogger("PluginDetail");

interface PluginDetailPanelProps {
  detail: PluginDetail;
  onClose: () => void;
}

export const PluginDetailPanel: React.FC<PluginDetailPanelProps> = ({
  detail,
  onClose,
}) => {
  const { t } = useTranslation();
  const { manifest, state, grantedPermissions, statistics } = detail;

  const isStateError = typeof state === "object" && "error" in state;
  const stateLabel = isStateError
    ? `${t('common.error', 'Error')}: ${(state as { error: string }).error}`
    : state;

  return (
    <div style={styles.panel}>
      <div style={styles.header}>
        <h2 style={styles.title}>{manifest.name}</h2>
        <button style={styles.closeButton} onClick={onClose}>
          <X size={20} />
        </button>
      </div>

      <div style={styles.content}>
        {/* Basic Info */}
        <section style={styles.section}>
          <h3 style={styles.sectionTitle}>
            <Info size={16} />
            <span>{t('plugins.detail.information', 'Information')}</span>
          </h3>
          <div style={styles.infoGrid}>
            <div style={styles.infoItem}>
              <span style={styles.infoLabel}>{t('plugins.detail.id', 'ID')}</span>
              <span style={styles.infoValue}>{manifest.id}</span>
            </div>
            <div style={styles.infoItem}>
              <span style={styles.infoLabel}>{t('plugins.version')}</span>
              <span style={styles.infoValue}>{manifest.version}</span>
            </div>
            <div style={styles.infoItem}>
              <span style={styles.infoLabel}>{t('plugins.detail.type', 'Type')}</span>
              <span style={styles.infoValue}>{manifest.type}</span>
            </div>
            <div style={styles.infoItem}>
              <span style={styles.infoLabel}>{t('plugins.detail.state', 'State')}</span>
              <span
                style={{
                  ...styles.infoValue,
                  color: isStateError ? "#ef4444" : "#22c55e",
                }}
              >
                {stateLabel}
              </span>
            </div>
          </div>
          <p style={styles.description}>{manifest.description}</p>
        </section>

        {/* Author */}
        <section style={styles.section}>
          <h3 style={styles.sectionTitle}>
            <span>{t('plugins.author')}</span>
          </h3>
          <div style={styles.authorInfo}>
            <span style={styles.authorName}>{manifest.author.name}</span>
            {manifest.author.email && (
              <span style={styles.authorEmail}>{manifest.author.email}</span>
            )}
          </div>
          {manifest.homepage && (
            <a
              href={manifest.homepage}
              target="_blank"
              rel="noopener noreferrer"
              style={styles.link}
            >
              <ExternalLink size={14} />
              <span>{t('plugins.detail.homepage', 'Homepage')}</span>
            </a>
          )}
          {manifest.repository && (
            <a
              href={manifest.repository}
              target="_blank"
              rel="noopener noreferrer"
              style={styles.link}
            >
              <ExternalLink size={14} />
              <span>{t('plugins.detail.repository', 'Repository')}</span>
            </a>
          )}
        </section>

        {/* Permissions */}
        <section style={styles.section}>
          <h3 style={styles.sectionTitle}>
            <Shield size={16} />
            <span>{t('plugins.permissions')} ({manifest.permissions.length})</span>
          </h3>
          {manifest.permissions.length === 0 ? (
            <p style={styles.emptyText}>{t('plugins.detail.noPermissions', 'No permissions required')}</p>
          ) : (
            <div style={styles.permissionList}>
              {manifest.permissions.map((perm) => (
                <div key={perm.permission} style={styles.permissionItem}>
                  <div style={styles.permissionHeader}>
                    <span style={styles.permissionId}>{perm.permission}</span>
                    <span
                      style={{
                        ...styles.statusBadge,
                        backgroundColor: grantedPermissions.includes(
                          perm.permission,
                        )
                          ? "rgba(34, 197, 94, 0.2)"
                          : "rgba(239, 68, 68, 0.2)",
                        color: grantedPermissions.includes(perm.permission)
                          ? "#22c55e"
                          : "#ef4444",
                      }}
                    >
                      {grantedPermissions.includes(perm.permission)
                        ? t('plugins.detail.granted', 'Granted')
                        : t('plugins.detail.notGranted', 'Not Granted')}
                    </span>
                  </div>
                  <p style={styles.permissionReason}>{perm.reason}</p>
                  {perm.required && (
                    <span style={styles.requiredBadge}>{t('common.required', 'Required')}</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>

        {/* Statistics */}
        <section style={styles.section}>
          <h3 style={styles.sectionTitle}>
            <Activity size={16} />
            <span>{t('plugins.detail.statistics', 'Statistics')}</span>
          </h3>
          <div style={styles.statsGrid}>
            <div style={styles.statItem}>
              <span style={styles.statValue}>{statistics.activatedCount}</span>
              <span style={styles.statLabel}>{t('plugins.detail.activations', 'Activations')}</span>
            </div>
            <div style={styles.statItem}>
              <span style={styles.statValue}>
                {Math.round(statistics.totalRuntimeMs / 1000)}s
              </span>
              <span style={styles.statLabel}>{t('plugins.detail.runtime', 'Runtime')}</span>
            </div>
            <div style={styles.statItem}>
              <span style={styles.statValue}>{statistics.errorCount}</span>
              <span style={styles.statLabel}>{t('plugins.detail.errors', 'Errors')}</span>
            </div>
          </div>
          {statistics.lastActivated && (
            <div style={styles.lastActivated}>
              <Calendar size={14} />
              <span>
                {t('plugins.detail.lastActivated', 'Last activated')}:{" "}
                {new Date(statistics.lastActivated).toLocaleString()}
              </span>
            </div>
          )}
        </section>
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  panel: {
    backgroundColor: "#1f2937",
    borderRadius: "12px",
    overflow: "hidden",
    height: "100%",
    display: "flex",
    flexDirection: "column",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "16px 20px",
    borderBottom: "1px solid #333",
  },
  title: {
    fontSize: "18px",
    fontWeight: 600,
    color: "#fff",
    margin: 0,
  },
  closeButton: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: "32px",
    height: "32px",
    padding: 0,
    backgroundColor: "transparent",
    color: "#888",
    border: "none",
    borderRadius: "6px",
    cursor: "pointer",
  },
  content: {
    flex: 1,
    overflow: "auto",
    padding: "20px",
  },
  section: {
    marginBottom: "24px",
  },
  sectionTitle: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    fontSize: "14px",
    fontWeight: 600,
    color: "#888",
    textTransform: "uppercase",
    letterSpacing: "0.5px",
    margin: "0 0 12px 0",
  },
  infoGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(2, 1fr)",
    gap: "12px",
    marginBottom: "12px",
  },
  infoItem: {
    display: "flex",
    flexDirection: "column",
    gap: "4px",
  },
  infoLabel: {
    fontSize: "12px",
    color: "#666",
    textTransform: "uppercase",
  },
  infoValue: {
    fontSize: "14px",
    color: "#fff",
  },
  description: {
    fontSize: "14px",
    color: "#aaa",
    lineHeight: 1.6,
    margin: 0,
  },
  authorInfo: {
    display: "flex",
    flexDirection: "column",
    gap: "4px",
    marginBottom: "12px",
  },
  authorName: {
    fontSize: "15px",
    fontWeight: 500,
    color: "#fff",
  },
  authorEmail: {
    fontSize: "13px",
    color: "#888",
  },
  link: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    fontSize: "13px",
    color: "#3b82f6",
    textDecoration: "none",
    marginBottom: "8px",
  },
  permissionList: {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
  },
  permissionItem: {
    padding: "12px",
    backgroundColor: "#111827",
    borderRadius: "6px",
    border: "1px solid #333",
  },
  permissionHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: "6px",
  },
  permissionId: {
    fontSize: "13px",
    fontFamily: "monospace",
    color: "#fff",
  },
  statusBadge: {
    fontSize: "11px",
    padding: "2px 8px",
    borderRadius: "4px",
  },
  permissionReason: {
    fontSize: "13px",
    color: "#888",
    margin: 0,
  },
  requiredBadge: {
    fontSize: "11px",
    padding: "2px 6px",
    backgroundColor: "rgba(59, 130, 246, 0.2)",
    color: "#3b82f6",
    borderRadius: "4px",
    marginTop: "8px",
    display: "inline-block",
  },
  emptyText: {
    fontSize: "14px",
    color: "#666",
    fontStyle: "italic",
  },
  statsGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(3, 1fr)",
    gap: "12px",
  },
  statItem: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    padding: "16px",
    backgroundColor: "#111827",
    borderRadius: "8px",
  },
  statValue: {
    fontSize: "24px",
    fontWeight: 600,
    color: "#fff",
  },
  statLabel: {
    fontSize: "12px",
    color: "#888",
    marginTop: "4px",
  },
  lastActivated: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    marginTop: "12px",
    fontSize: "13px",
    color: "#888",
  },
};
