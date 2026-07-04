/**
 * QR Code Generator Plugin for Cliporax
 * TypeScript Implementation
 */

import QRCode from "qrcode";

// Plugin types
interface PluginContext {
  theme?: "dark" | "light" | "system";
  [key: string]: unknown;
}

interface ClipboardItem {
  id: string;
  type: "text" | "image" | "file";
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

// Generate QR code as data URL
async function generateQRCode(
  text: string,
  size: number = 256,
): Promise<string> {
  try {
    return await QRCode.toDataURL(text, {
      width: size,
      margin: 4,
      color: {
        dark: "#000000",
        light: "#ffffff",
      },
      errorCorrectionLevel: "M",
    });
  } catch (error) {
    console.error("[QRCodePlugin] Error generating QR code:", error);
    throw error;
  }
}

// Show QR code modal
function showQRCodeModal(content: string, theme: string): void {
  // Remove existing modal
  const existingModal = document.getElementById("qrcode-modal");
  if (existingModal) {
    existingModal.remove();
  }

  const isDark = theme === "dark";

  // Create modal
  const modal = document.createElement("div");
  modal.id = "qrcode-modal";
  modal.style.cssText = `
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10000;
    backdrop-filter: blur(4px);
  `;

  const container = document.createElement("div");
  container.style.cssText = `
    background: ${isDark ? "#1f2937" : "#fff"};
    border-radius: 16px;
    padding: 24px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
    max-width: 320px;
    width: 90%;
    text-align: center;
  `;

  const title = document.createElement("h3");
  title.style.cssText = `
    color: ${isDark ? "#f3f4f6" : "#1f2937"};
    margin: 0 0 16px 0;
    font-size: 18px;
    font-weight: 600;
  `;
  title.textContent = "QR Code";

  // Loading placeholder
  const imgContainer = document.createElement("div");
  imgContainer.style.cssText = `
    width: 200px;
    height: 200px;
    border-radius: 8px;
    margin: 0 auto 16px;
    background: white;
    padding: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
  `;

  const loadingText = document.createElement("span");
  loadingText.textContent = "Generating...";
  loadingText.style.color = "#666";
  imgContainer.appendChild(loadingText);

  const textPreview = document.createElement("p");
  textPreview.style.cssText = `
    color: ${isDark ? "#9ca3af" : "#6b7280"};
    font-size: 12px;
    margin: 0 0 16px 0;
    word-break: break-all;
    max-height: 60px;
    overflow: hidden;
  `;
  textPreview.textContent =
    content.length > 100 ? content.substring(0, 100) + "..." : content;

  const btnContainer = document.createElement("div");
  btnContainer.style.cssText =
    "display: flex; gap: 8px; justify-content: center;";

  const dlBtn = document.createElement("button");
  dlBtn.style.cssText = `
    padding: 8px 16px;
    background: #3b82f6;
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    font-size: 14px;
    font-weight: 500;
  `;
  dlBtn.textContent = "Download";
  dlBtn.disabled = true;

  const closeBtn = document.createElement("button");
  closeBtn.style.cssText = `
    padding: 8px 16px;
    background: ${isDark ? "#374151" : "#f3f4f6"};
    color: ${isDark ? "#f3f4f6" : "#1f2937"};
    border: none;
    border-radius: 8px;
    cursor: pointer;
    font-size: 14px;
    font-weight: 500;
  `;
  closeBtn.textContent = "Close";
  closeBtn.onclick = () => modal.remove();

  // Generate QR code
  let qrDataUrl: string;
  generateQRCode(content, 256)
    .then((url) => {
      qrDataUrl = url;
      const img = document.createElement("img");
      img.src = url;
      img.style.cssText = "width: 100%; height: 100%; object-fit: contain;";
      imgContainer.innerHTML = "";
      imgContainer.appendChild(img);
      dlBtn.disabled = false;
    })
    .catch((error) => {
      loadingText.textContent = "Failed to generate";
      loadingText.style.color = "#ef4444";
      console.error("[QRCodePlugin] Error:", error);
    });

  dlBtn.onclick = () => {
    if (qrDataUrl) {
      const link = document.createElement("a");
      link.download = "qrcode.png";
      link.href = qrDataUrl;
      link.click();
    }
  };

  btnContainer.appendChild(dlBtn);
  btnContainer.appendChild(closeBtn);
  container.appendChild(title);
  container.appendChild(imgContainer);
  container.appendChild(textPreview);
  container.appendChild(btnContainer);
  modal.appendChild(container);
  modal.onclick = (e) => {
    if (e.target === modal) modal.remove();
  };

  document.body.appendChild(modal);
}

// Create QR code button element
function createQRCodeButton(item: ClipboardItem, theme: string): HTMLElement {
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
  btn.title = "Generate QR Code";
  btn.innerHTML = `
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="3" y="3" width="7" height="7"/>
      <rect x="14" y="3" width="7" height="7"/>
      <rect x="3" y="14" width="7" height="7"/>
      <rect x="14" y="14" width="7" height="7"/>
    </svg>
  `;

  btn.onmouseenter = () => {
    btn.style.background =
      theme === "dark" ? "rgba(255,255,255,0.15)" : "rgba(0,0,0,0.08)";
  };
  btn.onmouseleave = () => {
    btn.style.background =
      theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(255,255,255,0.7)";
  };
  btn.onclick = (e) => {
    e.stopPropagation();
    showQRCodeModal(item.content, theme);
  };

  return btn;
}

// Plugin definition
const plugin: Plugin = {
  meta: {
    id: "com.cliporax.qrcode",
    name: "QR Code Generator",
    version: "1.0.0",
  },

  onActivate: (ctx: PluginContext) => {
    console.log("[QRCodePlugin] Activated");
  },

  onDeactivate: () => {
    console.log("[QRCodePlugin] Deactivated");
  },

  extensions: {
    QRCodeButton: {
      render: (props: ExtensionProps): HTMLElement | null => {
        const item = props.data?.item;
        const position = props.data?.position;
        const theme = props.context?.theme || "dark";

        if (!item || (item.type !== "text" && item.type !== "file") || position !== "action") {
          return null;
        }

        return createQRCodeButton(item, theme);
      },

      shouldShow: (data: unknown): boolean => {
        const props = data as ExtensionProps;
        const item = props.data?.item;
        const position = props.data?.position;
        return (item?.type === "text" || item?.type === "file") && position === "action";
      },
    },
  },
};

// Export for different environments
declare global {
  interface Window {
    CliporaxPlugins: Record<string, Plugin>;
  }
}

// Register plugin
if (typeof window !== "undefined") {
  window.CliporaxPlugins = window.CliporaxPlugins || {};
  window.CliporaxPlugins["com.cliporax.qrcode"] = plugin;
}

export default plugin;
