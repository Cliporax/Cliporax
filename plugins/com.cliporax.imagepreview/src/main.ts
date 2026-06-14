/**
 * Image Preview Plugin for Cliporax
 * Provides a separate resizable window for viewing images with zoom controls
 */

import { invoke } from "@tauri-apps/api/core";

// Plugin types
interface PluginContext {
  theme?: "dark" | "light" | "system";
  [key: string]: unknown;
}

interface ClipboardItem {
  id: string;
  type: "text" | "image";
  content: string;
  pinned?: boolean;
  created_at?: string;
  updated_at?: string;
}

interface ExtensionProps {
  data?: {
    item?: ClipboardItem;
    position?: string;
  };
  context?: PluginContext;
}

interface Plugin {
  meta: {
    id: string;
    name: string;
    version: string;
  };
  onActivate: (ctx: PluginContext) => void;
  onDeactivate: () => void;
  extensions: {
    [key: string]: {
      render: (props: ExtensionProps) => HTMLElement | null;
      shouldShow?: (data: unknown) => boolean;
    };
  };
}

/**
 * Create the preview button element
 */
function createPreviewButton(item: ClipboardItem, theme: string): HTMLElement {
  const btn = document.createElement("button");
  btn.style.cssText = `
    width: 22px;
    height: 22px;
    border-radius: 6px;
    border: none;
    background: ${theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(255,255,255,0.7)"};
    backdrop-filter: blur(12px);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    color: ${theme === "dark" ? "#e2e8f0" : "#52525b"};
    transition: all 0.15s ease;
  `;
  btn.title = "Preview Image in New Window";
  btn.innerHTML = `
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="11" cy="11" r="8"/>
      <path d="m21 21-4.35-4.35"/>
      <path d="M11 8v6"/>
      <path d="M8 11h6"/>
    </svg>
  `;

  // Hover effects
  btn.onmouseenter = () => {
    btn.style.background =
      theme === "dark" ? "rgba(255,255,255,0.15)" : "rgba(0,0,0,0.08)";
  };
  btn.onmouseleave = () => {
    btn.style.background =
      theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(255,255,255,0.7)";
  };

  // Click handler - open preview window
  btn.onclick = async (e) => {
    e.stopPropagation();
    try {
      console.log("[ImagePreview] Opening preview window for item:", item.id);
      const label = await invoke<string>("preview_create_window", {
        imageData: item.content,
        title: `Image Preview - #${item.id}`,
      });
      console.log("[ImagePreview] Preview window created:", label);
    } catch (error) {
      console.error("[ImagePreview] Failed to create preview window:", error);
    }
  };

  return btn;
}

// Plugin definition
const plugin: Plugin = {
  meta: {
    id: "com.cliporax.imagepreview",
    name: "Image Preview",
    version: "1.0.0",
  },

  onActivate: (_ctx: PluginContext) => {
    console.log("[ImagePreview] Plugin activated");
  },

  onDeactivate: () => {
    console.log("[ImagePreview] Plugin deactivated");
  },

  extensions: {
    PreviewButton: {
      render: (props: ExtensionProps): HTMLElement | null => {
        const item = props.data?.item;
        const position = props.data?.position;
        const theme = props.context?.theme || "dark";

        // Only show for image items in the action position
        if (!item || item.type !== "image" || position !== "action") {
          return null;
        }

        return createPreviewButton(item, theme);
      },

      shouldShow: (data: unknown): boolean => {
        const props = data as ExtensionProps;
        return (
          props.data?.item?.type === "image" &&
          props.data?.position === "action"
        );
      },
    },
  },
};

// Register plugin globally
declare global {
  interface Window {
    CliporaxPlugins: Record<string, Plugin>;
  }
}

if (typeof window !== "undefined") {
  window.CliporaxPlugins = window.CliporaxPlugins || {};
  window.CliporaxPlugins["com.cliporax.imagepreview"] = plugin;
}

export default plugin;
