import React, { useState, useCallback, useRef } from "react";
import { AlertTriangle } from "lucide-react";
import { useTheme } from "../contexts/ThemeContext";

export interface ConfirmDialogOptions {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  confirmButtonClass?: string;
}

interface ConfirmDialogState extends ConfirmDialogOptions {
  isOpen: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

interface ConfirmDialogContextType {
  confirm: (options: ConfirmDialogOptions) => Promise<boolean>;
}

import { createContext, useContext } from "react";

const ConfirmDialogContext = createContext<ConfirmDialogContextType | null>(null);

export const useConfirm = () => {
  const context = useContext(ConfirmDialogContext);
  if (!context) {
    throw new Error("useConfirm must be used within ConfirmDialogProvider");
  }
  return context;
};

export function ConfirmDialogProvider({ children }: { children: React.ReactNode }) {
  const [dialog, setDialog] = useState<ConfirmDialogState | null>(null);
  const resolverRef = useRef<((value: boolean) => void) | null>(null);

  const confirm = useCallback((options: ConfirmDialogOptions): Promise<boolean> => {
    return new Promise((resolve) => {
      resolverRef.current = resolve;
      setDialog({
        isOpen: true,
        title: options.title,
        message: options.message,
        confirmText: options.confirmText || "Confirm",
        cancelText: options.cancelText || "Cancel",
        confirmButtonClass: options.confirmButtonClass || "",
        onConfirm: () => {
          setDialog(null);
          resolve(true);
          resolverRef.current = null;
        },
        onCancel: () => {
          setDialog(null);
          resolve(false);
          resolverRef.current = null;
        },
      });
    });
  }, []);

  const handleCancel = useCallback(() => {
    if (dialog) {
      dialog.onCancel();
    }
  }, [dialog]);

  const handleConfirm = useCallback(() => {
    if (dialog) {
      dialog.onConfirm();
    }
  }, [dialog]);

  // Handle Escape key
  React.useEffect(() => {
    if (!dialog) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        handleCancel();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [dialog, handleCancel]);

  return (
    <ConfirmDialogContext.Provider value={{ confirm }}>
      {children}
      {dialog && <ConfirmDialogUI dialog={dialog} onConfirm={handleConfirm} onCancel={handleCancel} />}
    </ConfirmDialogContext.Provider>
  );
}

function ConfirmDialogUI({
  dialog,
  onConfirm,
  onCancel,
}: {
  dialog: ConfirmDialogState;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";

  return (
    <div
      className="fixed inset-0 z-[10000] flex items-center justify-center"
      style={{
        backgroundColor: isDark ? "rgba(0, 0, 0, 0.6)" : "rgba(0, 0, 0, 0.4)",
        backdropFilter: "blur(4px)",
        WebkitBackdropFilter: "blur(4px)",
        animation: "fadeIn 0.15s ease-out",
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onCancel();
        }
      }}
    >
      <div
        className="w-full max-w-sm rounded-xl shadow-xl overflow-hidden"
        style={{
          backgroundColor: isDark ? "rgba(30, 41, 59, 0.98)" : "rgba(255, 255, 255, 0.98)",
          backdropFilter: "blur(16px)",
          WebkitBackdropFilter: "blur(16px)",
          border: `1px solid ${isDark ? "rgba(255, 255, 255, 0.1)" : "rgba(0, 0, 0, 0.08)"}`,
          boxShadow: isDark
            ? "0 20px 40px rgba(0, 0, 0, 0.4)"
            : "0 20px 40px rgba(0, 0, 0, 0.12)",
          animation: "scaleIn 0.2s ease-out",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header with warning icon */}
        <div className="flex items-start gap-3 px-5 pt-5 pb-3">
          <div
            className="flex-shrink-0 w-8 h-8 rounded-lg flex items-center justify-center"
            style={{
              backgroundColor: isDark ? "rgba(239, 68, 68, 0.15)" : "rgba(239, 68, 68, 0.1)",
            }}
          >
            <AlertTriangle size={16} style={{ color: "#ef4444" }} />
          </div>
          <div className="flex-1 min-w-0">
            <h3
              className="text-sm font-semibold mb-1"
              style={{ color: isDark ? "#f1f5f9" : "#1f2937" }}
            >
              {dialog.title}
            </h3>
            <p
              className="text-xs leading-relaxed"
              style={{ color: isDark ? "#94a3b8" : "#6b7280" }}
            >
              {dialog.message}
            </p>
          </div>
        </div>

        {/* Action buttons */}
        <div className="flex items-center justify-end gap-2 px-5 py-3">
          <button
            onClick={onCancel}
            className="px-3 py-1.5 text-xs font-medium rounded-lg transition-all duration-150"
            style={{
              backgroundColor: isDark ? "rgba(255, 255, 255, 0.05)" : "rgba(0, 0, 0, 0.04)",
              color: isDark ? "#cbd5e1" : "#374151",
              border: `1px solid ${isDark ? "rgba(255, 255, 255, 0.1)" : "rgba(0, 0, 0, 0.08)"}`,
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = isDark
                ? "rgba(255, 255, 255, 0.1)"
                : "rgba(0, 0, 0, 0.08)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = isDark
                ? "rgba(255, 255, 255, 0.05)"
                : "rgba(0, 0, 0, 0.04)";
            }}
          >
            {dialog.cancelText}
          </button>
          <button
            onClick={onConfirm}
            className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-all duration-150 ${
              dialog.confirmButtonClass || ""
            }`}
            style={{
              backgroundColor: "#ef4444",
              color: "#ffffff",
              border: "none",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = "#dc2626";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = "#ef4444";
            }}
          >
            {dialog.confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}

// Add animation styles
if (typeof document !== "undefined") {
  const style = document.createElement("style");
  style.textContent = `
    @keyframes fadeIn {
      from {
        opacity: 0;
      }
      to {
        opacity: 1;
      }
    }

    @keyframes scaleIn {
      from {
        transform: scale(0.95);
        opacity: 0;
      }
      to {
        transform: scale(1);
        opacity: 1;
      }
    }
  `;
  document.head.appendChild(style);
}
