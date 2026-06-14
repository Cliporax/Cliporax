/**
 * QR Code Scanner Plugin for Cliporax
 * Captures screen and scans for QR codes
 */

import jsQR from "jsqr";
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
  config?: Record<string, unknown>;
}

const SENSITIVE_PATTERN = /(password|code|otp|验证码|secret|key)/i;

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

let activeTheme = "dark";
let shortcutListener: ((event: Event) => void) | null = null;

// Scan QR codes from image data
function scanQRCodeFromImageData(
  imageData: ImageData,
): Array<{ data: string; location: jsQR.Location }> {
  try {
    const code = jsQR(imageData.data, imageData.width, imageData.height, {
      inversionAttempts: "attemptBoth",
    });

    if (code) {
      return [{ data: code.data, location: code.location }];
    }
    return [];
  } catch (error) {
    console.error("[QRScanner] Error scanning QR code:", error);
    return [];
  }
}

function canvasToImageData(canvas: HTMLCanvasElement): ImageData | null {
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  return ctx.getImageData(0, 0, canvas.width, canvas.height);
}

function cloneCanvas(
  source: HTMLCanvasElement,
  scale = 1,
  padding = 0,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(source.width * scale + padding * 2));
  canvas.height = Math.max(1, Math.round(source.height * scale + padding * 2));

  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return canvas;

  ctx.imageSmoothingEnabled = false;
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(
    source,
    padding,
    padding,
    Math.round(source.width * scale),
    Math.round(source.height * scale),
  );

  return canvas;
}

function thresholdCanvas(source: HTMLCanvasElement, threshold: number): HTMLCanvasElement {
  const canvas = cloneCanvas(source);
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return canvas;

  const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const data = imageData.data;

  for (let i = 0; i < data.length; i += 4) {
    const luminance = data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114;
    const value = luminance < threshold ? 0 : 255;
    data[i] = value;
    data[i + 1] = value;
    data[i + 2] = value;
    data[i + 3] = 255;
  }

  ctx.putImageData(imageData, 0, 0);
  return canvas;
}

async function scanWithBarcodeDetector(
  canvas: HTMLCanvasElement,
): Promise<Array<{ data: string; location: jsQR.Location }>> {
  const BarcodeDetectorCtor = (window as unknown as {
    BarcodeDetector?: new (options?: { formats?: string[] }) => {
      detect: (source: HTMLCanvasElement | ImageBitmap | Blob) => Promise<Array<{ rawValue: string }>>;
    };
  }).BarcodeDetector;

  if (!BarcodeDetectorCtor) {
    return [];
  }

  try {
    const detector = new BarcodeDetectorCtor({ formats: ["qr_code"] });
    const codes = await detector.detect(canvas);
    const rawValue = codes.find((code) => code.rawValue)?.rawValue;

    if (!rawValue) {
      return [];
    }

    return [
      {
        data: rawValue,
        location: {
          topLeftCorner: { x: 0, y: 0 },
          topRightCorner: { x: canvas.width, y: 0 },
          bottomLeftCorner: { x: 0, y: canvas.height },
          bottomRightCorner: { x: canvas.width, y: canvas.height },
          topLeftFinderPattern: { x: 0, y: 0 },
          topRightFinderPattern: { x: canvas.width, y: 0 },
          bottomLeftFinderPattern: { x: 0, y: canvas.height },
          bottomRightAlignmentPattern: { x: canvas.width, y: canvas.height },
        },
      },
    ];
  } catch (error) {
    console.warn("[QRScanner] BarcodeDetector scan failed:", error);
    return [];
  }
}

async function scanQRCodeFromCanvas(
  canvas: HTMLCanvasElement,
): Promise<Array<{ data: string; location: jsQR.Location }>> {
  const nativeResult = await scanWithBarcodeDetector(canvas);
  if (nativeResult.length > 0) {
    return nativeResult;
  }

  const variants: HTMLCanvasElement[] = [
    canvas,
    cloneCanvas(canvas, 1, 24),
    cloneCanvas(canvas, 2, 48),
    cloneCanvas(canvas, 3, 72),
    thresholdCanvas(canvas, 96),
    thresholdCanvas(canvas, 128),
    thresholdCanvas(canvas, 160),
  ];

  for (const variant of variants) {
    const imageData = canvasToImageData(variant);
    if (!imageData) continue;

    const qrCodes = scanQRCodeFromImageData(imageData);
    if (qrCodes.length > 0) {
      return qrCodes;
    }
  }

  return [];
}

// Capture screen using canvas
async function captureScreen(): Promise<HTMLCanvasElement | null> {
  try {
    if (!navigator.mediaDevices?.getDisplayMedia) {
      throw new Error("Screen capture is not available in this WebView");
    }

    const stream = await navigator.mediaDevices.getDisplayMedia({
      video: {
        cursor: "never",
      },
    });

    const video = document.createElement("video");
    video.srcObject = stream;

    await new Promise<void>((resolve) => {
      video.onloadedmetadata = () => {
        video.play();
        resolve();
      };
    });

    await new Promise((resolve) => setTimeout(resolve, 300));

    const canvas = document.createElement("canvas");
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;

    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) {
      throw new Error("Cannot get canvas context");
    }

    ctx.drawImage(video, 0, 0);

    const tracks = stream.getTracks();
    tracks.forEach((track) => track.stop());

    return canvas;
  } catch (error) {
    console.error("[QRScanner] Screen capture failed:", error);
    return null;
  }
}

function imageDataUrlToCanvas(dataUrl: string): Promise<HTMLCanvasElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;

      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      if (!ctx) {
        reject(new Error("Cannot get canvas context"));
        return;
      }

      ctx.drawImage(img, 0, 0);
      resolve(canvas);
    };
    img.onerror = () => reject(new Error("Failed to load captured image"));
    img.src = dataUrl;
  });
}

async function copyTextToClipboard(text: string): Promise<void> {
  await invoke("clipboard_write_text_and_create", {
    content: text,
    metadata: JSON.stringify({
      source: "QR Scanner",
      source_app: "Cliporax",
      window_title: "QR Code Scanner",
      timestamp: new Date().toISOString(),
    }),
    tags: JSON.stringify(["qrscanner"]),
    isSensitive: SENSITIVE_PATTERN.test(text) ? 1 : 0,
  });
}

function showScannerToast(
  message: string,
  theme: string,
  type: "info" | "success" | "warning" | "error" = "info",
): void {
  const existingToast = document.getElementById("qrscanner-toast");
  if (existingToast) {
    existingToast.remove();
  }

  const toast = document.createElement("div");
  toast.id = "qrscanner-toast";

  const colors = {
    info: "#3b82f6",
    success: "#10b981",
    warning: "#f59e0b",
    error: "#ef4444",
  };

  toast.style.cssText = `
    position: fixed;
    right: 16px;
    top: 16px;
    z-index: 10001;
    max-width: min(420px, calc(100vw - 32px));
    padding: 12px 14px;
    border-radius: 8px;
    border-left: 4px solid ${colors[type]};
    background: ${theme === "dark" ? "#111827" : "#ffffff"};
    color: ${theme === "dark" ? "#f3f4f6" : "#111827"};
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.28);
    font-size: 13px;
    line-height: 1.45;
    word-break: break-word;
  `;
  toast.textContent = message;

  document.body.appendChild(toast);
  setTimeout(() => {
    toast.remove();
  }, type === "error" ? 7000 : 4200);
}

async function scanSelectedRegion(theme: string): Promise<void> {
  showScannerToast("Drag to select the QR code region...", theme, "info");

  try {
    const dataUrl = await invoke<string>("qrscanner_capture_region");
    const canvas = await imageDataUrlToCanvas(dataUrl);
    const qrCodes = await scanQRCodeFromCanvas(canvas);

    if (qrCodes.length === 0) {
      showScannerToast("No QR code found in the selected region.", theme, "warning");
      return;
    }

    await copyTextToClipboard(qrCodes[0].data);
    showScannerToast(`QR code copied: ${qrCodes[0].data}`, theme, "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error("[QRScanner] Region scan failed:", error);

    if (message.includes("not implemented on Windows")) {
      showScannerToast("Opening screen capture fallback...", theme, "warning");
      showScannerModal(theme);
      return;
    }

    showScannerToast(`QR scan failed: ${message}`, theme, "error");
  }
}

// Show scanner UI
function showScannerModal(theme: string): Promise<string | null> {
  return new Promise((resolve) => {
    const isDark = theme === "dark";

    const existingModal = document.getElementById("qrscanner-modal");
    if (existingModal) {
      existingModal.remove();
    }

    const modal = document.createElement("div");
    modal.id = "qrscanner-modal";
    modal.style.cssText = `
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      bottom: 0;
      background: rgba(0, 0, 0, 0.8);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 10000;
      backdrop-filter: blur(8px);
    `;

    const container = document.createElement("div");
    container.style.cssText = `
      background: ${isDark ? "#1f2937" : "#fff"};
      border-radius: 16px;
      padding: 24px;
      box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
      max-width: 600px;
      width: 90%;
      text-align: center;
    `;

    const title = document.createElement("h3");
    title.style.cssText = `
      color: ${isDark ? "#f3f4f6" : "#1f2937"};
      margin: 0 0 8px 0;
      font-size: 20px;
      font-weight: 600;
    `;
    title.textContent = "Scan QR Code from Screen";

    const subtitle = document.createElement("p");
    subtitle.style.cssText = `
      color: ${isDark ? "#9ca3af" : "#6b7280"};
      font-size: 13px;
      margin: 0 0 20px 0;
    `;
    subtitle.textContent = "Select the screen or window to capture";

    const previewContainer = document.createElement("div");
    previewContainer.style.cssText = `
      width: 100%;
      height: 300px;
      border-radius: 8px;
      margin: 0 auto 16px;
      background: ${isDark ? "#374151" : "#f3f4f6"};
      display: flex;
      align-items: center;
      justify-content: center;
      overflow: hidden;
      position: relative;
    `;

    const statusText = document.createElement("span");
    statusText.style.cssText = `
      color: ${isDark ? "#9ca3af" : "#6b7280"};
      font-size: 14px;
    `;
    statusText.textContent = "Click 'Start Capture' to begin";
    previewContainer.appendChild(statusText);

    const resultsContainer = document.createElement("div");
    resultsContainer.style.cssText = `
      max-height: 150px;
      overflow-y: auto;
      margin: 0 0 16px 0;
      text-align: left;
    `;
    resultsContainer.style.display = "none";

    const btnContainer = document.createElement("div");
    btnContainer.style.cssText =
      "display: flex; gap: 8px; justify-content: center; flex-wrap: wrap;";

    const captureBtn = document.createElement("button");
    captureBtn.style.cssText = `
      padding: 10px 20px;
      background: #3b82f6;
      color: white;
      border: none;
      border-radius: 8px;
      cursor: pointer;
      font-size: 14px;
      font-weight: 500;
    `;
    captureBtn.textContent = "Start Capture";

    const closeBtn = document.createElement("button");
    closeBtn.style.cssText = `
      padding: 10px 20px;
      background: ${isDark ? "#374151" : "#f3f4f6"};
      color: ${isDark ? "#f3f4f6" : "#1f2937"};
      border: none;
      border-radius: 8px;
      cursor: pointer;
      font-size: 14px;
      font-weight: 500;
    `;
    closeBtn.textContent = "Close";

    let isScanning = false;

    captureBtn.onclick = async () => {
      if (isScanning) return;
      isScanning = true;

      captureBtn.disabled = true;
      captureBtn.textContent = "Capturing...";
      statusText.textContent = "Capturing screen...";

      try {
        const canvas = await captureScreen();

        if (!canvas) {
          statusText.textContent = "Failed to capture screen. Please try again.";
          captureBtn.disabled = false;
          captureBtn.textContent = "Start Capture";
          isScanning = false;
          return;
        }

        statusText.textContent = "Scanning for QR codes...";

        const img = document.createElement("img");
        img.src = canvas.toDataURL();
        img.style.cssText = "max-width: 100%; max-height: 100%; object-fit: contain;";
        previewContainer.innerHTML = "";
        previewContainer.appendChild(img);

        const ctx = canvas.getContext("2d", { willReadFrequently: true });
        if (!ctx) {
          statusText.textContent = "Failed to process image.";
          captureBtn.disabled = false;
          captureBtn.textContent = "Start Capture";
          isScanning = false;
          return;
        }

        const qrCodes = await scanQRCodeFromCanvas(canvas);

        if (qrCodes.length > 0) {
          statusText.textContent = `Found ${qrCodes.length} QR code(s)!`;
          statusText.style.color = "#10b981";

          resultsContainer.style.display = "block";
          resultsContainer.innerHTML = "";

          qrCodes.forEach((qr, index) => {
            const resultItem = document.createElement("div");
            resultItem.style.cssText = `
              padding: 10px;
              margin-bottom: 8px;
              background: ${isDark ? "#374151" : "#f9fafb"};
              border-radius: 6px;
              border-left: 3px solid #10b981;
            `;

            const resultText = document.createElement("p");
            resultText.style.cssText = `
              color: ${isDark ? "#f3f4f6" : "#1f2937"};
              font-size: 12px;
              margin: 0 0 8px 0;
              word-break: break-all;
              max-height: 60px;
              overflow: hidden;
            `;
            resultText.textContent = qr.data;

            const copyBtn = document.createElement("button");
            copyBtn.style.cssText = `
              padding: 4px 12px;
              background: #10b981;
              color: white;
              border: none;
              border-radius: 4px;
              cursor: pointer;
              font-size: 12px;
            `;
            copyBtn.textContent = "Copy to Clipboard";
            copyBtn.onclick = async () => {
              try {
                await copyTextToClipboard(qr.data);
                copyBtn.textContent = "Copied!";
                copyBtn.style.background = "#059669";
                setTimeout(() => {
                  copyBtn.textContent = "Copy to Clipboard";
                  copyBtn.style.background = "#10b981";
                }, 2000);
              } catch (error) {
                console.error("[QRScanner] Failed to copy:", error);
                copyBtn.textContent = "Failed";
                copyBtn.style.background = "#ef4444";
              }
            };

            resultItem.appendChild(resultText);
            resultItem.appendChild(copyBtn);
            resultsContainer.appendChild(resultItem);
          });
        } else {
          statusText.textContent = "No QR codes found. Please try again.";
          statusText.style.color = "#f59e0b";
        }

        captureBtn.disabled = false;
        captureBtn.textContent = "Capture Again";
        isScanning = false;
      } catch (error) {
        console.error("[QRScanner] Error during capture:", error);
        statusText.textContent = "Error: " + (error as Error).message;
        statusText.style.color = "#ef4444";
        captureBtn.disabled = false;
        captureBtn.textContent = "Start Capture";
        isScanning = false;
      }
    };

    closeBtn.onclick = () => {
      modal.remove();
      resolve(null);
    };

    btnContainer.appendChild(captureBtn);
    btnContainer.appendChild(closeBtn);

    container.appendChild(title);
    container.appendChild(subtitle);
    container.appendChild(previewContainer);
    container.appendChild(resultsContainer);
    container.appendChild(btnContainer);
    modal.appendChild(container);

    modal.onclick = (e) => {
      if (e.target === modal) {
        modal.remove();
        resolve(null);
      }
    };

    document.body.appendChild(modal);
  });
}

// Create scanner button element
function createScannerButton(theme: string): HTMLElement {
  const btn = document.createElement("button");
  btn.style.cssText = `
    width: 100%;
    padding: 12px;
    border-radius: 8px;
    border: none;
    background: ${theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.05)"};
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: ${theme === "dark" ? "#e2e8f0" : "#52525b"};
    transition: all 0.15s ease;
    font-size: 14px;
    font-weight: 500;
  `;
  btn.title = "Select a screen region and scan QR code";
  btn.innerHTML = `
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M3 7V5a2 2 0 0 1 2-2h2"/>
      <path d="M17 3h2a2 2 0 0 1 2 2v2"/>
      <path d="M21 17v2a2 2 0 0 1-2 2h-2"/>
      <path d="M7 21H5a2 2 0 0 1-2-2v-2"/>
      <rect x="7" y="7" width="10" height="10" rx="1"/>
      <line x1="12" y1="12" x2="12" y2="12.01"/>
    </svg>
    Scan QR Region
  `;

  btn.onmouseenter = () => {
    btn.style.background =
      theme === "dark" ? "rgba(255,255,255,0.15)" : "rgba(0,0,0,0.08)";
  };
  btn.onmouseleave = () => {
    btn.style.background =
      theme === "dark" ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.05)";
  };
  btn.onclick = (e) => {
    e.stopPropagation();
    scanSelectedRegion(theme);
  };

  return btn;
}

// Plugin definition
const plugin: Plugin = {
  meta: {
    id: "com.cliporax.qrscanner",
    name: "QR Code Scanner",
    version: "1.0.0",
  },

  onActivate: (ctx: PluginContext) => {
    console.log("[QRScanner] Activated");
    activeTheme = ctx.theme || "dark";

    if (!shortcutListener) {
      shortcutListener = (event: Event) => {
        const detail = (event as CustomEvent).detail;
        if (detail?.pluginId === "com.cliporax.qrscanner") {
          scanSelectedRegion(activeTheme);
        }
      };
      window.addEventListener("cliporax:plugin-shortcut", shortcutListener);
    }
  },

  onDeactivate: () => {
    console.log("[QRScanner] Deactivated");
    if (shortcutListener) {
      window.removeEventListener("cliporax:plugin-shortcut", shortcutListener);
      shortcutListener = null;
    }
    const modal = document.getElementById("qrscanner-modal");
    if (modal) {
      modal.remove();
    }
  },

  extensions: {
    QRScannerPanel: {
      render: (props: ExtensionProps): HTMLElement | null => {
        const theme = props.context?.theme || "dark";
        activeTheme = theme;

        if (props.data?.position !== "sidebar") {
          return null;
        }

        return createScannerButton(theme);
      },

      shouldShow: (data: unknown): boolean => {
        const props = data as ExtensionProps;
        return props.data?.position === "sidebar";
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
  window.CliporaxPlugins["com.cliporax.qrscanner"] = plugin;
}

export default plugin;
