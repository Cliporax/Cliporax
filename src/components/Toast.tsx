import { useState, useEffect, useCallback } from "react";
import { X, AlertCircle, CheckCircle, Info } from "lucide-react";

export interface Toast {
  id: string;
  type: "success" | "error" | "info" | "warning";
  message: string;
  duration?: number; // Auto-close time in milliseconds; 0 disables auto-close
}

interface ToastProviderProps {
  children: React.ReactNode;
}

interface ToastContextType {
  showToast: (toast: Omit<Toast, "id">) => void;
  success: (message: string, duration?: number) => void;
  error: (message: string, duration?: number) => void;
  info: (message: string, duration?: number) => void;
  warning: (message: string, duration?: number) => void;
}

import { createContext, useContext } from "react";

const ToastContext = createContext<ToastContextType | null>(null);

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return context;
};

let toastId = 0;

export function ToastProvider({ children }: ToastProviderProps) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const showToast = useCallback((toast: Omit<Toast, "id">) => {
    const id = `toast-${++toastId}`;
    const newToast: Toast = {
      ...toast,
      id,
      duration: toast.duration ?? 5000, // Default: 5 seconds
    };

    setToasts((prev) => [...prev, newToast]);

    // Auto close
    if (newToast.duration && newToast.duration > 0) {
      setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
      }, newToast.duration);
    }
  }, []);

  const success = useCallback(
    (message: string, duration?: number) => {
      showToast({ type: "success", message, duration });
    },
    [showToast]
  );

  const error = useCallback(
    (message: string, duration?: number) => {
      showToast({ type: "error", message, duration });
    },
    [showToast]
  );

  const info = useCallback(
    (message: string, duration?: number) => {
      showToast({ type: "info", message, duration });
    },
    [showToast]
  );

  const warning = useCallback(
    (message: string, duration?: number) => {
      showToast({ type: "warning", message, duration });
    },
    [showToast]
  );

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ showToast, success, error, info, warning }}>
      {children}
      <ToastContainer toasts={toasts} onRemove={removeToast} />
    </ToastContext.Provider>
  );
}

function ToastContainer({
  toasts,
  onRemove,
}: {
  toasts: Toast[];
  onRemove: (id: string) => void;
}) {
  if (toasts.length === 0) return null;

  return (
    <div
      style={{
        position: "fixed",
        top: "16px",
        right: "16px",
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        gap: "8px",
        maxWidth: "400px",
      }}
    >
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onRemove={onRemove} />
      ))}
    </div>
  );
}

function ToastItem({
  toast,
  onRemove,
}: {
  toast: Toast;
  onRemove: (id: string) => void;
}) {
  const iconMap = {
    success: <CheckCircle size={20} />,
    error: <AlertCircle size={20} />,
    warning: <AlertCircle size={20} />,
    info: <Info size={20} />,
  };

  const colorMap = {
    success: {
      bg: "rgba(34, 197, 94, 0.1)",
      border: "rgba(34, 197, 94, 0.3)",
      icon: "#22c55e",
      text: "#e2e8f0",
    },
    error: {
      bg: "rgba(239, 68, 68, 0.1)",
      border: "rgba(239, 68, 68, 0.3)",
      icon: "#ef4444",
      text: "#e2e8f0",
    },
    warning: {
      bg: "rgba(245, 158, 11, 0.1)",
      border: "rgba(245, 158, 11, 0.3)",
      icon: "#f59e0b",
      text: "#e2e8f0",
    },
    info: {
      bg: "rgba(59, 130, 246, 0.1)",
      border: "rgba(59, 130, 246, 0.3)",
      icon: "#3b82f6",
      text: "#e2e8f0",
    },
  };

  const colors = colorMap[toast.type];

  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: "12px",
        padding: "12px 16px",
        background: colors.bg,
        border: `1px solid ${colors.border}`,
        borderRadius: "8px",
        color: colors.text,
        boxShadow: "0 4px 6px rgba(0, 0, 0, 0.1)",
        backdropFilter: "blur(10px)",
        WebkitBackdropFilter: "blur(10px)",
        animation: "slideIn 0.3s ease-out",
      }}
    >
      <span style={{ color: colors.icon, flexShrink: 0, marginTop: "2px" }}>
        {iconMap[toast.type]}
      </span>
      <div style={{ flex: 1, fontSize: "14px", lineHeight: "1.5" }}>
        {toast.message}
      </div>
      <button
        onClick={() => onRemove(toast.id)}
        style={{
          background: "none",
          border: "none",
          color: colors.text,
          cursor: "pointer",
          padding: "2px",
          borderRadius: "4px",
          opacity: 0.6,
          transition: "opacity 0.15s",
          flexShrink: 0,
        }}
        onMouseEnter={(e) => (e.currentTarget.style.opacity = "1")}
        onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.6")}
        aria-label="Close notification"
      >
        <X size={16} />
      </button>
    </div>
  );
}

// Add animation styles
if (typeof document !== "undefined") {
  const style = document.createElement("style");
  style.textContent = `
    @keyframes slideIn {
      from {
        transform: translateX(100%);
        opacity: 0;
      }
      to {
        transform: translateX(0);
        opacity: 1;
      }
    }
  `;
  document.head.appendChild(style);
}
