import React, { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  ChevronRight,
  Clock3,
  Cloud,
  Database,
  FileWarning,
  FolderSync,
  KeyRound,
  Loader2,
  Lock,
  Pause,
  Play,
  Plug,
  RefreshCw,
  RotateCcw,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  TerminalSquare,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  sync,
  type ConflictResolutionInput,
  type SyncConflict,
  type SyncLogEntry,
  type SyncPluginOption,
  type SyncProfile,
  type SyncRunReport,
  type SyncStatus,
  type SyncTabOption,
} from "../../lib/tauri-api";

interface CloudSyncTabProps {
  isDark: boolean;
}

type Provider = "webdav" | "sftp" | "google_drive" | "one_drive";
type SyncSection = "setup" | "scope" | "security" | "status" | "conflicts" | "logs";
type SyncMode = "all" | "selected";
type SavedCredentialRefs = Partial<Record<"username" | "password" | "private_key" | "passphrase", string>>;

interface FormState {
  profileName: string;
  provider: Provider;
  serverUrl: string;
  username: string;
  password: string;
  privateKey: string;
  passphrase: string;
  remoteRoot: string;
  sftpPort: number;
  authMethod: "password" | "privateKey";
  tabMode: SyncMode;
  selectedTabs: number[];
  pluginMode: SyncMode;
  selectedPlugins: string[];
  encryptionEnabled: boolean;
  uploadSensitiveItems: boolean;
  syncOnStartup: boolean;
  syncOnLocalChange: boolean;
  intervalMinutes: number;
  paused: boolean;
}

const DEFAULT_FORM: FormState = {
  profileName: "Personal WebDAV",
  provider: "webdav",
  serverUrl: "https://dav.example.com/remote.php/dav/files/me",
  username: "",
  password: "",
  privateKey: "",
  passphrase: "",
  remoteRoot: "/Cliporax/v1",
  sftpPort: 22,
  authMethod: "password",
  tabMode: "selected",
  selectedTabs: [1, 2],
  pluginMode: "selected",
  selectedPlugins: ["com.cliporax.qrcode"],
  encryptionEnabled: false,
  uploadSensitiveItems: false,
  syncOnStartup: true,
  syncOnLocalChange: true,
  intervalMinutes: 15,
  paused: false,
};

const PROVIDER_DEFAULTS: Record<
  Provider,
  {
    profileName: string;
    serverUrl: string;
    remoteRoot: string;
    authMethod: FormState["authMethod"];
  }
> = {
  webdav: {
    profileName: "Personal WebDAV",
    serverUrl: "https://dav.example.com/remote.php/dav/files/me",
    remoteRoot: "/Cliporax/v1",
    authMethod: "password",
  },
  sftp: {
    profileName: "Personal SFTP",
    serverUrl: "sftp.example.com",
    remoteRoot: "/Cliporax/v1",
    authMethod: "password",
  },
  google_drive: {
    profileName: "Google Drive Sync",
    serverUrl: "",
    remoteRoot: "Cliporax/v1",
    authMethod: "password",
  },
  one_drive: {
    profileName: "OneDrive Sync",
    serverUrl: "",
    remoteRoot: "Cliporax/v1",
    authMethod: "password",
  },
};

const PROVIDER_LABELS: Record<Provider, string> = {
  webdav: "WebDAV",
  sftp: "SFTP",
  google_drive: "Google Drive",
  one_drive: "OneDrive",
};

const CLOUD_TOKEN_PROVIDERS = new Set<Provider>(["google_drive", "one_drive"]);

const normalizeProvider = (provider: string | null | undefined): Provider => {
  switch (provider) {
    case "webdav":
    case "web_dav":
      return "webdav";
    case "sftp":
      return "sftp";
    case "google_drive":
      return "google_drive";
    case "one_drive":
      return "one_drive";
    default:
      return DEFAULT_FORM.provider;
  }
};

const formatSyncDate = (value: string | null | undefined) => {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
};

const DEFAULT_PROFILE_ID = "default-cloud-sync";

const buildSftpRemoteRoot = (host: string, port: number, remoteRoot: string) => {
  const trimmedHost = host.trim().replace(/^sftp:\/\//, "").replace(/\/.*$/, "");
  const normalizedRoot = remoteRoot.trim().startsWith("/")
    ? remoteRoot.trim()
    : `/${remoteRoot.trim() || "Cliporax/v1"}`;
  return `sftp://${trimmedHost}:${port}${normalizedRoot}`;
};

const parseSftpRemoteRoot = (remoteRoot: string) => {
  const withoutScheme = remoteRoot.replace(/^sftp:\/\//, "");
  const slashIndex = withoutScheme.indexOf("/");
  const hostPort = slashIndex >= 0 ? withoutScheme.slice(0, slashIndex) : withoutScheme;
  const path = slashIndex >= 0 ? withoutScheme.slice(slashIndex) : "/Cliporax/v1";
  const colonIndex = hostPort.lastIndexOf(":");

  if (colonIndex < 0) {
    return { host: hostPort, port: 22, path };
  }

  const port = Number(hostPort.slice(colonIndex + 1));
  return {
    host: hostPort.slice(0, colonIndex),
    port: Number.isFinite(port) ? port : 22,
    path,
  };
};

const formFromProfile = (profile: SyncProfile): FormState => {
  const provider = normalizeProvider(profile.provider);
  const sftpRemote = provider === "sftp" ? parseSftpRemoteRoot(profile.remote_root) : null;

  return {
    ...DEFAULT_FORM,
    ...PROVIDER_DEFAULTS[provider],
    profileName: profile.name,
    provider,
    serverUrl:
      provider === "sftp"
        ? sftpRemote?.host ?? ""
        : provider === "webdav"
          ? profile.remote_root
          : PROVIDER_DEFAULTS[provider].serverUrl,
    sftpPort: provider === "sftp" ? sftpRemote?.port ?? DEFAULT_FORM.sftpPort : DEFAULT_FORM.sftpPort,
    remoteRoot:
      provider === "sftp"
        ? sftpRemote?.path ?? PROVIDER_DEFAULTS.sftp.remoteRoot
        : provider === "webdav"
          ? ""
          : profile.remote_root,
    tabMode: profile.sync_tabs.mode,
    selectedTabs: profile.sync_tabs.selected_tab_ids,
    pluginMode: profile.sync_plugins.mode,
    selectedPlugins: profile.sync_plugins.selected_plugin_ids,
    encryptionEnabled: profile.encryption.enabled,
    syncOnStartup: profile.schedule.sync_on_startup,
    syncOnLocalChange: profile.schedule.sync_on_local_change,
    intervalMinutes: profile.schedule.interval_minutes,
    paused: Boolean(profile.schedule.paused),
  };
};

const resolutionByLabelKey: Record<string, ConflictResolutionInput> = {
  keepLocal: "use_local",
  keepRemote: "use_remote",
  keepBoth: "keep_both",
};

const messageTone = (message: string) => {
  const lower = message.toLowerCase();
  if (
    lower.includes("failed") ||
    lower.includes("unavailable") ||
    lower.includes("before") ||
    lower.includes("required")
  ) {
    return "error";
  }
  return "info";
};

const isUntrustedSftpHostMessage = (message: string) =>
  message.includes("SFTP host key") && message.includes("not trusted");

const CloudSyncTab: React.FC<CloudSyncTabProps> = ({ isDark }) => {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<SyncSection | null>(null);
  const [testingConnection, setTestingConnection] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [activeProfileId, setActiveProfileId] = useState<string | null>(null);
  const [tabOptions, setTabOptions] = useState<SyncTabOption[]>([]);
  const [pluginOptions, setPluginOptions] = useState<SyncPluginOption[]>([]);
  const [conflicts, setConflicts] = useState<SyncConflict[]>([]);
  const [logEntries, setLogEntries] = useState<SyncLogEntry[]>([]);
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null);
  const [lastReport, setLastReport] = useState<SyncRunReport | null>(null);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [savedCredentialRefs, setSavedCredentialRefs] = useState<SavedCredentialRefs>({});
  const [pendingSftpHostTrust, setPendingSftpHostTrust] = useState<{
    profileId: string;
    host: string;
    port: number;
    message: string;
  } | null>(null);
  const [trustingSftpHostKey, setTrustingSftpHostKey] = useState(false);
  const [unlockPassword, setUnlockPassword] = useState("");
  const [rememberUnlock, setRememberUnlock] = useState(false);
  const [form, setForm] = useState<FormState>(DEFAULT_FORM);

  const palette = useMemo(
    () => ({
      panel: isDark ? "rgba(15,23,42,0.46)" : "rgba(255,255,255,0.78)",
      panelSoft: isDark ? "rgba(255,255,255,0.05)" : "rgba(248,250,252,0.86)",
      panelStrong: isDark ? "rgba(20,184,166,0.13)" : "rgba(20,184,166,0.08)",
      border: isDark ? "rgba(255,255,255,0.08)" : "rgba(15,23,42,0.08)",
      borderStrong: isDark ? "rgba(20,184,166,0.32)" : "rgba(20,184,166,0.28)",
      text: isDark ? "#e2e8f0" : "#3f3f46",
      muted: isDark ? "#94a3b8" : "#71717a",
      faint: isDark ? "#64748b" : "#a1a1aa",
      accent: "#14b8a6",
      blue: "#3b82f6",
      amber: "#f59e0b",
      danger: "#ef4444",
      input: isDark ? "rgba(2,6,23,0.34)" : "rgba(255,255,255,0.9)",
    }),
    [isDark],
  );

  const sections: Array<{
    id: SyncSection;
    label: string;
    description: string;
    icon: React.ReactNode;
    badge?: string;
  }> = [
    {
      id: "setup",
      label: t("cloudSync.sections.setup"),
      description: t("cloudSync.sections.setupDesc"),
      icon: <Server size={16} />,
    },
    {
      id: "scope",
      label: t("cloudSync.sections.scope"),
      description: t("cloudSync.sections.scopeDesc"),
      icon: <SlidersHorizontal size={16} />,
    },
    {
      id: "security",
      label: t("cloudSync.sections.security"),
      description: t("cloudSync.sections.securityDesc"),
      icon: <ShieldCheck size={16} />,
    },
    {
      id: "status",
      label: t("cloudSync.sections.status"),
      description: t("cloudSync.sections.statusDesc"),
      icon: <FolderSync size={16} />,
    },
    {
      id: "conflicts",
      label: t("cloudSync.sections.conflicts"),
      description: t("cloudSync.sections.conflictsDesc"),
      icon: <FileWarning size={16} />,
      badge: String(conflicts.length),
    },
    {
      id: "logs",
      label: t("cloudSync.sections.logs"),
      description: t("cloudSync.sections.logsDesc"),
      icon: <TerminalSquare size={16} />,
    },
  ];

  const updateForm = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }));
  };

  const updateProvider = (provider: Provider) => {
    if (provider === form.provider) {
      return;
    }

    const defaults = PROVIDER_DEFAULTS[provider];
    setForm((prev) => ({
      ...prev,
      provider,
      profileName:
        !prev.profileName.trim() || prev.profileName === PROVIDER_DEFAULTS[prev.provider].profileName
          ? defaults.profileName
          : prev.profileName,
      serverUrl: defaults.serverUrl,
      remoteRoot: defaults.remoteRoot,
      authMethod: defaults.authMethod,
      username: CLOUD_TOKEN_PROVIDERS.has(provider) ? "" : prev.username,
      password: "",
      privateKey: "",
      passphrase: "",
      sftpPort: provider === "sftp" ? prev.sftpPort || 22 : 22,
    }));
    setSavedCredentialRefs({});
  };

  const refreshRuntimeState = async (profileId: string) => {
    const [status, report, nextConflicts, nextLogs] = await Promise.all([
      sync.getStatus(profileId),
      sync.getLastReport(profileId),
      sync.getConflicts(profileId),
      sync.getLogEntries(profileId, 50),
    ]);
    setSyncStatus(status);
    setLastReport(report);
    setConflicts(nextConflicts);
    setLogEntries(nextLogs);
  };

  useEffect(() => {
    let cancelled = false;

    const loadSyncData = async () => {
      try {
        const [profiles, tabs, plugins] = await Promise.all([
          sync.profileList(),
          sync.getTabOptions(),
          sync.getPluginOptions(),
        ]);
        if (cancelled) return;

        const profileId = profiles[0]?.id ?? null;
        setActiveProfileId(profileId);
        setTabOptions(tabs);
        setPluginOptions(plugins);

        if (profileId) {
          const profile = await sync.profileGet(profileId);
          if (cancelled) return;
          setForm(formFromProfile(profile));
          setSavedCredentialRefs(profile.credential_refs);
          await refreshRuntimeState(profileId);
        } else {
          setConflicts([]);
          setLogEntries([]);
          setSyncStatus(null);
          setLastReport(null);
          setSavedCredentialRefs({});
        }
      } catch (error) {
        if (!cancelled) {
          setSyncMessage(`Cloud Sync data unavailable: ${error}`);
        }
      }
    };

    loadSyncData();
    return () => {
      cancelled = true;
    };
  }, []);

  const toggleTab = (tabId: number) => {
    updateForm(
      "selectedTabs",
      form.selectedTabs.includes(tabId)
        ? form.selectedTabs.filter((id) => id !== tabId)
        : [...form.selectedTabs, tabId],
    );
  };

  const togglePlugin = (pluginId: string) => {
    updateForm(
      "selectedPlugins",
      form.selectedPlugins.includes(pluginId)
        ? form.selectedPlugins.filter((id) => id !== pluginId)
        : [...form.selectedPlugins, pluginId],
    );
  };

  const saveProfile = async () => {
    const profileId = activeProfileId ?? DEFAULT_PROFILE_ID;
    const credentialRefs: SavedCredentialRefs = { ...savedCredentialRefs };
    setSyncMessage(null);
    setPendingSftpHostTrust(null);

    if (!form.profileName.trim()) {
      setSyncMessage("Profile name is required.");
      return null;
    }

    if (form.provider === "webdav" && !form.serverUrl.trim()) {
      setSyncMessage("WebDAV URL is required.");
      return null;
    }

    if (form.provider === "sftp" && !form.serverUrl.trim()) {
      setSyncMessage("SFTP host is required.");
      return null;
    }

    if (!CLOUD_TOKEN_PROVIDERS.has(form.provider) && !credentialRefs.username && !form.username.trim()) {
      setSyncMessage("Username is required.");
      return null;
    }

    if (form.authMethod === "password" && !credentialRefs.password && !form.password) {
      setSyncMessage(CLOUD_TOKEN_PROVIDERS.has(form.provider) ? "Access token is required." : "Password is required.");
      return null;
    }

    if (form.provider === "sftp" && form.authMethod === "privateKey" && !credentialRefs.private_key && !form.privateKey.trim()) {
      setSyncMessage("Private key path or PEM content is required.");
      return null;
    }

    try {
      if (!CLOUD_TOKEN_PROVIDERS.has(form.provider) && form.username.trim()) {
        const usernameRef = await sync.secretSet(profileId, "username", form.username);
        credentialRefs.username = usernameRef.ref_id;
      }
      if (CLOUD_TOKEN_PROVIDERS.has(form.provider)) {
        delete credentialRefs.username;
      }

      if (form.authMethod === "password" || CLOUD_TOKEN_PROVIDERS.has(form.provider)) {
        if (form.password) {
          const passwordRef = await sync.secretSet(profileId, "password", form.password);
          credentialRefs.password = passwordRef.ref_id;
        }
        delete credentialRefs.private_key;
        delete credentialRefs.passphrase;
      } else {
        if (form.privateKey.trim()) {
          const keyRef = await sync.secretSet(profileId, "private_key", form.privateKey);
          credentialRefs.private_key = keyRef.ref_id;
        }

        if (form.passphrase) {
          const passphraseRef = await sync.secretSet(profileId, "passphrase", form.passphrase);
          credentialRefs.passphrase = passphraseRef.ref_id;
        }
        delete credentialRefs.password;
      }

      await sync.profileUpdate({
        id: profileId,
        name: form.profileName.trim(),
        provider: form.provider,
        remote_root:
          form.provider === "webdav"
            ? form.serverUrl.trim()
            : form.provider === "sftp"
              ? buildSftpRemoteRoot(form.serverUrl, form.sftpPort, form.remoteRoot)
              : form.remoteRoot.trim().replace(/^\/+/, "") || PROVIDER_DEFAULTS[form.provider].remoteRoot,
        sync_tabs: {
          mode: form.tabMode,
          selected_tab_ids: form.tabMode === "all" ? [] : form.selectedTabs,
        },
        sync_plugins: {
          mode: "selected",
          selected_plugin_ids:
            form.pluginMode === "all"
              ? pluginOptions.map((plugin) => plugin.id)
              : form.selectedPlugins,
          include_plugin_bundles: false,
          include_granted_permissions: false,
        },
        encryption: {
          enabled: form.encryptionEnabled,
          algorithm: "xchacha20poly1305",
          kdf: "argon2id",
        },
        credential_refs: credentialRefs,
        schedule: {
          manual: true,
          sync_on_startup: form.syncOnStartup,
          startup_delay_seconds: 15,
          sync_on_local_change: form.syncOnLocalChange,
          local_change_debounce_seconds: 30,
          interval_minutes: form.intervalMinutes,
          retry_backoff_seconds: [30, 120, 300, 900],
          pause_on_metered_network: false,
          paused: form.paused,
        },
      });

      setActiveProfileId(profileId);
      setSavedCredentialRefs(credentialRefs);
      setSyncMessage("Sync profile saved.");
      return profileId;
    } catch (error) {
      setSyncMessage(`Save profile failed: ${error}`);
      return null;
    }
  };

  const testConnection = async () => {
    const profileId = await saveProfile();
    if (!profileId) {
      return;
    }
    setTestingConnection(true);
    setSyncMessage(null);
    setPendingSftpHostTrust(null);
    try {
      const result = await sync.testConnection(profileId);
      setSyncMessage(result.message);
      if (result.success) {
        setPendingSftpHostTrust(null);
      } else if (form.provider === "sftp" && isUntrustedSftpHostMessage(result.message)) {
        setPendingSftpHostTrust({
          profileId,
          host: form.serverUrl.trim(),
          port: form.sftpPort,
          message: result.message,
        });
      }
    } catch (error) {
      setSyncMessage(`Connection test failed: ${error}`);
    } finally {
      setTestingConnection(false);
    }
  };

  const trustSftpHostKey = async () => {
    if (!pendingSftpHostTrust) return;

    setTrustingSftpHostKey(true);
    try {
      const trusted = await sync.trustSftpHostKey(pendingSftpHostTrust.profileId);
      setPendingSftpHostTrust(null);
      const result = await sync.testConnection(pendingSftpHostTrust.profileId);
      setSyncMessage(
        result.success
          ? `Trusted ${trusted.host}:${trusted.port} (${trusted.fingerprint_sha256}). ${result.message}`
          : result.message,
      );
    } catch (error) {
      setSyncMessage(`Trust SFTP host key failed: ${error}`);
    } finally {
      setTrustingSftpHostKey(false);
    }
  };

  const runSyncNow = async () => {
    const profileId = await saveProfile();
    if (!profileId) return;

    setSyncing(true);
    try {
      if (form.encryptionEnabled && syncStatus?.is_locked !== false) {
        if (!unlockPassword) {
          setSyncMessage("Unlock password is required before encrypted sync can run.");
          setSyncing(false);
          return;
        }
        await sync.profileUnlock(profileId, unlockPassword, rememberUnlock);
        setUnlockPassword("");
      }
      const report = await sync.runNow(profileId);
      setLastReport(report);
      setSyncMessage(`${report.status}: ${report.items_uploaded} uploaded, ${report.items_downloaded} downloaded`);
      await refreshRuntimeState(profileId);
    } catch (error) {
      setSyncMessage(`Sync failed: ${error}`);
      await refreshRuntimeState(profileId).catch(() => {});
    } finally {
      setSyncing(false);
    }
  };

  const lockProfile = async () => {
    if (!activeProfileId) {
      setSyncMessage("Save a profile before locking sync.");
      return;
    }
    try {
      await sync.profileLock(activeProfileId);
      setUnlockPassword("");
      await refreshRuntimeState(activeProfileId);
      setSyncMessage("Encrypted profile locked.");
    } catch (error) {
      setSyncMessage(`Lock profile failed: ${error}`);
    }
  };

  const unlockProfile = async () => {
    const profileId = await saveProfile();
    if (!profileId) return;
    if (!unlockPassword) {
      setSyncMessage("Unlock password is required.");
      return;
    }
    try {
      await sync.profileUnlock(profileId, unlockPassword, rememberUnlock);
      setUnlockPassword("");
      await refreshRuntimeState(profileId);
      setSyncMessage("Encrypted profile unlocked.");
    } catch (error) {
      setSyncMessage(`Unlock profile failed: ${error}`);
    }
  };

  const resetDraft = async () => {
    if (!activeProfileId) {
      setForm(DEFAULT_FORM);
      setSyncMessage("Draft reset to defaults.");
      return;
    }

    try {
      const profile = await sync.profileGet(activeProfileId);
      setForm(formFromProfile(profile));
      setSyncMessage("Draft reset to the saved profile.");
    } catch (error) {
      setSyncMessage(`Reset draft failed: ${error}`);
    }
  };

  const resolveConflict = async (
    conflictId: number,
    resolution: ConflictResolutionInput,
  ) => {
    if (!activeProfileId) {
      setSyncMessage("Create a sync profile before resolving conflicts.");
      return;
    }

    try {
      await sync.resolveConflict(activeProfileId, conflictId, resolution);
      const nextConflicts = await sync.getConflicts(activeProfileId);
      setConflicts(nextConflicts);
      setSyncMessage("Conflict resolved.");
    } catch (error) {
      setSyncMessage(`Resolve conflict failed: ${error}`);
    }
  };

  const renderSyncMessage = () => {
    if (!syncMessage) return null;

    const tone = messageTone(syncMessage);
    return (
      <div
        className="rounded-lg px-3 py-2 text-xs leading-5"
        style={{
          backgroundColor:
            tone === "error"
              ? isDark
                ? "rgba(239,68,68,0.14)"
                : "rgba(239,68,68,0.08)"
              : palette.panelStrong,
          border: `1px solid ${
            tone === "error" ? "rgba(239,68,68,0.32)" : palette.borderStrong
          }`,
          color: tone === "error" ? palette.danger : palette.text,
        }}
      >
        {syncMessage}
      </div>
    );
  };

  const renderSftpHostTrustDialog = () => {
    if (!pendingSftpHostTrust) return null;

    return (
      <div className="fixed inset-0 z-[70] flex items-center justify-center px-4">
        <div
          className="absolute inset-0"
          style={{ backgroundColor: isDark ? "rgba(2,6,23,0.62)" : "rgba(15,23,42,0.22)" }}
          onClick={() => {
            if (!trustingSftpHostKey) {
              setPendingSftpHostTrust(null);
            }
          }}
        />
        <div
          className="relative w-full max-w-md rounded-lg p-5 shadow-2xl"
          style={{
            backgroundColor: isDark ? "#0f172a" : "#ffffff",
            border: `1px solid ${palette.border}`,
            color: palette.text,
          }}
        >
          <div className="flex items-start gap-3">
            <span
              className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-lg"
              style={{
                backgroundColor: isDark ? "rgba(245,158,11,0.16)" : "rgba(245,158,11,0.12)",
                color: palette.amber,
              }}
            >
              <AlertTriangle size={18} />
            </span>
            <div className="min-w-0">
              <h4 className="text-sm font-semibold">Trust SFTP host key?</h4>
              <p className="mt-2 text-xs leading-5" style={{ color: palette.muted }}>
                {pendingSftpHostTrust.host}:{pendingSftpHostTrust.port} is not in known_hosts.
                Trust this server key and retry the connection test.
              </p>
              <p className="mt-2 break-words rounded-lg px-3 py-2 text-[11px] leading-5" style={{ backgroundColor: palette.panelSoft, color: palette.faint }}>
                {pendingSftpHostTrust.message}
              </p>
            </div>
          </div>
          <div className="mt-5 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setPendingSftpHostTrust(null)}
              disabled={trustingSftpHostKey}
              className="h-9 rounded-lg px-3 text-sm transition-all disabled:opacity-60"
              style={{
                backgroundColor: palette.panelSoft,
                border: `1px solid ${palette.border}`,
                color: palette.text,
              }}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={trustSftpHostKey}
              disabled={trustingSftpHostKey}
              className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm transition-all disabled:opacity-70"
              style={{ backgroundColor: palette.accent, color: "#ffffff" }}
            >
              {trustingSftpHostKey ? <Loader2 size={15} className="animate-spin" /> : <Check size={15} />}
              Trust and Retry
            </button>
          </div>
        </div>
      </div>
    );
  };

  const inputStyle: React.CSSProperties = {
    backgroundColor: palette.input,
    border: `1px solid ${palette.border}`,
    color: palette.text,
  };

  const renderToggle = (
    checked: boolean,
    onChange: () => void,
    label: string,
    description: string,
  ) => (
    <div
      className="flex items-center justify-between gap-4 rounded-lg p-4"
      style={{
        backgroundColor: palette.panelSoft,
        border: `1px solid ${palette.border}`,
      }}
    >
      <div className="min-w-0">
        <p className="text-sm font-medium" style={{ color: palette.text }}>
          {label}
        </p>
        <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
          {description}
        </p>
      </div>
      <button
        type="button"
        onClick={onChange}
        className="h-6 w-11 flex-shrink-0 rounded-full transition-all"
        style={{
          backgroundColor: checked ? palette.accent : isDark ? "#475569" : "#d4d4d8",
        }}
        aria-pressed={checked}
        aria-label={label}
      >
        <span
          className="block h-4 w-4 rounded-full bg-white shadow transition-transform"
          style={{ transform: checked ? "translateX(22px)" : "translateX(4px)" }}
        />
      </button>
    </div>
  );

  const renderSegmented = <T extends string>(
    value: T,
    options: Array<{ value: T; label: string; icon?: React.ReactNode }>,
    onChange: (next: T) => void,
  ) => (
    <div
      className="grid gap-2 rounded-lg p-1"
      style={{
        gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))`,
        backgroundColor: palette.panelSoft,
        border: `1px solid ${palette.border}`,
      }}
    >
      {options.map((option) => {
        const selected = value === option.value;
        return (
          <button
            key={option.value}
            type="button"
            onClick={() => onChange(option.value)}
            className="flex min-h-10 items-center justify-center gap-2 rounded-md px-3 text-sm transition-all"
            style={{
              backgroundColor: selected ? palette.panelStrong : "transparent",
              color: selected ? palette.accent : palette.muted,
              border: `1px solid ${selected ? palette.borderStrong : "transparent"}`,
            }}
          >
            {option.icon}
            <span className="truncate">{option.label}</span>
          </button>
        );
      })}
    </div>
  );

  const renderSetup = () => (
    <div className="space-y-5">
      <div>
        <h4 className="text-sm font-semibold" style={{ color: palette.text }}>
          {t("cloudSync.setup.title")}
        </h4>
        <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
          {t("cloudSync.setup.description")}
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <label className="space-y-2">
          <span className="text-xs font-medium" style={{ color: palette.muted }}>
            {t("cloudSync.setup.profileName")}
          </span>
          <input
            value={form.profileName}
            onChange={(event) => updateForm("profileName", event.target.value)}
            className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
            style={inputStyle}
          />
        </label>

        <div className="space-y-2">
          <span className="text-xs font-medium" style={{ color: palette.muted }}>
            {t("cloudSync.setup.provider")}
          </span>
          <div className="grid grid-cols-2 gap-2">
            {([
              { value: "webdav", icon: <Server size={15} />, desc: t("cloudSync.setup.webdavHint") },
              { value: "sftp", icon: <Plug size={15} />, desc: t("cloudSync.setup.sftpHint") },
              { value: "google_drive", icon: <Cloud size={15} />, desc: t("cloudSync.setup.googleDriveHint") },
              { value: "one_drive", icon: <Database size={15} />, desc: t("cloudSync.setup.oneDriveHint") },
            ] as Array<{ value: Provider; icon: React.ReactNode; desc: string }>).map((option) => {
              const selected = form.provider === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => updateProvider(option.value)}
                  className="min-h-16 rounded-lg px-3 py-2 text-left transition-all"
                  style={{
                    backgroundColor: selected ? palette.panelStrong : palette.panelSoft,
                    border: `1px solid ${selected ? palette.borderStrong : palette.border}`,
                    color: selected ? palette.accent : palette.text,
                  }}
                >
                  <span className="flex items-center gap-2 text-sm font-medium">
                    {option.icon}
                    {PROVIDER_LABELS[option.value]}
                  </span>
                  <span className="mt-1 block text-xs leading-4" style={{ color: palette.muted }}>
                    {option.desc}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {(form.provider === "webdav" || form.provider === "sftp") && (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
          <label className="space-y-2 xl:col-span-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              {form.provider === "webdav"
                ? t("cloudSync.setup.serverUrl")
                : t("cloudSync.setup.host")}
            </span>
            <input
              value={form.serverUrl}
              onChange={(event) => updateForm("serverUrl", event.target.value)}
              placeholder={
                form.provider === "webdav"
                  ? "https://dav.example.com/remote.php/dav/files/me"
                  : "sftp.example.com"
              }
              className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>

          {form.provider === "sftp" ? (
          <label className="space-y-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              {t("cloudSync.setup.port")}
            </span>
            <input
              type="number"
              min={1}
              max={65535}
              value={form.sftpPort}
              onChange={(event) =>
                updateForm("sftpPort", Number(event.target.value) || 22)
              }
              className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>
          ) : (
          <label className="space-y-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              {t("cloudSync.setup.username")}
            </span>
            <input
              value={form.username}
              onChange={(event) => updateForm("username", event.target.value)}
              className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>
          )}
        </div>
      )}

      {form.provider === "sftp" && (
        <label className="block space-y-2">
          <span className="text-xs font-medium" style={{ color: palette.muted }}>
            {t("cloudSync.setup.username")}
          </span>
          <input
            value={form.username}
            onChange={(event) => updateForm("username", event.target.value)}
            className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
            style={inputStyle}
          />
        </label>
      )}

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {form.provider !== "webdav" && (
          <label className="space-y-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              {CLOUD_TOKEN_PROVIDERS.has(form.provider)
                ? t("cloudSync.setup.appFolder")
                : t("cloudSync.setup.remoteRoot")}
            </span>
            <input
              value={form.remoteRoot}
              onChange={(event) => updateForm("remoteRoot", event.target.value)}
              placeholder={PROVIDER_DEFAULTS[form.provider].remoteRoot}
              className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>
        )}

        {form.provider === "sftp" ? (
          <div className="space-y-2">
          <span className="text-xs font-medium" style={{ color: palette.muted }}>
            {t("cloudSync.setup.authMethod")}
          </span>
          {renderSegmented<FormState["authMethod"]>(
            form.authMethod,
            [
              {
                value: "password",
                label: t("cloudSync.setup.password"),
                icon: <KeyRound size={15} />,
              },
              {
                value: "privateKey",
                label: t("cloudSync.setup.privateKey"),
                icon: <Lock size={15} />,
              },
            ],
            (next) => updateForm("authMethod", next),
          )}
          </div>
        ) : (
          <div
            className="rounded-lg p-3 text-xs leading-5"
            style={{ backgroundColor: palette.panelSoft, border: `1px solid ${palette.border}`, color: palette.muted }}
          >
            {form.provider === "webdav"
              ? t("cloudSync.setup.webdavAuthNote")
              : t("cloudSync.setup.oauthTokenNote")}
          </div>
        )}
      </div>

      {form.authMethod === "password" || CLOUD_TOKEN_PROVIDERS.has(form.provider) ? (
        <label className="block space-y-2">
          <span className="text-xs font-medium" style={{ color: palette.muted }}>
            {CLOUD_TOKEN_PROVIDERS.has(form.provider)
              ? t("cloudSync.setup.accessToken")
              : t("cloudSync.setup.password")}
          </span>
          <input
            type="password"
            value={form.password}
            onChange={(event) => updateForm("password", event.target.value)}
            placeholder={
              CLOUD_TOKEN_PROVIDERS.has(form.provider)
                ? t("cloudSync.setup.accessTokenPlaceholder")
                : undefined
            }
            className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
            style={inputStyle}
          />
        </label>
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <label className="space-y-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              {t("cloudSync.setup.privateKey")}
            </span>
            <textarea
              value={form.privateKey}
              onChange={(event) => updateForm("privateKey", event.target.value)}
              className="min-h-24 w-full resize-y rounded-lg px-3 py-2 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>
          <label className="space-y-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              {t("cloudSync.setup.passphrase")}
            </span>
            <input
              type="password"
              value={form.passphrase}
              onChange={(event) => updateForm("passphrase", event.target.value)}
              className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>
        </div>
      )}

      <div
        className="flex flex-wrap items-center justify-between gap-3 rounded-lg p-4"
        style={{
          backgroundColor: palette.panelStrong,
          border: `1px solid ${palette.borderStrong}`,
        }}
      >
        <div>
          <p className="text-sm font-medium" style={{ color: palette.text }}>
            {t("cloudSync.setup.connection")}
          </p>
          <p className="mt-1 text-xs" style={{ color: palette.muted }}>
            {t("cloudSync.setup.connectionDesc")}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={saveProfile}
            className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm transition-all"
            style={{
              backgroundColor: palette.panelSoft,
              border: `1px solid ${palette.border}`,
              color: palette.text,
            }}
          >
            <Check size={15} />
            {t("common.save")}
          </button>
          <button
            type="button"
            onClick={testConnection}
            className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm transition-all"
            style={{
              backgroundColor: palette.accent,
              color: "#ffffff",
            }}
          >
            {testingConnection ? <Loader2 size={15} className="animate-spin" /> : <RefreshCw size={15} />}
            {testingConnection
              ? t("cloudSync.actions.testing")
              : t("cloudSync.actions.test")}
          </button>
        </div>
      </div>
    </div>
  );

  const renderScope = () => (
    <div className="space-y-6">
      <div>
        <h4 className="text-sm font-semibold" style={{ color: palette.text }}>
          {t("cloudSync.scope.title")}
        </h4>
        <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
          {t("cloudSync.scope.description")}
        </p>
      </div>

      <section className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium" style={{ color: palette.text }}>
              {t("cloudSync.scope.tabs")}
            </p>
            <p className="mt-1 text-xs" style={{ color: palette.muted }}>
              {t("cloudSync.scope.tabsDesc")}
            </p>
          </div>
          <div className="w-48">
            {renderSegmented<SyncMode>(
              form.tabMode,
              [
                { value: "all", label: t("cloudSync.scope.all") },
                { value: "selected", label: t("cloudSync.scope.selected") },
              ],
              (next) => updateForm("tabMode", next),
            )}
          </div>
        </div>

        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {tabOptions.map((tab) => {
            const checked = form.tabMode === "all" || form.selectedTabs.includes(tab.id);
            return (
              <button
                key={tab.id}
                type="button"
                disabled={form.tabMode === "all"}
                onClick={() => toggleTab(tab.id)}
                className="flex min-h-14 items-center justify-between rounded-lg px-3 text-left transition-all disabled:cursor-default"
                style={{
                  backgroundColor: checked ? palette.panelStrong : palette.panelSoft,
                  border: `1px solid ${checked ? palette.borderStrong : palette.border}`,
                  color: palette.text,
                }}
              >
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium">{tab.name}</span>
                  <span className="text-xs" style={{ color: palette.muted }}>
                    #{tab.id}
                  </span>
                </span>
                {checked && <Check size={16} style={{ color: palette.accent }} />}
              </button>
            );
          })}
        </div>
      </section>

      <section className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium" style={{ color: palette.text }}>
              {t("cloudSync.scope.plugins")}
            </p>
            <p className="mt-1 text-xs" style={{ color: palette.muted }}>
              {t("cloudSync.scope.pluginsDesc")}
            </p>
          </div>
          <div className="w-48">
            {renderSegmented<SyncMode>(
              form.pluginMode,
              [
                { value: "all", label: t("cloudSync.scope.all") },
                { value: "selected", label: t("cloudSync.scope.selected") },
              ],
              (next) => updateForm("pluginMode", next),
            )}
          </div>
        </div>

        <div className="space-y-2">
          {pluginOptions.map((plugin) => {
            const checked =
              form.pluginMode === "all" || form.selectedPlugins.includes(plugin.id);
            return (
              <button
                key={plugin.id}
                type="button"
                disabled={form.pluginMode === "all"}
                onClick={() => togglePlugin(plugin.id)}
                className="flex min-h-12 w-full items-center justify-between rounded-lg px-3 text-left transition-all disabled:cursor-default"
                style={{
                  backgroundColor: checked ? palette.panelStrong : palette.panelSoft,
                  border: `1px solid ${checked ? palette.borderStrong : palette.border}`,
                  color: palette.text,
                }}
              >
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium">
                    {plugin.name}
                  </span>
                  <span className="block truncate text-xs" style={{ color: palette.muted }}>
                    {plugin.id} · {plugin.is_active ? "active" : "inactive"}
                  </span>
                </span>
                {checked && <Check size={16} style={{ color: palette.accent }} />}
              </button>
            );
          })}
        </div>
      </section>
    </div>
  );

  const renderSecurity = () => (
    <div className="space-y-5">
      <div>
        <h4 className="text-sm font-semibold" style={{ color: palette.text }}>
          {t("cloudSync.security.title")}
        </h4>
        <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
          {t("cloudSync.security.description")}
        </p>
      </div>

      {renderToggle(
        form.encryptionEnabled,
        () => updateForm("encryptionEnabled", !form.encryptionEnabled),
        t("cloudSync.security.encryption"),
        t("cloudSync.security.encryptionDesc"),
      )}

      {renderToggle(
        form.uploadSensitiveItems,
        () => updateForm("uploadSensitiveItems", !form.uploadSensitiveItems),
        t("cloudSync.security.sensitive"),
        t("cloudSync.security.sensitiveDesc"),
      )}

      {form.encryptionEnabled && (
        <div
          className="grid grid-cols-1 gap-3 rounded-lg p-4 xl:grid-cols-[1fr_auto]"
          style={{
            backgroundColor: palette.panelSoft,
            border: `1px solid ${palette.border}`,
          }}
        >
          <label className="space-y-2">
            <span className="text-xs font-medium" style={{ color: palette.muted }}>
              Unlock password
            </span>
            <input
              type="password"
              value={unlockPassword}
              onChange={(event) => setUnlockPassword(event.target.value)}
              className="h-10 w-full rounded-lg px-3 text-sm outline-none transition-all"
              style={inputStyle}
            />
          </label>
          <div className="flex flex-wrap items-end gap-2">
            <button
              type="button"
              onClick={() => setRememberUnlock((value) => !value)}
              className="flex h-10 items-center gap-2 rounded-lg px-3 text-sm"
              style={{
                backgroundColor: rememberUnlock ? palette.panelStrong : palette.panelSoft,
                border: `1px solid ${rememberUnlock ? palette.borderStrong : palette.border}`,
                color: rememberUnlock ? palette.accent : palette.text,
              }}
            >
              <KeyRound size={15} />
              Remember
            </button>
            <button
              type="button"
              onClick={unlockProfile}
              className="flex h-10 items-center gap-2 rounded-lg px-3 text-sm"
              style={{ backgroundColor: palette.accent, color: "#ffffff" }}
            >
              <Lock size={15} />
              Unlock
            </button>
            <button
              type="button"
              onClick={lockProfile}
              className="flex h-10 items-center gap-2 rounded-lg px-3 text-sm"
              style={{
                backgroundColor: palette.panelSoft,
                border: `1px solid ${palette.border}`,
                color: palette.text,
              }}
            >
              <Lock size={15} />
              Lock
            </button>
          </div>
        </div>
      )}

      <div
        className="rounded-lg p-4"
        style={{
          backgroundColor: form.encryptionEnabled
            ? palette.panelStrong
            : isDark
              ? "rgba(245,158,11,0.12)"
              : "rgba(245,158,11,0.1)",
          border: `1px solid ${
            form.encryptionEnabled ? palette.borderStrong : "rgba(245,158,11,0.35)"
          }`,
        }}
      >
        <div className="flex gap-3">
          {form.encryptionEnabled ? (
            <ShieldCheck size={18} style={{ color: palette.accent }} />
          ) : (
            <AlertTriangle size={18} style={{ color: palette.amber }} />
          )}
          <div>
            <p className="text-sm font-medium" style={{ color: palette.text }}>
              {form.encryptionEnabled
                ? t("cloudSync.security.encryptedState")
                : t("cloudSync.security.plainState")}
            </p>
            <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
              {form.encryptionEnabled
                ? t("cloudSync.security.encryptedStateDesc")
                : t("cloudSync.security.plainStateDesc")}
            </p>
          </div>
        </div>
      </div>
    </div>
  );

  const renderStatus = () => (
    <div className="space-y-5">
      <div className="grid grid-cols-1 gap-3 xl:grid-cols-3">
        {[
          {
            label: t("cloudSync.status.lastSuccess"),
            value: formatSyncDate(syncStatus?.last_sync_at ?? lastReport?.completed_at),
            icon: <Check size={16} />,
            color: palette.accent,
          },
          {
            label: "Status",
            value: form.paused || syncStatus?.is_paused
              ? t("cloudSync.status.paused")
              : syncStatus?.phase ?? syncStatus?.status ?? lastReport?.status ?? "idle",
            icon: <Clock3 size={16} />,
            color: form.paused || syncStatus?.is_locked ? palette.amber : palette.blue,
          },
          {
            label: "Last report",
            value: lastReport
              ? `${lastReport.items_uploaded} up · ${lastReport.items_downloaded} down · ${lastReport.conflicts_found} conflicts`
              : "No runs yet",
            icon: <Database size={16} />,
            color: palette.blue,
          },
        ].map((item) => (
          <div
            key={item.label}
            className="rounded-lg p-4"
            style={{
              backgroundColor: palette.panelSoft,
              border: `1px solid ${palette.border}`,
            }}
          >
            <div className="flex items-center gap-2" style={{ color: item.color }}>
              {item.icon}
              <span className="text-xs font-medium">{item.label}</span>
            </div>
            <p className="mt-3 text-lg font-semibold" style={{ color: palette.text }}>
              {item.value}
            </p>
          </div>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
        {renderToggle(
          form.syncOnStartup,
          () => updateForm("syncOnStartup", !form.syncOnStartup),
          t("cloudSync.status.startup"),
          t("cloudSync.status.startupDesc"),
        )}
        {renderToggle(
          form.syncOnLocalChange,
          () => updateForm("syncOnLocalChange", !form.syncOnLocalChange),
          t("cloudSync.status.localChange"),
          t("cloudSync.status.localChangeDesc"),
        )}
      </div>

      <label className="block space-y-2">
        <span className="text-xs font-medium" style={{ color: palette.muted }}>
          {t("cloudSync.status.interval")}
        </span>
        <input
          type="range"
          min={5}
          max={120}
          step={5}
          value={form.intervalMinutes}
          onChange={(event) =>
            updateForm("intervalMinutes", Number(event.target.value))
          }
          className="w-full"
        />
        <span className="text-xs" style={{ color: palette.faint }}>
          {t("cloudSync.status.intervalValue", { count: form.intervalMinutes })}
        </span>
      </label>

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={runSyncNow}
          className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm"
          style={{ backgroundColor: palette.accent, color: "#ffffff" }}
        >
          {syncing ? <Loader2 size={15} className="animate-spin" /> : <FolderSync size={15} />}
          {syncing ? t("cloudSync.actions.syncing") : t("cloudSync.actions.syncNow")}
        </button>
        <button
          type="button"
          onClick={async () => {
            const profileId = activeProfileId ?? (await saveProfile());
            if (!profileId) return;
            const nextPaused = !form.paused;
            try {
              if (nextPaused) {
                await sync.profilePause(profileId);
              } else {
                await sync.profileResume(profileId);
              }
              updateForm("paused", nextPaused);
              await refreshRuntimeState(profileId);
            } catch (error) {
              setSyncMessage(`Update pause state failed: ${error}`);
            }
          }}
          className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm"
          style={{
            backgroundColor: palette.panelSoft,
            border: `1px solid ${palette.border}`,
            color: palette.text,
          }}
        >
          {form.paused ? <Play size={15} /> : <Pause size={15} />}
          {form.paused ? t("cloudSync.actions.resume") : t("cloudSync.actions.pause")}
        </button>
      </div>
    </div>
  );

  const renderConflicts = () => (
    <div className="space-y-4">
      <div>
        <h4 className="text-sm font-semibold" style={{ color: palette.text }}>
          {t("cloudSync.conflicts.title")}
        </h4>
        <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
          {t("cloudSync.conflicts.description")}
        </p>
      </div>

      {conflicts.length === 0 && (
        <div
          className="rounded-lg p-4 text-sm"
          style={{
            backgroundColor: palette.panelSoft,
            border: `1px solid ${palette.border}`,
            color: palette.muted,
          }}
        >
          No pending conflicts
        </div>
      )}

      {conflicts.map((conflict) => (
        <div
          key={conflict.id}
          className="rounded-lg p-4"
          style={{
            backgroundColor: palette.panelSoft,
            border: `1px solid ${palette.border}`,
          }}
        >
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="truncate text-sm font-medium" style={{ color: palette.text }}>
                {conflict.entity_key}
              </p>
              <p className="mt-1 text-xs" style={{ color: palette.muted }}>
                {conflict.reason} · {formatSyncDate(conflict.created_at)}
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              {[
                { key: "keepLocal", label: t("cloudSync.conflicts.keepLocal") },
                { key: "keepRemote", label: t("cloudSync.conflicts.keepRemote") },
                { key: "keepBoth", label: t("cloudSync.conflicts.keepBoth") },
              ].map((action) => (
                <button
                  key={action.key}
                  type="button"
                  onClick={() =>
                    resolveConflict(conflict.id, resolutionByLabelKey[action.key])
                  }
                  className="h-8 rounded-lg px-3 text-xs"
                  style={{
                    backgroundColor: palette.panel,
                    border: `1px solid ${palette.border}`,
                    color: palette.text,
                  }}
                >
                  {action.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      ))}
    </div>
  );

  const renderLogs = () => (
    <div className="space-y-4">
      <div>
        <h4 className="text-sm font-semibold" style={{ color: palette.text }}>
          {t("cloudSync.logs.title")}
        </h4>
        <p className="mt-1 text-xs leading-5" style={{ color: palette.muted }}>
          {t("cloudSync.logs.description")}
        </p>
      </div>

      <div
        className="overflow-hidden rounded-lg"
        style={{
          backgroundColor: isDark ? "rgba(2,6,23,0.5)" : "rgba(244,244,245,0.9)",
          border: `1px solid ${palette.border}`,
        }}
      >
        {logEntries.length === 0 && (
          <div className="px-3 py-3 text-xs" style={{ color: palette.muted }}>
            No sync logs yet
          </div>
        )}
        {logEntries.map((entry, index) => (
          <div
            key={`${entry.timestamp}-${entry.message}`}
            className="grid grid-cols-[70px_56px_minmax(0,1fr)] gap-3 px-3 py-2 text-xs"
            style={{
              borderTop: index === 0 ? "none" : `1px solid ${palette.border}`,
              color: palette.muted,
            }}
          >
            <span>{new Date(entry.timestamp).toLocaleTimeString()}</span>
            <span
              style={{
                color: entry.level.toUpperCase() === "WARN" ? palette.amber : palette.accent,
              }}
            >
              {entry.level.toUpperCase()}
            </span>
            <span className="truncate font-mono">{entry.message}</span>
          </div>
        ))}
      </div>
    </div>
  );

  const renderActiveSection = () => {
    switch (activeSection) {
      case "setup":
        return renderSetup();
      case "scope":
        return renderScope();
      case "security":
        return renderSecurity();
      case "status":
        return renderStatus();
      case "conflicts":
        return renderConflicts();
      case "logs":
        return renderLogs();
      default:
        return null;
    }
  };

  const activeSectionMeta = sections.find((section) => section.id === activeSection);

  if (activeSection) {
    return (
      <>
        <div className="flex min-h-0 h-full flex-col gap-4">
          <div
            className="rounded-lg p-4"
            style={{
              backgroundColor: palette.panel,
              border: `1px solid ${palette.border}`,
            }}
          >
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-3">
                <button
                  type="button"
                  onClick={() => setActiveSection(null)}
                  className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-lg transition-all"
                  style={{
                    backgroundColor: palette.panelSoft,
                    border: `1px solid ${palette.border}`,
                    color: palette.text,
                  }}
                  aria-label={t("common.back")}
                  title={t("common.back")}
                >
                  <ArrowLeft size={16} />
                </button>
                <div
                  className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg"
                  style={{
                    backgroundColor: palette.panelStrong,
                    color: palette.accent,
                  }}
                >
                  {activeSectionMeta?.icon}
                </div>
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold" style={{ color: palette.text }}>
                    {activeSectionMeta?.label}
                  </p>
                  <p className="mt-1 truncate text-xs" style={{ color: palette.muted }}>
                    {activeSectionMeta?.description}
                  </p>
                </div>
              </div>
              <button
                type="button"
                onClick={resetDraft}
                className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm"
                style={{
                  backgroundColor: palette.panelSoft,
                  border: `1px solid ${palette.border}`,
                  color: palette.text,
                }}
              >
                <RotateCcw size={15} />
                {t("cloudSync.actions.resetDraft")}
              </button>
            </div>
          </div>

          {renderSyncMessage()}

          <div
            className="min-h-0 flex-1 overflow-y-auto rounded-lg p-5 settings-scroll-area"
            style={{
              backgroundColor: palette.panel,
              border: `1px solid ${palette.border}`,
            }}
          >
            {renderActiveSection()}
          </div>
        </div>
        {renderSftpHostTrustDialog()}
      </>
    );
  }

  return (
    <>
    <div className="flex min-h-0 h-full flex-col gap-4">
      <div
        className="rounded-lg p-4"
        style={{
          backgroundColor: palette.panel,
          border: `1px solid ${palette.border}`,
        }}
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <div
              className="flex h-10 w-10 items-center justify-center rounded-lg"
              style={{
                backgroundColor: palette.panelStrong,
                color: palette.accent,
              }}
            >
              <FolderSync size={20} />
            </div>
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold" style={{ color: palette.text }}>
                {form.profileName}
              </p>
              <p className="mt-1 truncate text-xs" style={{ color: palette.muted }}>
                {t("cloudSync.header.subtitle")}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <span
              className="rounded-full px-2.5 py-1 text-xs"
              style={{
                backgroundColor: form.paused
                  ? isDark
                    ? "rgba(245,158,11,0.14)"
                    : "rgba(245,158,11,0.1)"
                  : palette.panelStrong,
                color: form.paused ? palette.amber : palette.accent,
                border: `1px solid ${form.paused ? "rgba(245,158,11,0.32)" : palette.borderStrong}`,
              }}
            >
              {form.paused ? t("cloudSync.header.paused") : t("cloudSync.header.ready")}
            </span>
            <button
              type="button"
              onClick={resetDraft}
              className="flex h-9 items-center gap-2 rounded-lg px-3 text-sm"
              style={{
                backgroundColor: palette.panelSoft,
                border: `1px solid ${palette.border}`,
                color: palette.text,
              }}
            >
              <RotateCcw size={15} />
              {t("cloudSync.actions.resetDraft")}
            </button>
          </div>
        </div>
      </div>

      {renderSyncMessage()}

      <div className="min-h-0 flex-1 overflow-y-auto pr-1 settings-scroll-area">
        <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
          {sections.map((section) => {
            return (
              <button
                key={section.id}
                type="button"
                onClick={() => setActiveSection(section.id)}
                className="flex w-full items-center gap-3 rounded-lg p-4 text-left transition-all"
                style={{
                  backgroundColor: palette.panelSoft,
                  border: `1px solid ${palette.border}`,
                  color: palette.text,
                }}
              >
                <span
                  className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-lg"
                  style={{
                    backgroundColor: palette.panelStrong,
                    color: palette.accent,
                  }}
                >
                  {section.icon}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-2 text-sm font-medium">
                    <span className="truncate">{section.label}</span>
                    {section.badge && (
                      <span
                        className="rounded-full px-1.5 py-0.5 text-[10px]"
                        style={{
                          backgroundColor: isDark
                            ? "rgba(239,68,68,0.16)"
                            : "rgba(239,68,68,0.1)",
                          color: palette.danger,
                        }}
                      >
                        {section.badge}
                      </span>
                    )}
                  </span>
                  <span className="mt-1 block truncate text-xs" style={{ color: palette.muted }}>
                    {section.description}
                  </span>
                </span>
                <ChevronRight size={15} style={{ color: palette.faint }} />
              </button>
            );
          })}
        </div>
      </div>
    </div>
    {renderSftpHostTrustDialog()}
    </>
  );
};

export default CloudSyncTab;
