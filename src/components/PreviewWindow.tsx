import { useCallback, useEffect, useState } from "react";
import { Download, RotateCcw, ZoomIn, ZoomOut } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { preview, type PreviewData } from "../lib/tauri-api";
import { createLogger } from "../utils/logger";

const logger = createLogger("PreviewWindow");

function PreviewWindow() {
  const [data, setData] = useState<PreviewData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [zoomLevel, setZoomLevel] = useState(100);

  const loadPreviewData = useCallback(async () => {
    try {
      const nextData = await preview.getData();
      setData(nextData);
      setError(null);
      setZoomLevel(100);
    } catch (loadError) {
      logger.error("Failed to load preview data:", loadError);
      setError(String(loadError));
    }
  }, []);

  useEffect(() => {
    loadPreviewData();

    let disposed = false;
    let unlisten: (() => void) | null = null;

    listen("preview:updated", () => {
      if (!disposed) {
        void loadPreviewData();
      }
    })
      .then((dispose) => {
        if (disposed) {
          dispose();
        } else {
          unlisten = dispose;
        }
      })
      .catch((listenError) => {
        logger.error("Failed to listen for preview updates:", listenError);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadPreviewData]);

  const zoomIn = useCallback(() => {
    setZoomLevel((current) => Math.min(current + 25, 500));
  }, []);

  const zoomOut = useCallback(() => {
    setZoomLevel((current) => Math.max(current - 25, 25));
  }, []);

  const resetZoom = useCallback(() => {
    setZoomLevel(100);
  }, []);

  const saveImage = useCallback(() => {
    if (!data) return;
    const link = document.createElement("a");
    link.download = "clipboard-image.png";
    link.href = data.image_data;
    link.click();
  }, [data]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        void getCurrentWindow().close();
      }
      if (event.key === "+" || event.key === "=") {
        zoomIn();
      }
      if (event.key === "-") {
        zoomOut();
      }
      if (event.key === "0") {
        resetZoom();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [resetZoom, zoomIn, zoomOut]);

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-[#15161b] text-slate-200">
      <div
        className="flex min-h-0 flex-1 items-center justify-center overflow-hidden"
        onWheel={(event) => {
          event.preventDefault();
          if (event.deltaY < 0) {
            zoomIn();
          } else {
            zoomOut();
          }
        }}
      >
        {error ? (
          <div className="rounded-md border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-200">
            {error}
          </div>
        ) : data ? (
          <img
            className="max-h-full max-w-full object-contain transition-transform duration-200"
            src={data.image_data}
            alt={data.title || "Clipboard image preview"}
            style={{ transform: `scale(${zoomLevel / 100})` }}
            draggable={false}
          />
        ) : (
          <div className="text-sm text-slate-400">Loading preview...</div>
        )}
      </div>

      <div className="fixed bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-2 rounded-lg border border-white/10 bg-[#262a32]/95 px-3 py-2 shadow-2xl">
        <button
          type="button"
          className="flex h-9 w-9 items-center justify-center rounded-md text-slate-200 transition hover:bg-white/15"
          title="Zoom out"
          onClick={zoomOut}
        >
          <ZoomOut size={16} />
        </button>
        <span className="min-w-12 text-center text-xs font-medium text-slate-400">
          {zoomLevel}%
        </span>
        <button
          type="button"
          className="flex h-9 w-9 items-center justify-center rounded-md text-slate-200 transition hover:bg-white/15"
          title="Zoom in"
          onClick={zoomIn}
        >
          <ZoomIn size={16} />
        </button>
        <div className="mx-1 h-6 w-px bg-white/15" />
        <button
          type="button"
          className="flex h-9 w-9 items-center justify-center rounded-md text-slate-200 transition hover:bg-white/15"
          title="Reset zoom"
          onClick={resetZoom}
        >
          <RotateCcw size={16} />
        </button>
        <button
          type="button"
          className="flex h-9 w-9 items-center justify-center rounded-md text-slate-200 transition hover:bg-white/15 disabled:opacity-40"
          title="Save"
          onClick={saveImage}
          disabled={!data}
        >
          <Download size={16} />
        </button>
      </div>
    </div>
  );
}

export default PreviewWindow;
