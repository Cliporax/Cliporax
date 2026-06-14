import React from "react";
import { useTranslation } from "react-i18next";
import {
  CONTENT_PADDING_TOP,
  CONTENT_PADDING_BOTTOM,
  CONTENT_PADDING_RIGHT,
  SCROLLBAR_WIDTH,
} from "../constants";

interface ScrollbarProps {
  // Configuration parameters
  viewportHeight: number;
  contentHeight: number;
  totalHeight: number;
  isDark: boolean;
  // Scrollbar calculated values
  scrollTop: number;
  scrollbarTrackHeight: number;
  scrollbarThumbHeight: number;
  scrollbarThumbTop: number;
  isDraggingScrollbar: boolean;
  // Tooltip information
  dragTooltipIndex: number;
  totalCount: number;
  dragTooltipItem?: any;
  // Event handlers
  onScrollbarMouseDown: (e: React.MouseEvent) => void;
  onScrollbarTrackClick: (e: React.MouseEvent) => void;
}

// Format timestamp
const formatTimestamp = (timestamp?: string): string => {
  if (!timestamp) return "";
  try {
    const date = new Date(timestamp);
    return date.toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
};

export const Scrollbar: React.FC<ScrollbarProps> = ({
  viewportHeight,
  contentHeight,
  totalHeight,
  isDark,
  scrollTop,
  scrollbarTrackHeight,
  scrollbarThumbHeight,
  scrollbarThumbTop,
  isDraggingScrollbar,
  dragTooltipIndex,
  totalCount,
  dragTooltipItem,
  onScrollbarMouseDown,
  onScrollbarTrackClick,
}) => {
  const { t } = useTranslation();
  const maxScrollTop = Math.max(0, totalHeight - viewportHeight);
  const SCROLLBAR_MARGIN_TOP = CONTENT_PADDING_TOP;
  const SCROLLBAR_MARGIN_BOTTOM = CONTENT_PADDING_BOTTOM;

  if (maxScrollTop <= 0 || scrollbarTrackHeight <= 0) {
    return null;
  }

  return (
    <>
      {/* Custom scrollbar - compact right-side layout */}
      <div
        onClick={onScrollbarTrackClick}
        style={{
          position: "absolute",
          right: `${CONTENT_PADDING_RIGHT}px`,
          top: SCROLLBAR_MARGIN_TOP,
          height: scrollbarTrackHeight,
          width: `${SCROLLBAR_WIDTH}px`,
          background: isDark ? "rgba(255,255,255,0.02)" : "rgba(0,0,0,0.02)",
          cursor: "default",
          borderRadius: "3px",
        }}
      >
        {/* Scrollbar thumb */}
        <div
          onMouseDown={onScrollbarMouseDown}
          style={{
            position: "absolute",
            right: 0,
            width: `${SCROLLBAR_WIDTH}px`,
            height: scrollbarThumbHeight,
            top: scrollbarThumbTop - SCROLLBAR_MARGIN_TOP,
            borderRadius: "3px",
            background: isDraggingScrollbar
              ? isDark
                ? "rgba(255,255,255,0.3)"
                : "rgba(0,0,0,0.3)"
              : isDark
                ? "rgba(255,255,255,0.12)"
                : "rgba(0,0,0,0.12)",
            cursor: isDraggingScrollbar ? "grabbing" : "default",
            transition: isDraggingScrollbar ? "none" : "background 0.2s",
          }}
        />
      </div>

      {/* Scrollbar drag tooltip */}
      {isDraggingScrollbar && (
        <div
          style={{
            position: "absolute",
            right: `${CONTENT_PADDING_RIGHT + SCROLLBAR_WIDTH + 12}px`,
            top: "50%",
            transform: "translateY(-50%)",
            padding: "8px 14px",
            borderRadius: "8px",
            background: isDark
              ? "rgba(30, 41, 59, 0.95)"
              : "rgba(255, 255, 255, 0.95)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.1)"}`,
            boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
            fontSize: "13px",
            color: isDark ? "#e2e8f0" : "#1f2937",
            zIndex: 1000,
          }}
        >
          <div style={{ fontWeight: 500 }}>
            {t('clipboardList.scrollbarPosition', {
              current: dragTooltipIndex,
              total: totalCount,
            })}
          </div>
          {dragTooltipItem?.updated_at && (
            <div
              style={{
                fontSize: "11px",
                color: isDark ? "#94a3b8" : "#6b7280",
                marginTop: "4px",
              }}
            >
              {formatTimestamp(dragTooltipItem.updated_at)}
            </div>
          )}
        </div>
      )}
    </>
  );
};
