import React, { useState, useEffect, useMemo, useRef } from "react";
import { X, Check, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { createLogger } from "../utils/logger";
import { clipboard } from "../lib/tauri-api";
import { useTheme } from "../contexts/ThemeContext";

const logger = createLogger("ContentEditor");
const EDITOR_LAYOUT_SCAN_LIMIT = 100000;
const END_SELECTION_LIMIT = 200000;

interface ContentEditorProps {
  id: number;
  content: string;
  type: "text" | "image" | "file";
  onClose: () => void;
  onSave: (newContent: string) => void;
}

const ContentEditor: React.FC<ContentEditorProps> = ({
  id,
  content,
  type,
  onClose,
  onSave,
}) => {
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";
  const { t } = useTranslation();
  const [editedContent, setEditedContent] = useState(content);
  const [isSaving, setIsSaving] = useState(false);
  const [hasChanges, setHasChanges] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const contentLines = useMemo(() => {
    let lineCount = 0;
    let currentLineLength = 0;
    const scanLength = Math.min(editedContent.length, EDITOR_LAYOUT_SCAN_LIMIT);

    for (let i = 0; i < scanLength; i += 1) {
      const char = editedContent[i];
      if (char === "\n" || char === "\r") {
        lineCount += Math.max(Math.ceil(currentLineLength / 72), 1);
        currentLineLength = 0;
        if (char === "\r" && editedContent[i + 1] === "\n") i += 1;
      } else {
        currentLineLength += 1;
      }
    }

    lineCount += Math.max(Math.ceil(currentLineLength / 72), 1);
    return Math.max(lineCount, 1);
  }, [editedContent]);
  const editorHeight = Math.min(Math.max(contentLines * 24 + 128, 260), 670);

  // Track changes
  useEffect(() => {
    setHasChanges(editedContent !== content);
  }, [editedContent, content]);

  // Focus textarea on mount
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.focus();
      if (textareaRef.current.value.length <= END_SELECTION_LIMIT) {
        const len = textareaRef.current.value.length;
        textareaRef.current.setSelectionRange(len, len);
      } else {
        textareaRef.current.setSelectionRange(0, 0);
      }
    }
  }, []);

  // Handle keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (hasChanges) {
          // Show confirmation or just close
          onClose();
        } else {
          onClose();
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        e.preventDefault();
        if (hasChanges && !isSaving) {
          handleSave();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [hasChanges, isSaving, editedContent]);

  const handleSave = async () => {
    if (!hasChanges || isSaving) return;

    setIsSaving(true);
    try {
      logger.info("Saving edited content for item:", id);
      await clipboard.updateContent(id, editedContent);
      logger.info("Content saved successfully");
      onSave(editedContent);
      onClose();
    } catch (error) {
      logger.error("Failed to save content:", error);
    } finally {
      setIsSaving(false);
    }
  };

  const handleReset = () => {
    setEditedContent(content);
  };

  // Don't allow editing for images
  if (type === "image") {
    return (
      <div
        className="fixed inset-0 z-50 flex items-center justify-center backdrop-blur-xl transition-colors duration-300"
        style={{
          backgroundColor: isDark ? "rgba(0,0,0,0.4)" : "rgba(0,0,0,0.2)",
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) {
            onClose();
          }
        }}
      >
        <div
          className="w-[480px] rounded-2xl shadow-2xl flex flex-col overflow-hidden backdrop-blur-xl transition-colors duration-300"
          style={{
            backgroundColor: isDark
              ? "rgba(15,23,42,0.95)"
              : "rgba(252,251,249,0.95)",
            border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
            boxShadow: isDark
              ? "0 25px 50px -12px rgba(0,0,0,0.5)"
              : "0 25px 50px -12px rgba(0,0,0,0.08)",
          }}
        >
          <div
            className="flex items-center justify-between px-6 py-5 border-b"
            style={{
              borderColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(0,0,0,0.04)",
            }}
          >
            <h3
              className="text-sm font-medium"
              style={{ color: isDark ? "#e2e8f0" : "#4a4a48" }}
            >
              {t('contentEditor.cannotEdit')}
            </h3>
            <button
              onClick={onClose}
              className="p-2 rounded-xl transition-all"
              onMouseEnter={(e) =>
                (e.currentTarget.style.backgroundColor = isDark
                  ? "rgba(255,255,255,0.1)"
                  : "rgba(0,0,0,0.04)")
              }
              onMouseLeave={(e) =>
                (e.currentTarget.style.backgroundColor = "transparent")
              }
            >
              <X size={16} style={{ color: isDark ? "#94a3b8" : "#8a8a88" }} />
            </button>
          </div>
          <div className="p-6 text-center">
            <p style={{ color: isDark ? "#94a3b8" : "#6b6b69" }}>
              {t('contentEditor.imageCannotEdit')}
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center backdrop-blur-xl transition-colors duration-300"
      style={{
        backgroundColor: isDark ? "rgba(0,0,0,0.4)" : "rgba(0,0,0,0.2)",
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        className="w-[640px] max-w-[calc(100vw-48px)] max-h-[calc(100vh-48px)] rounded-2xl shadow-2xl flex flex-col overflow-hidden backdrop-blur-xl transition-colors duration-300"
        style={{
          backgroundColor: isDark
            ? "rgba(15,23,42,0.95)"
            : "rgba(252,251,249,0.95)",
          border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
          boxShadow: isDark
            ? "0 25px 50px -12px rgba(0,0,0,0.5)"
            : "0 25px 50px -12px rgba(0,0,0,0.08)",
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between px-3 py-1.5 border-b flex-shrink-0"
          style={{
            borderColor: isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)",
          }}
        >
          <div>
            <h3
              className="text-sm font-medium leading-4"
              style={{ color: isDark ? "#e2e8f0" : "#4a4a48" }}
            >
              {t("contentEditor.title")}
            </h3>
            <p
              className="text-[10px] leading-3"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {t("contentEditor.subtitle")}
            </p>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-lg transition-all"
            onMouseEnter={(e) =>
              (e.currentTarget.style.backgroundColor = isDark
                ? "rgba(255,255,255,0.1)"
                : "rgba(0,0,0,0.04)")
            }
            onMouseLeave={(e) =>
              (e.currentTarget.style.backgroundColor = "transparent")
            }
          >
            <X size={16} style={{ color: isDark ? "#94a3b8" : "#8a8a88" }} />
          </button>
        </div>

        {/* Editor */}
        <div className="p-3 overflow-hidden">
          <textarea
            ref={textareaRef}
            value={editedContent}
            onChange={(e) => setEditedContent(e.target.value)}
            className="w-full p-2.5 rounded-xl text-sm resize-none outline-none transition-all font-mono"
            style={{
              backgroundColor: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(255,255,255,0.7)",
              border: `1px solid ${isDark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.06)"}`,
              color: isDark ? "#e2e8f0" : "#4a4a48",
              height: `${editorHeight}px`,
              maxHeight: "calc(100vh - 136px)",
              lineHeight: "1.6",
            }}
            placeholder={t("contentEditor.placeholder")}
            spellCheck={false}
          />
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between gap-3 px-3 py-1.5 border-t flex-shrink-0"
          style={{
            borderColor: isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)",
          }}
        >
          <div className="flex items-center gap-2">
            {hasChanges && (
              <span
                className="text-[10px] px-1.5 py-0.5 rounded-lg"
                style={{
                  backgroundColor: isDark
                    ? "rgba(59,130,246,0.15)"
                    : "rgba(59,130,246,0.08)",
                  color: "#3b82f6",
                }}
              >
                {t("contentEditor.unsavedChanges")}
              </span>
            )}
            <span
              className="text-[10px]"
              style={{ color: isDark ? "#64748b" : "#9a9a98" }}
            >
              {editedContent.length} {t("contentEditor.characters")}
            </span>
          </div>
          <div className="flex items-center gap-2">
            {hasChanges && (
              <button
                onClick={handleReset}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all border"
                style={{
                  backgroundColor: isDark
                    ? "rgba(255,255,255,0.05)"
                    : "rgba(255,255,255,0.6)",
                  borderColor: isDark
                    ? "rgba(255,255,255,0.1)"
                    : "rgba(0,0,0,0.06)",
                  color: isDark ? "#94a3b8" : "#6b6b69",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.backgroundColor = isDark
                    ? "rgba(255,255,255,0.1)"
                    : "rgba(0,0,0,0.04)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.backgroundColor = isDark
                    ? "rgba(255,255,255,0.05)"
                    : "rgba(255,255,255,0.6)";
                }}
              >
                <RotateCcw size={12} />
                {t("contentEditor.reset")}
              </button>
            )}
            <button
              onClick={handleSave}
              disabled={!hasChanges || isSaving}
              className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all border"
              style={{
                backgroundColor:
                  hasChanges && !isSaving
                    ? "rgba(59,130,246,0.15)"
                    : isDark
                      ? "rgba(255,255,255,0.05)"
                      : "rgba(255,255,255,0.5)",
                borderColor:
                  hasChanges && !isSaving
                    ? "rgba(59,130,246,0.3)"
                    : isDark
                      ? "rgba(255,255,255,0.1)"
                      : "rgba(0,0,0,0.06)",
                color:
                  hasChanges && !isSaving
                    ? "#3b82f6"
                    : isDark
                      ? "#475569"
                      : "#a1a1aa",
                opacity: hasChanges ? 1 : 0.5,
                cursor: hasChanges && !isSaving ? "pointer" : "not-allowed",
              }}
              onMouseEnter={(e) => {
                if (hasChanges && !isSaving) {
                  e.currentTarget.style.backgroundColor =
                    "rgba(59,130,246,0.25)";
                }
              }}
              onMouseLeave={(e) => {
                if (hasChanges && !isSaving) {
                  e.currentTarget.style.backgroundColor =
                    "rgba(59,130,246,0.15)";
                }
              }}
            >
              <Check size={12} />
              {isSaving ? t("contentEditor.saving") : t("contentEditor.save")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default ContentEditor;
