import React, { forwardRef } from "react";
import { useTranslation } from "react-i18next";
import { useTheme } from "../contexts/ThemeContext";
import { useCardExtensions } from "../plugin/extensions";
import { ContextMenu } from "./ContextMenu";
import { createLogger } from "../utils/logger";

const logger = createLogger("ClipboardCard");

interface ClipboardCardProps {
  id: number;
  content: string;
  type: "text" | "image" | "file";
  index: number;
  isPinned: boolean;
  isSelected: boolean;
  lineHeight: "small" | "medium" | "large";
  isMultiSelectMode?: boolean;
  isMultiSelected?: boolean; // Whether selected in multi-select mode, shown highlighted
  batchItemIds?: Set<number>;
  isDraggingItem?: boolean; // Whether the current card is being dragged
  tabId?: number | null; // Current tab ID for context menu
  onBatchActionComplete?: () => void;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onTogglePin: () => void;
  onEdit?: () => void;
  onDragStart?: (e: React.DragEvent) => void;
  onDragEnd?: (e: React.DragEvent) => void;
  onMultiDragStart?: (e: React.DragEvent) => void;
  onMultiDragEnd?: (e: React.DragEvent) => void;
  // Drag reordering fields for HTML5
  onDragOver?: (e: React.DragEvent, showBelow: boolean) => void;
  onDragLeave?: () => void;
  onDrop?: (e: React.DragEvent) => void;
  // Mouse-drag fields for macOS compatibility
  onMouseDown?: (e: React.MouseEvent) => void;
  onMouseMove?: (e: React.MouseEvent) => void;
  onMouseUp?: () => void;
  onMouseLeaveCard?: () => void;
}

// Card background colors (unified for modern design)
const lightCardBg = "rgba(255, 255, 255, 0.85)";
const darkCardBg = "rgba(255, 255, 255, 0.04)";

// Card size configurations
export const CARD_SIZE_CONFIG = {
  small: { fontSize: 12, textHeight: 26 },
  medium: { fontSize: 14, textHeight: 32 },
  large: { fontSize: 16, textHeight: 36 },
} as const;

const IMAGE_CARD_HEIGHT = 96; // Fixed image card height

const ClipboardCard = forwardRef<HTMLDivElement, ClipboardCardProps>(
  (
    {
      id,
      content,
      type,
      index,
      isPinned,
      isSelected,
      lineHeight,
      isMultiSelectMode = false,
      isMultiSelected = false,
      batchItemIds,
      isDraggingItem = false,
      tabId,
      onBatchActionComplete,
      onClick,
      onDoubleClick,
      onTogglePin,
      onEdit,
      onDragStart,
      onDragEnd,
      onMultiDragStart,
      onMultiDragEnd,
      onDragOver,
      onDragLeave,
      onDrop,
      onMouseDown,
      onMouseMove,
      onMouseUp,
      onMouseLeaveCard,
    },
    ref,
  ) => {
    const { resolvedTheme } = useTheme();
    const { t } = useTranslation();
    const isDark = resolvedTheme === "dark";
    const isImage = type === "image";
    const [isHovered, setIsHovered] = React.useState(false);
    const [isDragging, setIsDragging] = React.useState(false);
    const [imageError, setImageError] = React.useState(false);

    // Get card extension buttons from plugins
    const cardExtensions = useCardExtensions(
      { id, content, type, is_pinned: isPinned },
      "action",
      isDark ? "dark" : "light",
    );

    // Debug log for images (commented to reduce noise)
    // React.useEffect(() => {
    //   if (isImage) {
    //     console.log(
    //       `[ClipboardCard] Image item ${id}: type=${type}, content_prefix=${content.substring(0, 50)}...`,
    //     );
    //   }
    // }, [id, isImage, type, content]);

    // Get unified background color based on theme
    const bgColor = isDark ? darkCardBg : lightCardBg;

    // Fixed height based on content type
    const cardHeight = isImage
      ? IMAGE_CARD_HEIGHT
      : CARD_SIZE_CONFIG[lineHeight].textHeight;

    // Use green highlight for selected state in multi-select mode
    const getBackgroundColor = () => {
      if (isMultiSelectMode && isMultiSelected) {
        return isDark ? "rgba(34, 197, 94, 0.15)" : "rgba(34, 197, 94, 0.1)";
      }
      // Use green highlight for the dragged item
      if (isDraggingItem) {
        return isDark ? "rgba(34, 197, 94, 0.15)" : "rgba(34, 197, 94, 0.1)";
      }
      if (isSelected) {
        return isDark ? "rgba(59, 130, 246, 0.15)" : "#e8f0fe";
      }
      return bgColor;
    };

    const getBorderColor = () => {
      if (isMultiSelectMode && isMultiSelected) {
        return isDark
          ? "1px solid rgba(34, 197, 94, 0.5)"
          : "1px solid rgba(34, 197, 94, 0.4)";
      }
      // Use a green border for the dragged item
      if (isDraggingItem) {
        return isDark
          ? "1px solid rgba(34, 197, 94, 0.5)"
          : "1px solid rgba(34, 197, 94, 0.4)";
      }
      if (isSelected) {
        return isDark
          ? "1px solid rgba(59, 130, 246, 0.5)"
          : "1px solid #aecbfa";
      }
      return `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`;
    };

    const cardStyle: React.CSSProperties = {
      position: "relative",
      width: "100%",
      padding: "6px 10px",
      borderRadius: "10px",
      backgroundColor: getBackgroundColor(),
      // Do not lift the item while dragging; keep it in place so the splitter highlight is easier to see
      boxShadow: isDragging
        ? isDark
          ? "0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 1px rgba(255, 255, 255, 0.03)"
          : "0 1px 4px rgba(0, 0, 0, 0.04), 0 0 0 1px rgba(0, 0, 0, 0.03)"
        : isHovered
          ? isDark
            ? "0 8px 32px rgba(0, 0, 0, 0.4), 0 0 0 1px rgba(255, 255, 255, 0.05)"
            : "0 4px 16px rgba(0, 0, 0, 0.06), 0 0 0 1px rgba(0, 0, 0, 0.03)"
          : isDark
            ? "0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 1px rgba(255, 255, 255, 0.03)"
            : "0 1px 4px rgba(0, 0, 0, 0.04), 0 0 0 1px rgba(0, 0, 0, 0.03)",
      // Show grab cursor while dragging, default cursor otherwise, and pointer in multi-select mode
      cursor: isDragging ? "grabbing" : isMultiSelectMode ? "pointer" : "default",
      transition: "all 0.2s cubic-bezier(0.4, 0, 0.2, 1)",
      transform: isDragging
        ? "translateY(0)"
        : isHovered
          ? "translateY(-1px)"
          : "translateY(0)",
      border: getBorderColor(),
      userSelect: "none",
      WebkitUserSelect: "none",
      display: "flex",
      alignItems: "center",
      gap: "10px",
      height: cardHeight,
      boxSizing: "border-box",
      overflow: "hidden",
      flexShrink: 0,
      // Lower opacity while dragging so the splitter highlight is clearer
      opacity: isDragging ? 0.7 : 1,
    };

    const indexContainerStyle: React.CSSProperties = {
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      minWidth: "28px",
      flexShrink: 0,
    };

    const indexBadgeStyle: React.CSSProperties = {
      fontSize: "10px",
      fontWeight: 600,
      color: isDark ? "#64748b" : "#9ca3af",
      letterSpacing: "0.02em",
      lineHeight: "1.2",
    };

    const pinIndicatorStyle: React.CSSProperties = {
      fontSize: "9px",
      color: "#3b82f6",
      marginTop: "2px",
    };

    const contentContainerStyle: React.CSSProperties = {
      flex: 1,
      minWidth: 0,
      display: "flex",
      flexDirection: "column",
      justifyContent: "center",
    };

    const contentStyle: React.CSSProperties = {
      margin: 0,
      fontSize: `${CARD_SIZE_CONFIG[lineHeight].fontSize}px`,
      color: isDark ? "#94a3b8" : "#6b6b69",
      lineHeight: "1.3",
      wordBreak: "break-word",
      overflow: "hidden",
      textOverflow: "ellipsis",
      whiteSpace: "nowrap",
    };

    const imageContainerStyle: React.CSSProperties = {
      borderRadius: "6px",
      overflow: "hidden",
      background: isDark ? "rgba(0,0,0,0.2)" : "rgba(0,0,0,0.03)",
      height: IMAGE_CARD_HEIGHT - 12, // Subtract padding
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.05)"}`,
      flex: 1,
    };

    const imageStyle: React.CSSProperties = {
      maxHeight: `${IMAGE_CARD_HEIGHT - 16}px`,
      maxWidth: "100%",
      objectFit: "contain",
      display: "block",
      borderRadius: "3px",
    };

    const actionsStyle: React.CSSProperties = {
      position: "absolute",
      right: "8px",
      top: "50%",
      display: "flex",
      gap: "3px",
      opacity: isHovered ? 1 : 0,
      visibility: isHovered ? "visible" : "hidden",
      transform: isHovered
        ? "translateY(-50%)"
        : "translateY(calc(-50% + 4px))",
      pointerEvents: isHovered ? "auto" : "none",
      transition:
        "opacity 0.2s ease, transform 0.2s ease, visibility 0.2s ease",
    };

    const buttonStyle: React.CSSProperties = {
      width: "22px",
      height: "22px",
      borderRadius: "6px",
      border: "none",
      background: isDark
        ? "rgba(255, 255, 255, 0.1)"
        : "rgba(255, 255, 255, 0.7)",
      backdropFilter: "blur(12px)",
      WebkitBackdropFilter: "blur(12px)",
      cursor: "pointer",
      fontSize: "11px",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      boxShadow: isDark
        ? "0 2px 8px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255,255,255,0.1)"
        : "0 2px 8px rgba(0, 0, 0, 0.08)",
      transition: "all 0.15s ease",
      color: isDark ? "#e2e8f0" : "#52525b",
    };

    return (
      <ContextMenu 
        itemId={id} 
        currentTabId={tabId ?? null}
        batchItemIds={
          isMultiSelectMode && isMultiSelected ? batchItemIds : undefined
        }
        onBatchActionComplete={onBatchActionComplete}
      >
        <div
          ref={ref}
          data-item-id={id}
          style={cardStyle}
        onMouseEnter={() => setIsHovered(true)}
        onMouseDown={(e) => {
          // macOS compatibility: simulate dragging with mouse events
          if (!isMultiSelectMode && onMouseDown) {
            onMouseDown(e);
          }
        }}
        onMouseMove={(e) => {
          if (isDraggingItem && onMouseMove) {
            onMouseMove(e);
          }
        }}
        onMouseUp={() => {
          if (isDraggingItem && onMouseUp) {
            onMouseUp();
          }
        }}
        onMouseLeave={() => {
          setIsHovered(false);
          if (isDraggingItem && onMouseLeaveCard) {
            onMouseLeaveCard();
          }
        }}
        onClick={(e) => {
          e.stopPropagation();
          // Always pass the event to the parent component, which checks Ctrl/Shift keys
          onClick(e);
        }}
        onDoubleClick={(e) => {
          e.stopPropagation();
          // Disable double-click in multi-select mode
          if (!isMultiSelectMode) {
            onDoubleClick();
          }
        }}
        draggable={
          (isMultiSelectMode && isMultiSelected && !!onMultiDragStart) ||
          (!isMultiSelectMode && !!onDragStart)
        }
        onDragStart={(e) => {
          logger.debug('[ClipboardCard] Drag started for item:', id);
          setIsDragging(true);
          if (isMultiSelectMode && isMultiSelected && onMultiDragStart) {
            onMultiDragStart(e);
          } else if (!isMultiSelectMode && onDragStart) {
            onDragStart(e);
          }
        }}
        onDragEnd={(e) => {
          setIsDragging(false);
          if (isMultiSelectMode && isMultiSelected && onMultiDragEnd) {
            onMultiDragEnd(e);
          } else if (!isMultiSelectMode && onDragEnd) {
            onDragEnd(e);
          }
        }}
        onDragOver={(e) => {
          logger.debug('[ClipboardCard] onDragOver triggered for item:', id);
          if (onDragOver) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
            // Simplified logic: there is only one insertion point between two items
            // Always show above the target item to mean "insert before this item"
            onDragOver(e, false);
          }
        }}
        onDragLeave={(e) => {
          if (onDragLeave) {
            e.preventDefault();
            onDragLeave();
          }
        }}
        onDrop={(e) => {
          logger.debug('[ClipboardCard] onDrop triggered for item:', id);
          if (onDrop) {
            e.preventDefault();
            onDrop(e);
          }
        }}
      >
        <div style={indexContainerStyle}>
          <span style={indexBadgeStyle}>#{index}</span>
          {isPinned && <span style={pinIndicatorStyle}>📌</span>}
        </div>
        <div style={contentContainerStyle}>
          {isImage ? (
            <div style={imageContainerStyle}>
              {imageError ? (
                <div style={{ color: "#ef4444", fontSize: "12px" }}>
                  {t("clipboardCard.imageLoadError")}
                </div>
              ) : (
                <img
                  src={content}
                  alt="clipboard content"
                  draggable={false}
                  style={imageStyle}
                  onError={(e) => {
                    // console.error(
                    //   `[ClipboardCard] Image ${id} failed to load:`,
                    //   e,
                    // );
                    setImageError(true);
                  }}
                  onLoad={() => {
                    // console.log(
                    //   `[ClipboardCard] Image ${id} loaded successfully`,
                    // );
                  }}
                />
              )}
            </div>
          ) : (
            <p style={contentStyle} title={content}>
              {content}
            </p>
          )}
        </div>

        {/* Action buttons - hidden by default, visible only on card hover, hidden in multi-select mode */}
        {!isMultiSelectMode && (
          <div style={actionsStyle}>
            {/* Edit button - only for text type */}
            {type === "text" && onEdit && (
              <button
                style={{
                  ...buttonStyle,
                  color: isDark ? "#94a3b8" : "#71717a",
                }}
                title={t("clipboardCard.editContent")}
                onClick={(e) => {
                  e.stopPropagation();
                  onEdit();
                }}
                onMouseOver={(e) => {
                  e.currentTarget.style.transform = "scale(1.05)";
                  e.currentTarget.style.background = isDark
                    ? "rgba(255, 255, 255, 0.15)"
                    : "rgba(0, 0, 0, 0.08)";
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.transform = "scale(1)";
                  e.currentTarget.style.background = isDark
                    ? "rgba(255, 255, 255, 0.1)"
                    : "rgba(0, 0, 0, 0.05)";
                }}
              >
                📝️
              </button>
            )}
            {/* Plugin extension buttons - rendered between edit and pin buttons */}
            {cardExtensions}
            <button
              style={{
                ...buttonStyle,
                color: isPinned ? "#3b82f6" : isDark ? "#94a3b8" : "#71717a",
                background: isPinned
                  ? isDark
                    ? "rgba(59, 130, 246, 0.2)"
                    : "rgba(59, 130, 246, 0.15)"
                  : isDark
                    ? "rgba(255, 255, 255, 0.1)"
                    : "rgba(0, 0, 0, 0.05)",
              }}
              title={
                isPinned ? t("clipboardCard.unpin") : t("clipboardCard.pin")
              }
              onClick={(e) => {
                e.stopPropagation();
                onTogglePin();
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.transform = "scale(1.05)";
                e.currentTarget.style.background = isPinned
                  ? isDark
                    ? "rgba(59, 130, 246, 0.3)"
                    : "rgba(59, 130, 246, 0.25)"
                  : isDark
                    ? "rgba(255, 255, 255, 0.15)"
                    : "rgba(0, 0, 0, 0.08)";
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.transform = "scale(1)";
                e.currentTarget.style.background = isPinned
                  ? isDark
                    ? "rgba(59, 130, 246, 0.2)"
                    : "rgba(59, 130, 246, 0.15)"
                  : isDark
                    ? "rgba(255, 255, 255, 0.1)"
                    : "rgba(0, 0, 0, 0.05)";
              }}
            >
              📌
            </button>
          </div>
        )}
      </div>
      </ContextMenu>
    );
  },
);

ClipboardCard.displayName = "ClipboardCard";

export default ClipboardCard;
