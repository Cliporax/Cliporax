import type { ConflictResolutionInput, SyncProfile } from "../../lib/tauri-api";

export type Provider = "webdav" | "sftp" | "google_drive" | "one_drive";
export type SyncSection = "setup" | "scope" | "security" | "status" | "conflicts" | "logs";
export type SyncMode = "all" | "selected";
export type SavedCredentialRefs = Partial<Record<"username" | "password" | "private_key" | "passphrase", string>>;

export interface FormState {
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
  selectedTabs: number[];
  selectedPlugins: string[];
  encryptionEnabled: boolean;
  uploadSensitiveItems: boolean;
  syncOnStartup: boolean;
  syncOnLocalChange: boolean;
  intervalMinutes: number;
  paused: boolean;
}

export const DEFAULT_FORM: FormState = {
  profileName: "Personal WebDAV",
  provider: "webdav",
  serverUrl: "https://dav.example.com/remote.php/dav/files/me",
  username: "",
  password: "",
  privateKey: "",
  passphrase: "",
  remoteRoot: "/cliporax/v1",
  sftpPort: 22,
  authMethod: "password",
  selectedTabs: [],
  selectedPlugins: [],
  encryptionEnabled: false,
  uploadSensitiveItems: false,
  syncOnStartup: true,
  syncOnLocalChange: true,
  intervalMinutes: 15,
  paused: false,
};

export const PROVIDER_DEFAULTS: Record<
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
    remoteRoot: "/cliporax/v1",
    authMethod: "password",
  },
  sftp: {
    profileName: "Personal SFTP",
    serverUrl: "sftp.example.com",
    remoteRoot: "/cliporax/v1",
    authMethod: "password",
  },
  google_drive: {
    profileName: "Google Drive Sync",
    serverUrl: "",
    remoteRoot: "cliporax/v1",
    authMethod: "password",
  },
  one_drive: {
    profileName: "OneDrive Sync",
    serverUrl: "",
    remoteRoot: "cliporax/v1",
    authMethod: "password",
  },
};

export const PROVIDER_LABELS: Record<Provider, string> = {
  webdav: "WebDAV",
  sftp: "SFTP",
  google_drive: "Google Drive",
  one_drive: "OneDrive",
};

export const CLOUD_TOKEN_PROVIDERS = new Set<Provider>(["google_drive", "one_drive"]);

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

export const formatSyncDate = (value: string | null | undefined) => {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
};

export const DEFAULT_PROFILE_ID = "default-cloud-sync";

const trimSlashes = (value: string) => value.trim().replace(/^\/+|\/+$/g, "");

export const buildWebdavRemoteRoot = (serverUrl: string, remoteRoot: string) => {
  const trimmedServerUrl = serverUrl.trim().replace(/\/+$/g, "");
  const trimmedRemoteRoot = trimSlashes(remoteRoot);

  if (!trimmedRemoteRoot) {
    return trimmedServerUrl;
  }

  const normalizedServerUrl = trimmedServerUrl.toLowerCase();
  const normalizedRemoteRoot = trimSlashes(trimmedRemoteRoot).toLowerCase();
  if (
    normalizedServerUrl.endsWith(`/${normalizedRemoteRoot}`) ||
    normalizedServerUrl.endsWith(`/${normalizedRemoteRoot}/`)
  ) {
    return trimmedServerUrl;
  }

  return `${trimmedServerUrl}/${trimmedRemoteRoot}`;
};

const parseWebdavRemoteRoot = (remoteRoot: string) => {
  const trimmedRemoteRoot = remoteRoot.trim();
  const defaultRemoteRoot = PROVIDER_DEFAULTS.webdav.remoteRoot;
  const normalizedDefaultRemoteRoot = trimSlashes(defaultRemoteRoot);
  const withoutTrailingSlash = trimmedRemoteRoot.replace(/\/+$/g, "");
  const defaultSuffix = `/${normalizedDefaultRemoteRoot}`;
  const normalizedDefaultSuffix = defaultSuffix.toLowerCase();

  if (!trimmedRemoteRoot) {
    return {
      serverUrl: PROVIDER_DEFAULTS.webdav.serverUrl,
      remoteRoot: defaultRemoteRoot,
    };
  }

  let serverUrl = withoutTrailingSlash;
  while (serverUrl.toLowerCase().endsWith(normalizedDefaultSuffix)) {
    serverUrl = serverUrl.slice(0, -defaultSuffix.length).replace(/\/+$/g, "");
  }

  if (serverUrl !== withoutTrailingSlash) {
    return {
      serverUrl,
      remoteRoot: defaultRemoteRoot,
    };
  }

  return {
    serverUrl: trimmedRemoteRoot,
    remoteRoot: "",
  };
};

export const buildSftpRemoteRoot = (host: string, port: number, remoteRoot: string) => {
  const trimmedHost = host.trim().replace(/^sftp:\/\//, "").replace(/\/.*$/, "");
  const normalizedRoot = remoteRoot.trim().startsWith("/")
    ? remoteRoot.trim()
    : `/${remoteRoot.trim() || trimSlashes(PROVIDER_DEFAULTS.sftp.remoteRoot)}`;
  return `sftp://${trimmedHost}:${port}${normalizedRoot}`;
};

const parseSftpRemoteRoot = (remoteRoot: string) => {
  const withoutScheme = remoteRoot.replace(/^sftp:\/\//, "");
  const slashIndex = withoutScheme.indexOf("/");
  const hostPort = slashIndex >= 0 ? withoutScheme.slice(0, slashIndex) : withoutScheme;
  const path = slashIndex >= 0 ? withoutScheme.slice(slashIndex) : PROVIDER_DEFAULTS.sftp.remoteRoot;
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

export const formFromProfile = (profile: SyncProfile): FormState => {
  const provider = normalizeProvider(profile.provider);
  const sftpRemote = provider === "sftp" ? parseSftpRemoteRoot(profile.remote_root) : null;
  const webdavRemote = provider === "webdav" ? parseWebdavRemoteRoot(profile.remote_root) : null;

  return {
    ...DEFAULT_FORM,
    ...PROVIDER_DEFAULTS[provider],
    profileName: profile.name,
    provider,
    serverUrl:
      provider === "sftp"
        ? sftpRemote?.host ?? ""
        : provider === "webdav"
          ? webdavRemote?.serverUrl ?? PROVIDER_DEFAULTS.webdav.serverUrl
          : PROVIDER_DEFAULTS[provider].serverUrl,
    sftpPort: provider === "sftp" ? sftpRemote?.port ?? DEFAULT_FORM.sftpPort : DEFAULT_FORM.sftpPort,
    remoteRoot:
      provider === "sftp"
        ? sftpRemote?.path ?? PROVIDER_DEFAULTS.sftp.remoteRoot
        : provider === "webdav"
          ? webdavRemote?.remoteRoot ?? PROVIDER_DEFAULTS.webdav.remoteRoot
          : profile.remote_root,
    selectedTabs: profile.sync_tabs.mode === "all" ? [] : profile.sync_tabs.selected_tab_ids,
    selectedPlugins: profile.sync_plugins.selected_plugin_ids,
    encryptionEnabled: profile.encryption.enabled,
    syncOnStartup: profile.schedule.sync_on_startup,
    syncOnLocalChange: profile.schedule.sync_on_local_change,
    intervalMinutes: profile.schedule.interval_minutes,
    paused: Boolean(profile.schedule.paused),
  };
};

export const resolutionByLabelKey: Record<string, ConflictResolutionInput> = {
  keepLocal: "use_local",
  keepRemote: "use_remote",
  keepBoth: "keep_both",
};

export const messageTone = (message: string) => {
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

export const isUntrustedSftpHostMessage = (message: string) =>
  message.includes("SFTP host key") && message.includes("not trusted");
