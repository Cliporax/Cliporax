import React, { useState, useRef, useEffect, useCallback } from "react";
import { FolderPlus, Copy, X } from "lucide-react";
import { useTabStore } from "../../../stores/tabStore";
import { useTheme } from "../../../contexts/ThemeContext";
import { clipboard } from "../../../lib/tauri-api";
import { createLogger } from "../../../utils/logger";

const logger = createLogger("MultiSelectBar");

interface MultiSelectBarProps {
  selectedCount: number;
  selectedIds: Set<number>;
  currentTabId: number | null;
  onActionComplete: () => void;
  onCancel: () => void;
}

type PopoverType = "move" | "copy" | null;

export function MultiSelectBar({
  selectedCount,
  selectedIds,
  currentTabId,
  onActionComplete,
  onCancel,
}: MultiSelectBarProps) {
  const { tabs } = useTabStore();
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";

  const [showPopover, setShowPopover] = useState<PopoverType>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const popoverRef = useRef<HTMLDivElement>(null);

  // Filter tabs (exclude current tab)
  const availableTabs = tabs.filter((t) => t.id !== currentTabId);

  // Close popover on outside click
  useEffect(() => {
    if (!showPopover) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setShowPopover(null);
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [showPopover]);

  // Close popover on Escape
  useEffect(() => {
    if (!showPopover) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setShowPopover(null);
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [showPopover]);

  const handleBatchMove = useCallback(
    async (targetTabId: number) => {
      if (isProcessing || selectedIds.size === 0) return;
      setIsProcessing(true);
      setShowPopover(null);

      try {
        const ids = Array.from(selectedIds);
        const moved = await clipboard.moveToTabBatch(ids, targetTabId);
        logger.info(`Moved ${moved} items to tab ${targetTabId}`);
        onActionComplete();
      } catch (error) {
        logger.error("Failed to batch move items:", error);
      } finally {
        setIsProcessing(false);
      }
    },
    [selectedIds, isProcessing, onActionComplete],
  );

  const handleBatchCopy = useCallback(
    async (targetTabId: number) => {
      if (isProcessing || selectedIds.size === 0) return;
      setIsProcessing(true);
      setShowPopover(null);

      try {
        const ids = Array.from(selectedIds);
        const copied = await clipboard.copyToTabBatch(ids, targetTabId);
        logger.info(`Copied ${copied} items to tab ${targetTabId}`);
        onActionComplete();
      } catch (error) {
        logger.error("Failed to batch copy items:", error);
      } finally {
        setIsProcessing(false);
      }
    },
    [selectedIds, isProcessing, onActionComplete],
  );

  const barStyle: React.CSSProperties = {
    position: "absolute",
    bottom: "12px",
    left: "50%",
    transform: "translateX(-50%)",
    display: "flex",
    alignItems: "center",
    gap: "8px",
    padding: "8px 16px",
    borderRadius: "12px",
    backgroundColor: isDark ? "rgba(30, 41, 59, 0.95)" : "rgba(255, 255, 255, 0.95)",
    backdropFilter: "blur(16px)",
    WebkitBackdropFilter: "blur(16px)",
    border: `1px solid ${isDark ? "rgba(255, 255, 255, 0.1)" : "rgba(0, 0, 0, 0.08)"}`,
    boxShadow: isDark
      ? "0 8px 32px rgba(0, 0, 0, 0.5)"
      : "0 8px 32px rgba(0, 0, 0, 0.15)",
    zIndex: 40,
    userSelect: "none",
    WebkitUserSelect: "none",
  };

  const countStyle: React.CSSProperties = {
    fontSize: "12px",
    fontWeight: 600,
    color: isDark ? "#e2e8f0" : "#4a4a48",
    paddingRight: "8px",
    borderRight: `1px solid ${isDark ? "rgba(255, 255, 255, 0.1)" : "rgba(0, 0, 0, 0.08)"}`,
    whiteSpace: "nowrap",
  };

  const btnBaseStyle: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    gap: "4px",
    padding: "6px 10px",
    borderRadius: "8px",
    border: "none",
    fontSize: "11px",
    fontWeight: 500,
    cursor: "pointer",
    transition: "all 0.15s ease",
    whiteSpace: "nowrap",
  };

  const primaryBtnStyle: React.CSSProperties = {
    ...btnBaseStyle,
    backgroundColor: isDark ? "rgba(59, 130, 246, 0.15)" : "rgba(59, 130, 246, 0.1)",
    color: "#3b82f6",
  };

  const cancelBtnStyle: React.CSSProperties = {
    ...btnBaseStyle,
    backgroundColor: "transparent",
    color: isDark ? "#94a3b8" : "#8a8a88",
  };

  const popoverStyle: React.CSSProperties = {
    position: "absolute",
    bottom: "100%",
    left: "50%",
    transform: "translateX(-50%)",
    marginBottom: "8px",
    minWidth: "160px",
    maxHeight: "220px",
    overflowY: "auto",
    padding: "4px",
    borderRadius: "10px",
    backgroundColor: isDark ? "rgba(30, 41, 59, 0.98)" : "rgba(255, 255, 255, 0.98)",
    backdropFilter: "blur(16px)",
    WebkitBackdropFilter: "blur(16px)",
    border: `1px solid ${isDark ? "rgba(255, 255, 255, 0.1)" : "rgba(0, 0, 0, 0.08)"}`,
    boxShadow: isDark
      ? "0 8px 32px rgba(0, 0, 0, 0.5)"
      : "0 8px 32px rgba(0, 0, 0, 0.15)",
    zIndex: 41,
  };

  const tabItemStyle: React.CSSProperties = {
    display: "block",
    width: "100%",
    textAlign: "left",
    padding: "6px 10px",
    borderRadius: "6px",
    border: "none",
    fontSize: "11px",
    color: isDark ? "#e2e8f0" : "#4a4a48",
    backgroundColor: "transparent",
    cursor: "pointer",
    transition: "background-color 0.1s ease",
  };

  const emptyStyle: React.CSSProperties = {
    padding: "6px 10px",
    fontSize: "11px",
    color: isDark ? "#64748b" : "#a1a1aa",
    textAlign: "center",
  };

  const handleTabItemHover = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.currentTarget.style.backgroundColor = isDark
      ? "rgba(255, 255, 255, 0.1)"
      : "rgba(0, 0, 0, 0.04)";
  };

  const handleTabItemLeave = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.currentTarget.style.backgroundColor = "transparent";
  };

  const handleBtnHover = (e: React.MouseEvent<HTMLButtonElement>, isPrimary: boolean) => {
    if (isPrimary) {
      e.currentTarget.style.backgroundColor = isDark
        ? "rgba(59, 130, 246, 0.25)"
        : "rgba(59, 130, 246, 0.18)";
    } else {
      e.currentTarget.style.backgroundColor = isDark
        ? "rgba(255, 255, 255, 0.1)"
        : "rgba(0, 0, 0, 0.04)";
    }
  };

  const handleBtnLeave = (e: React.MouseEvent<HTMLButtonElement>, isPrimary: boolean) => {
    if (isPrimary) {
      e.currentTarget.style.backgroundColor = isDark
        ? "rgba(59, 130, 246, 0.15)"
        : "rgba(59, 130, 246, 0.1)";
    } else {
      e.currentTarget.style.backgroundColor = "transparent";
    }
  };

  if (selectedCount === 0) return null;

  return (
    <div style={barStyle}>
      {/* Popover - positioned above the bar, shared for both move/copy */}
      {showPopover && (
        <div ref={popoverRef} style={popoverStyle}>
          {availableTabs.length === 0 ? (
            <div style={emptyStyle}>No other tabs</div>
          ) : (
            availableTabs.map((tab) => (
              <button
                key={tab.id}
                style={tabItemStyle}
                onClick={() => {
                  if (showPopover === "move") handleBatchMove(tab.id!);
                  else handleBatchCopy(tab.id!);
                }}
                onMouseEnter={handleTabItemHover}
                onMouseLeave={handleTabItemLeave}
              >
                {tab.name}
              </button>
            ))
          )}
        </div>
      )}

      {/* Count */}
      <span style={countStyle}>
        {selectedCount} selected
      </span>

      {/* Move To Button */}
      <button
        style={primaryBtnStyle}
        disabled={isProcessing || availableTabs.length === 0}
        onClick={() => setShowPopover(showPopover === "move" ? null : "move")}
        onMouseEnter={(e) => handleBtnHover(e, true)}
        onMouseLeave={(e) => handleBtnLeave(e, true)}
      >
        <FolderPlus size={13} />
        <span>Move to</span>
      </button>

      {/* Copy To Button */}
      <button
        style={primaryBtnStyle}
        disabled={isProcessing || availableTabs.length === 0}
        onClick={() => setShowPopover(showPopover === "copy" ? null : "copy")}
        onMouseEnter={(e) => handleBtnHover(e, true)}
        onMouseLeave={(e) => handleBtnLeave(e, true)}
      >
        <Copy size={13} />
        <span>Copy to</span>
      </button>

      {/* Cancel Button */}
      <button
        style={cancelBtnStyle}
        onClick={onCancel}
        onMouseEnter={(e) => handleBtnHover(e, false)}
        onMouseLeave={(e) => handleBtnLeave(e, false)}
      >
        <X size={13} />
        <span>Cancel</span>
      </button>
    </div>
  );
}
