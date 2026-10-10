import { useCallback, useEffect, useRef, useState } from "react";
import { Download, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { appUpdate, type AppUpdateStatus } from "../../lib/tauri-api";

const initialStatus: AppUpdateStatus = {
  phase: "idle", version: null, fileName: null, downloaded: 0, total: 0,
  lastChecked: null, error: null, manualInstall: false, installerOpened: false, packagePath: null,
};

export default function AppUpdateCard({ isDark }: { isDark: boolean }) {
  const { t } = useTranslation();
  const [status, setStatus] = useState(initialStatus);
  const [requestError, setRequestError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [showInstall, setShowInstall] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const prompted = useRef<string | null>(null);
  const mounted = useRef(false);
  const requestId = useRef(0);
  const actionPending = useRef(false);
  const busy = pending || ["checking", "downloading", "installing"].includes(status.phase);
  const key = (name: string) => `settings.about.${name}`;

  // Backend state keeps downloads alive across tab/window closure. Sequence reads so
  // a slow poll cannot overwrite the result of a later user action.
  useEffect(() => {
    mounted.current = true;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      const id = ++requestId.current;
      try {
        const next = await appUpdate.status();
        if (!stopped && id === requestId.current) {
          setStatus(next);
          setLoaded(true);
        }
      } catch (error) {
        if (!stopped && id === requestId.current) {
          setRequestError(String(error));
          setLoaded(true);
        }
      }
      if (!stopped) timer = setTimeout(refresh, 750);
    };
    void refresh();
    return () => { stopped = true; mounted.current = false; clearTimeout(timer); };
  }, []);

  useEffect(() => {
    if (status.phase === "available") prompted.current = null;
    if (status.phase === "ready" && status.fileName && !status.installerOpened && prompted.current !== status.fileName) {
      prompted.current = status.fileName;
      setShowInstall(true);
    }
  }, [status.phase, status.fileName, status.installerOpened]);

  useEffect(() => {
    if (showInstall) dialog.current?.showModal();
    else dialog.current?.close();
  }, [showInstall]);

  const run = useCallback(async (operation: () => Promise<AppUpdateStatus>) => {
    if (actionPending.current) return;
    actionPending.current = true;
    ++requestId.current;
    setPending(true);
    setRequestError(null);
    try {
      const next = await operation();
      ++requestId.current;
      if (mounted.current) setStatus(next);
    } catch (error) {
      if (mounted.current) setRequestError(String(error));
    } finally {
      actionPending.current = false;
      if (mounted.current) setPending(false);
    }
  }, []);

  const progress = status.total > 0 ? Math.min(100, Math.floor(status.downloaded / status.total * 100)) : 0;
  const error = requestError || status.error;
  let description = t(key("lastChecked"));
  if (status.lastChecked) description = t(key("checkedAt"), { time: new Date(status.lastChecked).toLocaleString() });
  if (status.phase === "checking") description = t(key("checking"));
  if (status.phase === "current") description = t(key("upToDate"));
  if (status.phase === "available") description = t(key("updateAvailable"), { version: status.version });
  if (status.phase === "downloading") description = t(key("downloadProgress"), { progress });
  if (status.phase === "ready") description = t(key(status.installerOpened ? "installerOpened" : "downloadReady"), { version: status.version });
  if (status.phase === "installing") description = t(key("openingInstaller"));
  const buttonClass = "inline-flex shrink-0 items-center justify-center gap-2 px-4 py-2 text-xs font-medium bg-blue-500/15 hover:bg-blue-500/25 text-blue-500 border border-blue-500/20 rounded-lg transition-all disabled:opacity-50 disabled:cursor-not-allowed";

  return (
    <div className="p-4 rounded-xl space-y-3" style={{
      backgroundColor: isDark ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.6)",
      border: `1px solid ${isDark ? "rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)"}`,
      color: isDark ? "#cbd5e1" : "#5a5a58",
    }}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <span className="text-sm font-medium">{t(key("checkUpdate"))}</span>
          <p role="status" className="text-xs mt-1" style={{ color: isDark ? "#94a3b8" : "#6b7280" }}>{description}</p>
        </div>
        <button type="button" className={buttonClass} disabled={busy || !loaded} onClick={() => {
          if (status.phase === "ready") setShowInstall(true);
          else void run(status.phase === "available" ? appUpdate.download : appUpdate.check);
        }}>
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
          {t(key(busy ? "pleaseWait" : status.phase === "ready" ? (status.manualInstall ? "openDownload" : "installNow") : status.phase === "available" ? "downloadUpdate" : "checkNow"))}
        </button>
      </div>
      {status.phase === "downloading" && <progress aria-label={t(key("downloadUpdate"))} value={status.downloaded} max={status.total || 1} className="w-full h-2 accent-blue-500" />}
      {status.fileName && <p className="text-xs break-all opacity-70">{status.fileName}</p>}
      {error && <p role="alert" className="text-xs text-red-500 break-words">{t(key("updateError"), { error })}</p>}
      {status.packagePath && <p className="text-xs break-all opacity-70">{t(key("savedTo"), { path: status.packagePath })}</p>}
      <dialog ref={dialog} aria-labelledby="app-update-title" onCancel={() => setShowInstall(false)} onClose={() => setShowInstall(false)}
        className="fixed inset-0 m-auto w-[calc(100%-2rem)] max-w-sm rounded-xl p-5 shadow-xl backdrop:bg-black/50"
        style={{ backgroundColor: isDark ? "#1e293b" : "#ffffff", color: isDark ? "#e2e8f0" : "#374151" }}>
        <h3 id="app-update-title" className="text-sm font-semibold">{t(key("downloadReady"), { version: status.version })}</h3>
        <p className="text-xs leading-relaxed mt-3">{t(key(status.manualInstall ? "manualInstallMessage" : "installMessage"))}</p>
        <div className="flex flex-wrap justify-end gap-2 mt-5">
          <button type="button" autoFocus className="px-3 py-2 text-xs rounded-lg hover:bg-gray-500/15" onClick={() => setShowInstall(false)}>{t(key("installLater"))}</button>
          <button type="button" className={buttonClass} disabled={busy} onClick={() => {
            setShowInstall(false);
            void run(appUpdate.install);
          }}>{t(key(status.manualInstall ? "openDownload" : "installNow"))}</button>
        </div>
      </dialog>
    </div>
  );
}
