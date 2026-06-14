/**
 * Permission Prompt Component
 */

import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Shield, AlertTriangle, Check, X, Info } from "lucide-react";
import type { PermissionRequest, Permission } from "../types";
import {
  getRiskLevelColor,
  getRiskLevelLabel,
  getCategoryLabel,
} from "../types/permission";
import { createLogger } from "../../utils/logger";

const logger = createLogger("PermissionPrompt");

interface PermissionPromptProps {
  pluginName: string;
  permissions: PermissionRequest[];
  permissionDefinitions: Permission[];
  onGrant: (permission: string) => Promise<void>;
  onGrantAll: () => Promise<void>;
  onDeny: () => void;
}

export const PermissionPrompt: React.FC<PermissionPromptProps> = ({
  pluginName,
  permissions,
  permissionDefinitions,
  onGrant,
  onGrantAll,
  onDeny,
}) => {
  const { t } = useTranslation();
  const [grantingPermissions, setGrantingPermissions] = useState<Set<string>>(
    new Set(),
  );
  const [grantedPermissions, setGrantedPermissions] = useState<Set<string>>(
    new Set(),
  );
  const [grantAllReady, setGrantAllReady] = useState(false);

  const getPermissionDefinition = (
    permissionId: string,
  ): Permission | undefined => {
    return permissionDefinitions.find((p) => p.id === permissionId);
  };

  const handleGrant = async (permissionId: string) => {
    setGrantAllReady(false);
    setGrantingPermissions((prev) => new Set(prev).add(permissionId));
    try {
      await onGrant(permissionId);
      setGrantedPermissions((prev) => new Set(prev).add(permissionId));
    } catch (err) {
      logger.error("Failed to grant permission:", err);
    } finally {
      setGrantingPermissions((prev) => {
        const next = new Set(prev);
        next.delete(permissionId);
        return next;
      });
    }
  };

  const handleGrantAll = async () => {
    if (!grantAllReady) {
      setGrantAllReady(true);
      return;
    }

    setGrantingPermissions(
      new Set(permissions.map((permission) => permission.permission)),
    );

    try {
      await onGrantAll();
      setGrantedPermissions(
        new Set(permissions.map((permission) => permission.permission)),
      );
    } catch (err) {
      logger.error("Failed to grant all permissions:", err);
      setGrantAllReady(false);
    } finally {
      setGrantingPermissions(new Set());
    }
  };

  const allGranted = permissions.every((p) =>
    grantedPermissions.has(p.permission),
  );
  const hasDangerous = permissions.some((p) => {
    const def = getPermissionDefinition(p.permission);
    return def?.riskLevel === "high" || def?.riskLevel === "critical";
  });

  return (
    <div style={styles.overlay}>
      <div style={styles.modal}>
        <div style={styles.header}>
          <Shield size={24} style={styles.headerIcon} />
          <div>
            <h2 style={styles.title}>{t('permissions.title')}</h2>
            <p style={styles.subtitle}>
              <strong>{pluginName}</strong> {t('permissions.description', { pluginName })}
            </p>
          </div>
        </div>

        {hasDangerous && (
          <div style={styles.warningBanner}>
            <AlertTriangle size={16} />
            <span>
              Some permissions are marked as dangerous. Review them carefully.
            </span>
          </div>
        )}

        <div style={styles.permissionList}>
          {permissions.map((perm) => {
            const def = getPermissionDefinition(perm.permission);
            const isGranted = grantedPermissions.has(perm.permission);
            const isGranting = grantingPermissions.has(perm.permission);

            return (
              <div
                key={perm.permission}
                style={{
                  ...styles.permissionItem,
                  ...(isGranted && styles.permissionItemGranted),
                }}
              >
                <div style={styles.permissionHeader}>
                  <div style={styles.permissionInfo}>
                    <span style={styles.permissionName}>
                      {def?.name || perm.permission}
                    </span>
                    <span
                      style={{
                        ...styles.riskBadge,
                        backgroundColor: `${getRiskLevelColor(
                          def?.riskLevel || "medium",
                        )}20`,
                        color: getRiskLevelColor(def?.riskLevel || "medium"),
                      }}
                    >
                      {getRiskLevelLabel(def?.riskLevel || "medium")}
                    </span>
                    {perm.required && (
                      <span style={styles.requiredBadge}>{t('common.required', 'Required')}</span>
                    )}
                  </div>

                  {isGranted ? (
                    <div style={styles.grantedBadge}>
                      <Check size={14} />
                      <span>{t('permissions.granted', 'Granted')}</span>
                    </div>
                  ) : (
                    <button
                      style={{
                        ...styles.grantButton,
                        ...(isGranting && styles.grantButtonLoading),
                      }}
                      onClick={() => handleGrant(perm.permission)}
                      disabled={isGranting}
                    >
                      {isGranting ? t('permissions.granting', 'Granting...') : t('permissions.allow')}
                    </button>
                  )}
                </div>

                <p style={styles.permissionReason}>{perm.reason}</p>

                {def && (
                  <div style={styles.permissionMeta}>
                    <span style={styles.categoryBadge}>
                      {getCategoryLabel(def.category || "other")}
                    </span>
                    <span style={styles.permissionId}>{perm.permission}</span>
                  </div>
                )}

                {def?.description && (
                  <p style={styles.permissionDescription}>{def.description}</p>
                )}
              </div>
            );
          })}
        </div>

        <div style={styles.actions}>
          <button style={styles.denyButton} onClick={onDeny}>
            {t('common.cancel')}
          </button>
          <button
            style={{
              ...styles.grantAllButton,
              ...(allGranted && styles.grantAllButtonDisabled),
            }}
            onClick={handleGrantAll}
            disabled={allGranted}
          >
            {allGranted
              ? t('permissions.allGranted', 'All Granted')
              : grantAllReady
                ? t('common.ok', 'OK')
                : t('permissions.allowAll', 'Grant All')}
          </button>
        </div>
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    position: "fixed",
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: "rgba(0, 0, 0, 0.7)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    zIndex: 1000,
  },
  modal: {
    backgroundColor: "#1f2937",
    borderRadius: "12px",
    padding: "24px",
    maxWidth: "560px",
    width: "90%",
    maxHeight: "80vh",
    overflow: "auto",
    boxShadow: "0 20px 40px rgba(0, 0, 0, 0.3)",
  },
  header: {
    display: "flex",
    alignItems: "flex-start",
    gap: "16px",
    marginBottom: "20px",
  },
  headerIcon: {
    color: "#f59e0b",
    flexShrink: 0,
  },
  title: {
    fontSize: "20px",
    fontWeight: 600,
    color: "#fff",
    margin: "0 0 4px 0",
  },
  subtitle: {
    fontSize: "14px",
    color: "#888",
    margin: 0,
  },
  warningBanner: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    padding: "12px",
    marginBottom: "16px",
    backgroundColor: "rgba(239, 68, 68, 0.1)",
    border: "1px solid rgba(239, 68, 68, 0.3)",
    borderRadius: "8px",
    fontSize: "13px",
    color: "#ef4444",
  },
  permissionList: {
    display: "flex",
    flexDirection: "column",
    gap: "12px",
    marginBottom: "20px",
  },
  permissionItem: {
    padding: "16px",
    backgroundColor: "#111827",
    borderRadius: "8px",
    border: "1px solid #333",
  },
  permissionItemGranted: {
    borderColor: "#22c55e",
    backgroundColor: "rgba(34, 197, 94, 0.05)",
  },
  permissionHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "flex-start",
    marginBottom: "8px",
  },
  permissionInfo: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    flexWrap: "wrap",
  },
  permissionName: {
    fontSize: "15px",
    fontWeight: 500,
    color: "#fff",
  },
  riskBadge: {
    fontSize: "11px",
    padding: "2px 6px",
    borderRadius: "4px",
    textTransform: "uppercase",
    fontWeight: 600,
  },
  requiredBadge: {
    fontSize: "11px",
    padding: "2px 6px",
    backgroundColor: "rgba(59, 130, 246, 0.2)",
    color: "#3b82f6",
    borderRadius: "4px",
  },
  grantedBadge: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
    fontSize: "13px",
    color: "#22c55e",
  },
  permissionReason: {
    fontSize: "14px",
    color: "#aaa",
    margin: "0 0 8px 0",
    fontStyle: "italic",
  },
  permissionMeta: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    marginBottom: "8px",
  },
  categoryBadge: {
    fontSize: "11px",
    padding: "2px 6px",
    backgroundColor: "#374151",
    color: "#888",
    borderRadius: "4px",
    textTransform: "uppercase",
  },
  permissionId: {
    fontSize: "12px",
    color: "#666",
    fontFamily: "monospace",
  },
  permissionDescription: {
    fontSize: "13px",
    color: "#888",
    margin: 0,
    lineHeight: 1.5,
  },
  grantButton: {
    padding: "6px 12px",
    fontSize: "13px",
    backgroundColor: "#3b82f6",
    color: "#fff",
    border: "none",
    borderRadius: "6px",
    cursor: "pointer",
    transition: "all 0.2s",
  },
  grantButtonLoading: {
    opacity: 0.7,
    cursor: "wait",
  },
  actions: {
    display: "flex",
    justifyContent: "flex-end",
    gap: "12px",
  },
  denyButton: {
    padding: "10px 20px",
    fontSize: "14px",
    backgroundColor: "transparent",
    color: "#888",
    border: "1px solid #333",
    borderRadius: "8px",
    cursor: "pointer",
    transition: "all 0.2s",
  },
  grantAllButton: {
    padding: "10px 20px",
    fontSize: "14px",
    backgroundColor: "#22c55e",
    color: "#fff",
    border: "none",
    borderRadius: "8px",
    cursor: "pointer",
    transition: "all 0.2s",
  },
  grantAllButtonDisabled: {
    backgroundColor: "#374151",
    color: "#888",
    cursor: "default",
  },
};
