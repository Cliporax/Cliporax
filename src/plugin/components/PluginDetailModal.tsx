/**
 * Plugin Detail Modal Component
 * Draggable plugin detail modal window
 */

import React, { useState, useRef, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useTheme } from "../../contexts/ThemeContext";
import { useToast } from "../../components/Toast";
import { Combobox } from "../../components/Combobox";
import {
  X,
  Shield,
  Settings,
  GripVertical,
  Keyboard,
  Globe,
  Monitor,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import type {
  PluginDetail as PluginDetailType,
  ConfigField,
} from "../../plugin/types";
import { createLogger } from "../../utils/logger";

const logger = createLogger("PluginDetailModal");

// Config field type definition using ConfigField imported from types
export type PluginConfigField = ConfigField;

interface PluginDetailModalProps {
  detail: PluginDetailType;
  onClose: () => void;
  onSaveConfig?: (config: Record<string, any>) => Promise<void>;
}

// Initial window size
const INITIAL_WIDTH = 700;
const INITIAL_HEIGHT = 600;
const MIN_WIDTH = 500;
const MIN_HEIGHT = 400;

export const PluginDetailModal: React.FC<PluginDetailModalProps> = ({
  detail,
  onClose,
  onSaveConfig,
}) => {
  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  const toast = useToast();
  const isDark = resolvedTheme === "dark";
  const { manifest, grantedPermissions, config } = detail;

  // Window position and size - initially centered
  const [position, setPosition] = useState(() => ({
    x: Math.max(0, (window.innerWidth - INITIAL_WIDTH) / 2),
    y: Math.max(0, (window.innerHeight - INITIAL_HEIGHT) / 2),
  }));
  const [size, setSize] = useState({
    width: INITIAL_WIDTH,
    height: INITIAL_HEIGHT,
  });
  const [isDragging, setIsDragging] = useState(false);
  const [isResizing, setIsResizing] = useState(false);
  const dragStartRef = useRef({ x: 0, y: 0 });
  const resizeStartRef = useRef({ x: 0, y: 0, width: 0, height: 0 });

  // Configuration state
  const [configValues, setConfigValues] = useState<Record<string, any>>({});
  const [configSections, setConfigSections] = useState<Record<string, boolean>>(
    {
      permissions: true,
      config: true,
    },
  );

  // Shortcut recording state
  const [recordingShortcut, setRecordingShortcut] = useState<string | null>(
    null,
  );

  // Fetch the config schema
  const configSchema = manifest.configSchema as any;
  const configFields: PluginConfigField[] = configSchema?.fields || [];

  // Initialize config values
  useEffect(() => {
    const initialValues: Record<string, any> = {};
    configFields.forEach((field) => {
      initialValues[field.key] = (config as any)?.[field.key] ?? field.default;
    });
    setConfigValues(initialValues);
  }, [config, configFields]);

  // Handle dragging
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (
        e.target === e.currentTarget ||
        (e.target as HTMLElement).closest(".modal-header")
      ) {
        setIsDragging(true);
        dragStartRef.current = {
          x: e.clientX - position.x,
          y: e.clientY - position.y,
        };
      }
    },
    [position],
  );

  // Handle resizing
  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      setIsResizing(true);
      resizeStartRef.current = {
        x: e.clientX,
        y: e.clientY,
        width: size.width,
        height: size.height,
      };
    },
    [size],
  );

  // Global mouse event listeners
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging) {
        let newX = e.clientX - dragStartRef.current.x;
        let newY = e.clientY - dragStartRef.current.y;

        // Constrain within the window viewport
        const maxX = Math.max(0, window.innerWidth - size.width);
        const maxY = Math.max(0, window.innerHeight - size.height);
        newX = Math.max(0, Math.min(newX, maxX));
        newY = Math.max(0, Math.min(newY, maxY));

        setPosition({ x: newX, y: newY });
      }
      if (isResizing) {
        const newWidth = Math.max(
          MIN_WIDTH,
          resizeStartRef.current.width + (e.clientX - resizeStartRef.current.x),
        );
        const newHeight = Math.max(
          MIN_HEIGHT,
          resizeStartRef.current.height +
            (e.clientY - resizeStartRef.current.y),
        );
        setSize({ width: newWidth, height: newHeight });
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      setIsResizing(false);
    };

    if (isDragging || isResizing) {
      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);
    }

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, isResizing, size]);

  // Listen for window size changes and constrain the modal position
  useEffect(() => {
    const handleResize = () => {
      // Constrain the modal position inside the visible area
      setPosition((prev) => {
        const maxX = Math.max(0, window.innerWidth - size.width);
        const maxY = Math.max(0, window.innerHeight - size.height);
        return {
          x: Math.max(0, Math.min(prev.x, maxX)),
          y: Math.max(0, Math.min(prev.y, maxY)),
        };
      });
    };

    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [size.width, size.height]);

  // Recenter when switching plugins
  useEffect(() => {
    const centerX = Math.max(0, (window.innerWidth - size.width) / 2);
    const centerY = Math.max(0, (window.innerHeight - size.height) / 2);
    setPosition({ x: centerX, y: centerY });
    // Reset window size
    setSize({ width: INITIAL_WIDTH, height: INITIAL_HEIGHT });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.manifest.id]);

  // Handle config changes
  const handleConfigChange = (key: string, value: any) => {
    setConfigValues((prev) => ({ ...prev, [key]: value }));
  };

  // Save config
  const handleSave = async () => {
    try {
      await onSaveConfig?.(configValues);
      onClose();
    } catch (error) {
      logger.error("Failed to save plugin config:", error);
      toast.error(t("plugins.detail.saveError", { error: String(error) }));
    }
  };

  // Shortcut recording
  const handleShortcutKeyDown = (e: React.KeyboardEvent, fieldKey: string) => {
    e.preventDefault();

    if (e.key === "Escape") {
      setRecordingShortcut(null);
      return;
    }

    const modifiers: string[] = [];
    if (e.ctrlKey) modifiers.push("Ctrl");
    if (e.altKey) modifiers.push("Alt");
    if (e.shiftKey) modifiers.push("Shift");
    if (e.metaKey) modifiers.push("Cmd");

    const keyName = e.key.toUpperCase();
    if (["CONTROL", "ALT", "SHIFT", "META"].includes(keyName)) return;

    const shortcut =
      modifiers.length > 0 ? `${modifiers.join("+")}+${keyName}` : keyName;
    handleConfigChange(fieldKey, shortcut);
    setRecordingShortcut(null);
  };

  const toggleSection = (section: string) => {
    setConfigSections((prev) => ({ ...prev, [section]: !prev[section] }));
  };

  // Render config fields
  const renderConfigField = (field: PluginConfigField) => {
    const value = configValues[field.key];

    switch (field.type) {
      case "text":
        return (
          <input
            type="text"
            value={value || ""}
            onChange={(e) => handleConfigChange(field.key, e.target.value)}
            className="w-full px-3 py-2 rounded-lg text-sm outline-none transition-all"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.7)",
              border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
              color: isDark ? "#e2e8f0" : "#4a4a48",
            }}
            placeholder={field.default || ""}
          />
        );

      case "number":
        return (
          <input
            type="number"
            value={value || ""}
            onChange={(e) =>
              handleConfigChange(field.key, Number(e.target.value))
            }
            className="w-full px-3 py-2 rounded-lg text-sm outline-none transition-all"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.7)",
              border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
              color: isDark ? "#e2e8f0" : "#4a4a48",
            }}
            min={field.min}
            max={field.max}
            placeholder={field.default?.toString() || ""}
          />
        );

      case "select":
        return (
          <Combobox
            value={String(value || "")}
            options={[
              { value: "", label: t("common.select", "Select...") },
              ...(field.options ?? []),
            ]}
            placeholder={t("common.select", "Select...")}
            ariaLabel={field.label}
            theme={resolvedTheme}
            onChange={(nextValue) => handleConfigChange(field.key, nextValue)}
          />
        );

      case "boolean":
        return (
          <button
            onClick={() => handleConfigChange(field.key, !value)}
            className="w-11 h-6 rounded-full transition-all"
            style={{
              backgroundColor: value
                ? "#3b82f6"
                : isDark
                  ? "#475569"
                  : "#c4c4c2",
            }}
          >
            <div
              className="w-4 h-4 bg-white rounded-full shadow transition-transform"
              style={{
                transform: value ? "translateX(22px)" : "translateX(4px)",
              }}
            />
          </button>
        );

      case "shortcut":
        const isGlobal = field.global ?? false;
        return (
          <div className="flex items-center gap-3">
            <button
              className={`flex-1 px-3 py-2 rounded-lg text-xs font-mono transition-all border text-left ${
                recordingShortcut === field.key ? "border-blue-500" : ""
              }`}
              style={{
                backgroundColor:
                  recordingShortcut === field.key
                    ? isDark
                      ? "rgba(59,130,246,0.2)"
                      : "rgba(59,130,246,0.08)"
                    : isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(255,255,255,0.5)",
                borderColor:
                  recordingShortcut === field.key
                    ? "rgba(59,130,246,0.5)"
                    : isDark
                      ? "rgba(255,255,255,0.1)"
                      : "rgba(0,0,0,0.06)",
                color:
                  recordingShortcut === field.key
                    ? "#3b82f6"
                    : isDark
                      ? "#94a3b8"
                      : "#6b6b69",
              }}
              onClick={() => setRecordingShortcut(field.key)}
              onKeyDown={(e) => handleShortcutKeyDown(e, field.key)}
              tabIndex={0}
            >
              {recordingShortcut === field.key
                ? t("settings.shortcuts.pressKeys", "Press keys...")
                : value || t("settings.shortcuts.notSet", "Not set")}
            </button>
            <div
              className="flex items-center gap-1 text-xs"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {isGlobal ? <Globe size={14} /> : <Monitor size={14} />}
              <span>
                {isGlobal
                  ? t("plugins.detail.global", "Global")
                  : t("plugins.detail.local", "App")}
              </span>
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div
      className="fixed z-50"
      style={{
        left: position.x,
        top: position.y,
        width: size.width,
        height: size.height,
        cursor: isDragging ? "grabbing" : "default",
      }}
    >
      {/* Window shadow backdrop */}
      <div
        className="absolute inset-0 rounded-2xl shadow-2xl"
        style={{
          backgroundColor: isDark
            ? "rgba(15,23,42,0.98)"
            : "rgba(252,251,249,0.98)",
          border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
          boxShadow: isDark
            ? "0 25px 50px -12px rgba(0,0,0,0.5)"
            : "0 25px 50px -12px rgba(0,0,0,0.08)",
        }}
      />

      {/* Draggable area */}
      <div
        className="modal-header absolute inset-x-0 top-0 h-14 cursor-grab active:cursor-grabbing"
        style={{
          borderBottom: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
        }}
        onMouseDown={handleMouseDown}
      />

      {/* Window content */}
      <div className="relative z-10 flex flex-col h-full">
        {/* Title bar */}
        <div
          className="flex items-center justify-between px-5 py-3"
          style={{
            borderBottom: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
          }}
        >
          <div className="flex items-center gap-3">
            <GripVertical
              size={16}
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            />
            <h2
              className="text-sm font-semibold"
              style={{ color: isDark ? "#e2e8f0" : "#4a4a48" }}
            >
              {manifest.name}
            </h2>
            <span
              className="text-xs px-2 py-0.5 rounded-md"
              style={{
                backgroundColor: isDark
                  ? "rgba(59,130,246,0.15)"
                  : "rgba(59,130,246,0.08)",
                color: "#3b82f6",
              }}
            >
              v{manifest.version}
            </span>
          </div>
          <button
            onClick={onClose}
            className="p-2 rounded-xl transition-all hover:scale-105"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(0,0,0,0.04)",
              color: isDark ? "#94a3b8" : "#8a8a88",
            }}
          >
            <X size={16} />
          </button>
        </div>

        {/* Content area */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          {/* Plugin description */}
          <p
            className="text-xs"
            style={{ color: isDark ? "#94a3b8" : "#7a7a78" }}
          >
            {manifest.description}
          </p>

          {/* Permissions section */}
          {manifest.permissions.length > 0 && (
            <div
              className="rounded-xl overflow-hidden"
              style={{
                backgroundColor: isDark
                  ? "rgba(255,255,255,0.03)"
                  : "rgba(255,255,255,0.5)",
                border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
              }}
            >
              <button
                className="w-full flex items-center justify-between px-4 py-3"
                onClick={() => toggleSection("permissions")}
                style={{
                  borderBottom: configSections.permissions
                    ? `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`
                    : "none",
                }}
              >
                <div className="flex items-center gap-2">
                  <Shield size={14} style={{ color: "#3b82f6" }} />
                  <span
                    className="text-sm font-medium"
                    style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
                  >
                    {t("plugins.permissions", "Permissions")} (
                    {manifest.permissions.length})
                  </span>
                </div>
                {configSections.permissions ? (
                  <ChevronUp size={16} />
                ) : (
                  <ChevronDown size={16} />
                )}
              </button>

              {configSections.permissions && (
                <div className="px-4 pb-4 space-y-2">
                  {manifest.permissions.map((perm) => {
                    const isGranted = grantedPermissions.includes(
                      perm.permission,
                    );
                    return (
                      <div
                        key={perm.permission}
                        className="flex items-center justify-between text-xs p-3 rounded-lg"
                        style={{
                          backgroundColor: isDark
                            ? "rgba(255,255,255,0.03)"
                            : "rgba(255,255,255,0.4)",
                        }}
                      >
                        <div className="flex-1">
                          <div
                            className="font-medium"
                            style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
                          >
                            {perm.permission}
                          </div>
                          {perm.reason && (
                            <div
                              className="mt-1"
                              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
                            >
                              {perm.reason}
                            </div>
                          )}
                        </div>
                        <span
                          className="px-2 py-1 rounded-md text-xs font-medium ml-2"
                          style={{
                            backgroundColor: isGranted
                              ? "rgba(34,197,94,0.15)"
                              : "rgba(239,68,68,0.15)",
                            color: isGranted ? "#22c55e" : "#ef4444",
                          }}
                        >
                          {isGranted
                            ? t("plugins.detail.granted", "Granted")
                            : t("plugins.detail.notGranted", "Not Granted")}
                        </span>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {/* Configuration section */}
          {configFields.length > 0 && (
            <div
              className="rounded-xl overflow-hidden"
              style={{
                backgroundColor: isDark
                  ? "rgba(255,255,255,0.03)"
                  : "rgba(255,255,255,0.5)",
                border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
              }}
            >
              <button
                className="w-full flex items-center justify-between px-4 py-3"
                onClick={() => toggleSection("config")}
                style={{
                  borderBottom: configSections.config
                    ? `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`
                    : "none",
                }}
              >
                <div className="flex items-center gap-2">
                  <Settings size={14} style={{ color: "#3b82f6" }} />
                  <span
                    className="text-sm font-medium"
                    style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
                  >
                    {t("plugins.detail.configuration", "Configuration")}
                  </span>
                </div>
                {configSections.config ? (
                  <ChevronUp size={16} />
                ) : (
                  <ChevronDown size={16} />
                )}
              </button>

              {configSections.config && (
                <div className="px-4 pb-4 space-y-3">
                  {configFields.map((field) => (
                    <div key={field.key}>
                      <label
                        className="block text-xs font-medium mb-1.5"
                        style={{ color: isDark ? "#cbd5e1" : "#5a5a58" }}
                      >
                        {field.label}
                        {field.type === "shortcut" && (
                          <Keyboard size={12} className="inline ml-1" />
                        )}
                      </label>
                      {field.description && (
                        <p
                          className="text-[10px] mb-2"
                          style={{ color: isDark ? "#64748b" : "#9a9a98" }}
                        >
                          {field.description}
                        </p>
                      )}
                      {renderConfigField(field)}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Show a hint when there is no configuration */}
          {configFields.length === 0 && (
            <div className="text-center py-8">
              <Settings
                size={32}
                style={{
                  color: isDark ? "#475569" : "#a1a1aa",
                  margin: "0 auto 12px",
                }}
              />
              <p
                className="text-xs"
                style={{ color: isDark ? "#64748b" : "#9a9a98" }}
              >
                {t("plugins.detail.noConfig", "No configuration available")}
              </p>
            </div>
          )}
        </div>

        {/* Bottom action bar */}
        <div
          className="px-5 py-3 flex items-center justify-end gap-2"
          style={{
            borderTop: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
          }}
        >
          <button
            onClick={onClose}
            className="px-4 py-2 text-xs font-medium rounded-lg transition-all"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.6)",
              color: isDark ? "#94a3b8" : "#6b6b69",
              border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
            }}
          >
            {t("common.cancel", "Cancel")}
          </button>
          {configFields.length > 0 && (
            <button
              onClick={handleSave}
              className="px-4 py-2 text-xs font-medium rounded-lg transition-all"
              style={{
                backgroundColor: "#3b82f6",
                color: "#fff",
              }}
            >
              {t("common.save", "Save")}
            </button>
          )}
        </div>
      </div>

      {/* Bottom-right resize handle */}
      <div
        className="absolute bottom-0 right-0 w-6 h-6 cursor-se-resize z-20"
        onMouseDown={handleResizeMouseDown}
      >
        <svg
          width="12"
          height="12"
          viewBox="0 0 12 12"
          style={{ position: "absolute", right: 4, bottom: 4 }}
        >
          <path
            d="M10 2L2 10M10 6L6 10M10 10L10 10"
            stroke={isDark ? "#64748b" : "#9a9a98"}
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        </svg>
      </div>
    </div>
  );
};
