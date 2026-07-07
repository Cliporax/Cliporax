/**
 * Cloud Sync Plugin for Cliporax
 * Provides settings UI for configuring cloud sync profiles,
 * viewing sync status, resolving conflicts, and manual sync controls.
 */

import { invoke } from "@tauri-apps/api/core";

const DEFAULT_REMOTE_ROOT = "cliporax/v1";

// ============================================================================
// Type Definitions
// ============================================================================

interface PluginContext {
  theme?: "dark" | "light" | "system";
  [key: string]: unknown;
}

interface ExtensionProps {
  data?: unknown;
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

// Sync API Types
interface SyncProfileSummary {
  id: string;
  name: string;
  provider: Provider;
  remote_root: string;
  encryption_enabled: boolean;
  last_sync_at?: string;
  status: "idle" | "syncing" | "error" | "success";
}

interface SyncProfile {
  id: string;
  name: string;
  provider: Provider;
  remote_root: string;
  credential_refs: {
    username?: string;
    password?: string;
    private_key?: string;
    passphrase?: string;
  };
  sync_tabs: { mode: "all" | "selected"; selected_tab_ids: number[] };
  sync_plugins: {
    mode: "selected";
    selected_plugin_ids: string[];
    include_plugin_bundles: boolean;
    include_granted_permissions: boolean;
  };
  schedule: {
    manual: boolean;
    sync_on_startup: boolean;
    startup_delay_seconds: number;
    sync_on_local_change: boolean;
    local_change_debounce_seconds: number;
    interval_minutes: number;
    retry_backoff_seconds: number[];
    pause_on_metered_network: boolean;
  };
  encryption: {
    enabled: boolean;
    algorithm: string;
    kdf: string;
  };
}

interface SecretRef {
  ref_id: string;
  profile_id: string;
  key: string;
}

interface SyncStatus {
  profile_id: string;
  status: "idle" | "running" | "success" | "error" | "cancelled";
  progress?: {
    phase: "upload" | "download" | "resolve" | "complete";
    uploaded: number;
    downloaded: number;
    resolved: number;
    total: number;
  };
  last_run_at?: string;
  last_error?: string;
}

interface SyncConflict {
  id: number;
  entity_type: string;
  entity_key: string;
  local_payload: string;
  remote_payload: string;
  reason: string;
  status: "pending" | "resolved";
  created_at: string;
}

interface SyncLogEntry {
  id: number;
  timestamp: string;
  level: "info" | "warn" | "error";
  message: string;
  profile_id?: string;
}

interface ConnectionTestResult {
  success: boolean;
  message: string;
  remote_accessible: boolean;
  remote_writable: boolean;
}

// ============================================================================
// Helper Functions
// ============================================================================

function getThemeColors(theme: string) {
  const isDark = theme === "dark";
  return {
    isDark,
    bg: isDark ? "#1f2937" : "#ffffff",
    bgSecondary: isDark ? "#111827" : "#f9fafb",
    bgTertiary: isDark ? "#374151" : "#f3f4f6",
    text: isDark ? "#f3f4f6" : "#1f2937",
    textSecondary: isDark ? "#9ca3af" : "#6b7280",
    border: isDark ? "#374151" : "#e5e7eb",
    primary: "#3b82f6",
    primaryHover: "#2563eb",
    success: "#10b981",
    warning: "#f59e0b",
    error: "#ef4444",
  };
}

function formatProvider(provider: string): string {
  switch (provider) {
    case "webdav":
      return "WebDAV";
    case "sftp":
      return "SFTP";
    case "google_drive":
      return "Google Drive";
    case "one_drive":
      return "OneDrive";
    default:
      return provider;
  }
}

function formatDate(dateStr?: string): string {
  if (!dateStr) return "Never";
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  } catch {
    return "Invalid date";
  }
}

// ============================================================================
// UI Component Builders
// ============================================================================

function createSection(
  title: string,
  icon: string,
  children: HTMLElement[],
  theme: string,
  collapsible: boolean = true,
): HTMLElement {
  const colors = getThemeColors(theme);
  const section = document.createElement("div");
  section.style.cssText = `
    border-radius: 12px;
    background: ${colors.bgSecondary};
    border: 1px solid ${colors.border};
    overflow: hidden;
    margin-bottom: 16px;
  `;

  const header = document.createElement("div");
  header.style.cssText = `
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px;
    cursor: ${collapsible ? "pointer" : "default"};
  `;

  const titleContainer = document.createElement("div");
  titleContainer.style.cssText = "display: flex; align-items: center; gap: 10px;";

  const iconSpan = document.createElement("span");
  iconSpan.innerHTML = icon;
  iconSpan.style.cssText = `color: ${colors.primary};`;

  const titleEl = document.createElement("h3");
  titleEl.style.cssText = `
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: ${colors.text};
  `;
  titleEl.textContent = title;

  titleContainer.appendChild(iconSpan);
  titleContainer.appendChild(titleEl);

  const chevron = document.createElement("span");
  chevron.style.cssText = `
    color: ${colors.textSecondary};
    transition: transform 0.2s;
  `;
  chevron.innerHTML = collapsible
    ? '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m6 9 6 6 6-6"/></svg>'
    : "";

  header.appendChild(titleContainer);
  if (collapsible) header.appendChild(chevron);

  const content = document.createElement("div");
  content.style.cssText = `
    padding: 0 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  `;

  children.forEach((child) => content.appendChild(child));

  let isCollapsed = false;
  if (collapsible) {
    header.onclick = () => {
      isCollapsed = !isCollapsed;
      content.style.display = isCollapsed ? "none" : "flex";
      chevron.style.transform = isCollapsed ? "rotate(-90deg)" : "rotate(0deg)";
    };
  }

  section.appendChild(header);
  section.appendChild(content);

  return section;
}

function createButton(
  text: string,
  onClick: () => void,
  theme: string,
  variant: "primary" | "secondary" | "danger" = "primary",
  disabled: boolean = false,
): HTMLButtonElement {
  const colors = getThemeColors(theme);
  const btn = document.createElement("button");
  btn.textContent = text;
  btn.disabled = disabled;

  const variantStyles = {
    primary: {
      bg: colors.primary,
      hover: colors.primaryHover,
      text: "#ffffff",
    },
    secondary: {
      bg: colors.bgTertiary,
      hover: colors.border,
      text: colors.text,
    },
    danger: {
      bg: colors.error,
      hover: "#dc2626",
      text: "#ffffff",
    },
  };

  const style = variantStyles[variant];
  btn.style.cssText = `
    padding: 10px 16px;
    background: ${disabled ? colors.bgTertiary : style.bg};
    color: ${style.text};
    border: none;
    border-radius: 8px;
    cursor: ${disabled ? "not-allowed" : "pointer"};
    font-size: 13px;
    font-weight: 500;
    transition: all 0.15s ease;
    opacity: ${disabled ? 0.5 : 1};
  `;

  if (!disabled) {
    btn.onmouseenter = () => {
      btn.style.background = style.hover;
    };
    btn.onmouseleave = () => {
      btn.style.background = style.bg;
    };
  }

  btn.onclick = (e) => {
    e.stopPropagation();
    onClick();
  };

  return btn;
}

function createInput(
  type: string,
  value: string,
  placeholder: string,
  theme: string,
  onChange?: (value: string) => void,
): HTMLInputElement {
  const colors = getThemeColors(theme);
  const input = document.createElement("input");
  input.type = type;
  input.value = value;
  input.placeholder = placeholder;
  input.style.cssText = `
    width: 100%;
    padding: 10px 12px;
    background: ${colors.bgTertiary};
    border: 1px solid ${colors.border};
    border-radius: 8px;
    color: ${colors.text};
    font-size: 13px;
    outline: none;
    transition: border-color 0.15s;
  `;

  input.onfocus = () => {
    input.style.borderColor = colors.primary;
  };
  input.onblur = () => {
    input.style.borderColor = colors.border;
  };

  if (onChange) {
    input.oninput = () => onChange(input.value);
  }

  return input;
}

function trimSlashes(value: string): string {
  return value.trim().replace(/^\/+|\/+$/g, "");
}

function buildProfileRemoteRoot(provider: string, server: string, remoteRoot: string): string {
  const trimmedServer = server.trim().replace(/\/+$/g, "");
  const normalizedRemoteRoot = trimSlashes(remoteRoot || DEFAULT_REMOTE_ROOT);

  if (provider === "webdav") {
    if (!normalizedRemoteRoot) return trimmedServer;
    const lowerServer = trimmedServer.toLowerCase();
    const lowerRoot = normalizedRemoteRoot.toLowerCase();
    if (lowerServer.endsWith(`/${lowerRoot}`)) return trimmedServer;
    return `${trimmedServer}/${normalizedRemoteRoot}`;
  }

  if (provider === "sftp") {
    const host = trimmedServer.replace(/^sftp:\/\//, "").replace(/\/.*$/, "");
    return `sftp://${host}/${normalizedRemoteRoot}`;
  }

  return normalizedRemoteRoot;
}

function createSelect(
  options: { value: string; label: string }[],
  value: string,
  theme: string,
  onChange?: (value: string) => void,
): HTMLSelectElement {
  const colors = getThemeColors(theme);
  const select = document.createElement("select");
  select.value = value;
  select.style.cssText = `
    width: 100%;
    padding: 10px 12px;
    background: ${colors.bgTertiary};
    border: 1px solid ${colors.border};
    border-radius: 8px;
    color: ${colors.text};
    font-size: 13px;
    outline: none;
  `;

  options.forEach((opt) => {
    const option = document.createElement("option");
    option.value = opt.value;
    option.textContent = opt.label;
    select.appendChild(option);
  });

  if (onChange) {
    select.onchange = () => onChange(select.value);
  }

  return select;
}

function createToggle(
  checked: boolean,
  theme: string,
  onChange: (checked: boolean) => void,
): HTMLElement {
  const colors = getThemeColors(theme);
  const container = document.createElement("div");
  container.style.cssText = `
    width: 44px;
    height: 24px;
    background: ${checked ? colors.primary : colors.bgTertiary};
    border-radius: 12px;
    position: relative;
    cursor: pointer;
    transition: background 0.2s;
  `;

  const knob = document.createElement("div");
  knob.style.cssText = `
    width: 20px;
    height: 20px;
    background: white;
    border-radius: 50%;
    position: absolute;
    top: 2px;
    left: ${checked ? "22px" : "2px"};
    transition: left 0.2s;
    box-shadow: 0 1px 3px rgba(0,0,0,0.2);
  `;

  container.appendChild(knob);
  container.onclick = (e) => {
    e.stopPropagation();
    const newChecked = !checked;
    knob.style.left = newChecked ? "22px" : "2px";
    container.style.background = newChecked ? colors.primary : colors.bgTertiary;
    onChange(newChecked);
  };

  return container;
}

function createStatusBadge(
  status: string,
  theme: string,
): HTMLElement {
  const colors = getThemeColors(theme);
  const badge = document.createElement("span");
  badge.style.cssText = `
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 12px;
    font-size: 12px;
    font-weight: 500;
  `;

  const statusConfig: Record<string, { bg: string; text: string; dot: string }> = {
    idle: { bg: colors.bgTertiary, text: colors.textSecondary, dot: colors.textSecondary },
    syncing: { bg: "#3b82f620", text: colors.primary, dot: colors.primary },
    success: { bg: "#10b98120", text: colors.success, dot: colors.success },
    error: { bg: "#ef444420", text: colors.error, dot: colors.error },
  };

  const config = statusConfig[status] || statusConfig.idle;
  badge.style.background = config.bg;
  badge.style.color = config.text;

  const dot = document.createElement("span");
  dot.style.cssText = `
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: ${config.dot};
  `;

  badge.appendChild(dot);
  badge.appendChild(document.createTextNode(status.charAt(0).toUpperCase() + status.slice(1)));

  return badge;
}

function createLoadingSpinner(theme: string): HTMLElement {
  const colors = getThemeColors(theme);
  const spinner = document.createElement("div");
  spinner.style.cssText = `
    width: 16px;
    height: 16px;
    border: 2px solid ${colors.bgTertiary};
    border-top-color: ${colors.primary};
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  `;

  if (!document.getElementById("cloud-sync-spin-keyframes")) {
    const style = document.createElement("style");
    style.id = "cloud-sync-spin-keyframes";
    style.textContent = `@keyframes spin { to { transform: rotate(360deg); } }`;
    document.head.appendChild(style);
  }

  return spinner;
}

function showInlineNotice(message: string, theme: string, isError = false): void {
  const existing = document.getElementById("cloud-sync-notice");
  if (existing) existing.remove();

  const colors = getThemeColors(theme);
  const notice = document.createElement("div");
  notice.id = "cloud-sync-notice";
  notice.style.cssText = `
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 10000;
    max-width: 360px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid ${isError ? colors.error : colors.success};
    background: ${colors.bgSecondary};
    color: ${isError ? colors.error : colors.text};
    box-shadow: 0 10px 30px rgba(0,0,0,0.18);
    font-size: 13px;
    line-height: 1.4;
  `;
  notice.textContent = message;
  document.body.appendChild(notice);
  window.setTimeout(() => notice.remove(), 4200);
}

async function getDefaultProfileId(): Promise<string | null> {
  const profiles = await invoke<SyncProfileSummary[]>("sync_profile_list");
  return profiles[0]?.id ?? null;
}

// ============================================================================
// Main Settings Panel
// ============================================================================

function createCloudSyncSettings(theme: string): HTMLElement {
  const colors = getThemeColors(theme);
  const container = document.createElement("div");
  container.style.cssText = `
    padding: 20px;
    background: ${colors.bg};
    color: ${colors.text};
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    min-height: 100%;
  `;

  // Header
  const header = document.createElement("div");
  header.style.cssText = `
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
  `;

  const headerLeft = document.createElement("div");
  headerLeft.style.cssText = "display: flex; align-items: center; gap: 12px;";

  const icon = document.createElement("div");
  icon.style.cssText = `
    width: 40px;
    height: 40px;
    background: ${colors.primary};
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
  `;
  icon.innerHTML = `
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2">
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
      <polyline points="17 8 12 3 7 8"/>
      <line x1="12" y1="3" x2="12" y2="15"/>
    </svg>
  `;

  const title = document.createElement("h2");
  title.style.cssText = `
    margin: 0;
    font-size: 20px;
    font-weight: 700;
    color: ${colors.text};
  `;
  title.textContent = "Cloud Sync";

  headerLeft.appendChild(icon);
  headerLeft.appendChild(title);

  const syncBtn = createButton("Sync Now", () => handleSyncNow(theme, container), theme, "primary");
  syncBtn.id = "cloud-sync-btn";

  header.appendChild(headerLeft);
  header.appendChild(syncBtn);
  container.appendChild(header);

  // Status Section
  const statusSection = createStatusSection(theme);
  container.appendChild(statusSection);

  // Profiles Section
  const profilesSection = createProfilesSection(theme, container);
  container.appendChild(profilesSection);

  // Sync Log Section
  const logSection = createSyncLogSection(theme);
  container.appendChild(logSection);

  return container;
}

function createStatusSection(theme: string): HTMLElement {
  const statusEl = document.createElement("div");
  statusEl.id = "cloud-sync-status";
  statusEl.style.cssText = `
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px;
    background: ${getThemeColors(theme).bgSecondary};
    border: 1px solid ${getThemeColors(theme).border};
    border-radius: 12px;
    margin-bottom: 16px;
  `;

  const left = document.createElement("div");
  left.style.cssText = "display: flex; align-items: center; gap: 12px;";

  const badge = createStatusBadge("idle", theme);
  badge.id = "cloud-sync-status-badge";

  const text = document.createElement("span");
  text.style.cssText = `font-size: 13px; color: ${getThemeColors(theme).textSecondary};`;
  text.textContent = "No active sync profile";
  text.id = "cloud-sync-status-text";

  left.appendChild(badge);
  left.appendChild(text);
  statusEl.appendChild(left);

  const lastSync = document.createElement("span");
  lastSync.style.cssText = `font-size: 12px; color: ${getThemeColors(theme).textSecondary};`;
  lastSync.textContent = "Last sync: Never";
  lastSync.id = "cloud-sync-last-sync";
  statusEl.appendChild(lastSync);

  return statusEl;
}

function createProfilesSection(theme: string, container: HTMLElement): HTMLElement {
  const colors = getThemeColors(theme);

  const profileList = document.createElement("div");
  profileList.id = "cloud-sync-profiles";

  const addBtn = createButton("Add Profile", () => showAddProfileModal(theme, container), theme, "secondary");
  addBtn.style.width = "100%";
  addBtn.style.marginTop = "12px";

  const section = createSection(
    "Sync Profiles",
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>',
    [profileList, addBtn],
    theme,
    false,
  );

  // Load profiles
  loadProfiles(theme, profileList, container);

  return section;
}

async function loadProfiles(
  theme: string,
  profileList: HTMLElement,
  container: HTMLElement,
): Promise<void> {
  try {
    const profiles = await invoke<SyncProfileSummary[]>("sync_profile_list");

    profileList.innerHTML = "";

    if (profiles.length === 0) {
      const empty = document.createElement("div");
      empty.style.cssText = `
        text-align: center;
        padding: 24px;
        color: ${getThemeColors(theme).textSecondary};
        font-size: 13px;
      `;
      empty.textContent = "No sync profiles configured. Click 'Add Profile' to get started.";
      profileList.appendChild(empty);
      return;
    }

    profiles.forEach((profile) => {
      const card = createProfileCard(profile, theme, container);
      profileList.appendChild(card);
    });
  } catch (error) {
    console.error("[CloudSync] Failed to load profiles:", error);
    profileList.innerHTML = `<div style="color: ${getThemeColors(theme).error}; font-size: 13px;">Failed to load profiles</div>`;
  }
}

function createProfileCard(
  profile: SyncProfileSummary,
  theme: string,
  container: HTMLElement,
): HTMLElement {
  const colors = getThemeColors(theme);
  const card = document.createElement("div");
  card.style.cssText = `
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
    background: ${colors.bgTertiary};
    border: 1px solid ${colors.border};
    border-radius: 8px;
    margin-bottom: 8px;
    transition: background 0.15s;
  `;

  card.onmouseenter = () => {
    card.style.background = colors.border;
  };
  card.onmouseleave = () => {
    card.style.background = colors.bgTertiary;
  };

  const left = document.createElement("div");
  left.style.cssText = "display: flex; flex-direction: column; gap: 4px;";

  const name = document.createElement("div");
  name.style.cssText = `font-size: 14px; font-weight: 500; color: ${colors.text};`;
  name.textContent = profile.name;

  const meta = document.createElement("div");
  meta.style.cssText = `font-size: 12px; color: ${colors.textSecondary};`;
  meta.textContent = `${formatProvider(profile.provider)} • ${profile.remote_root}`;

  left.appendChild(name);
  left.appendChild(meta);

  const right = document.createElement("div");
  right.style.cssText = "display: flex; align-items: center; gap: 8px;";

  const badge = createStatusBadge(profile.status, theme);
  right.appendChild(badge);

  const editBtn = createButton("Edit", () => showEditProfileModal(profile.id, theme, container), theme, "secondary");
  editBtn.style.padding = "6px 12px";
  editBtn.style.fontSize = "12px";
  right.appendChild(editBtn);

  card.appendChild(left);
  card.appendChild(right);

  return card;
}

function createSyncLogSection(theme: string): HTMLElement {
  const logContainer = document.createElement("div");
  logContainer.id = "cloud-sync-logs";
  logContainer.style.cssText = `
    max-height: 200px;
    overflow-y: auto;
    font-family: "SF Mono", "Fira Code", monospace;
    font-size: 12px;
  `;

  const section = createSection(
    "Sync Log",
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>',
    [logContainer],
    theme,
    true,
  );

  // Load logs
  loadSyncLogs(theme, logContainer);

  return section;
}

async function loadSyncLogs(theme: string, logContainer: HTMLElement): Promise<void> {
  try {
    const profileId = await getDefaultProfileId();
    if (!profileId) {
      logContainer.innerHTML = `<div style="color: ${getThemeColors(theme).textSecondary}; padding: 12px;">No sync profile configured</div>`;
      return;
    }
    const logs = await invoke<SyncLogEntry[]>("sync_get_log_entries", { profileId, limit: 50 });

    logContainer.innerHTML = "";

    if (logs.length === 0) {
      logContainer.innerHTML = `<div style="color: ${getThemeColors(theme).textSecondary}; padding: 12px;">No sync logs yet</div>`;
      return;
    }

    logs.forEach((log) => {
      const entry = document.createElement("div");
      entry.style.cssText = `
        padding: 8px 0;
        border-bottom: 1px solid ${getThemeColors(theme).border};
        color: ${
          log.level === "error"
            ? getThemeColors(theme).error
            : log.level === "warn"
              ? getThemeColors(theme).warning
              : getThemeColors(theme).textSecondary
        };
      `;

      const time = new Date(log.timestamp).toLocaleTimeString();
      entry.textContent = `[${time}] ${log.message}`;
      logContainer.appendChild(entry);
    });
  } catch (error) {
    console.error("[CloudSync] Failed to load logs:", error);
  }
}

async function handleSyncNow(theme: string, container: HTMLElement): Promise<void> {
  const btn = document.getElementById("cloud-sync-btn") as HTMLButtonElement;
  if (!btn) return;

  btn.disabled = true;
  btn.textContent = "Syncing...";

  try {
    const profileId = await getDefaultProfileId();
    if (!profileId) {
      throw new Error("No sync profile configured");
    }
    await invoke("sync_run_now", { profileId });

    // Update status
    const badge = document.getElementById("cloud-sync-status-badge");
    const text = document.getElementById("cloud-sync-status-text");
    if (badge) badge.replaceWith(createStatusBadge("success", theme));
    if (text) text.textContent = "Sync completed successfully";

    // Reload profiles
    const profileList = document.getElementById("cloud-sync-profiles");
    if (profileList) {
      await loadProfiles(theme, profileList, container);
    }

    // Reload logs
    const logContainer = document.getElementById("cloud-sync-logs");
    if (logContainer) {
      await loadSyncLogs(theme, logContainer);
    }
  } catch (error) {
    console.error("[CloudSync] Sync failed:", error);
    const badge = document.getElementById("cloud-sync-status-badge");
    const text = document.getElementById("cloud-sync-status-text");
    if (badge) badge.replaceWith(createStatusBadge("error", theme));
    if (text) text.textContent = `Sync failed: ${error}`;
  } finally {
    btn.disabled = false;
    btn.textContent = "Sync Now";
  }
}

// ============================================================================
// Modal Dialogs
// ============================================================================

function showAddProfileModal(theme: string, container: HTMLElement): void {
  removeExistingModal();

  const colors = getThemeColors(theme);
  const modal = createModalBase("Add Sync Profile", theme);

  const form = document.createElement("div");
  form.style.cssText = "display: flex; flex-direction: column; gap: 16px;";

  // Profile name
  form.appendChild(createFormField("Profile Name", "text", "My Sync Profile", theme));

  // Provider type
  const providerField = createFormSelect(
    "Provider",
    [
      { value: "webdav", label: "WebDAV" },
      { value: "sftp", label: "SFTP" },
      { value: "google_drive", label: "Google Drive" },
      { value: "one_drive", label: "OneDrive" },
    ],
    "webdav",
    theme,
  );
  form.appendChild(providerField);

  // Server URL/Host
  const serverField = createFormField("Server URL / SFTP Host", "text", "https://dav.example.com", theme);
  serverField.id = "cloud-sync-server";
  form.appendChild(serverField);

  // Username
  const usernameField = createFormField("Username", "text", "", theme);
  usernameField.id = "cloud-sync-username";
  form.appendChild(usernameField);

  // Password
  const passwordField = createFormField("Password / Access Token", "password", "", theme);
  passwordField.id = "cloud-sync-password";
  form.appendChild(passwordField);

  // Remote root
  const rootField = createFormField("Remote Path / App Folder", "text", DEFAULT_REMOTE_ROOT, theme);
  rootField.id = "cloud-sync-remote-root";
  form.appendChild(rootField);

  // Encryption toggle
  const encryptionRow = document.createElement("div");
  encryptionRow.style.cssText = "display: flex; align-items: center; justify-content: space-between;";

  const encryptionLabel = document.createElement("label");
  encryptionLabel.style.cssText = `font-size: 13px; color: ${colors.text};`;
  encryptionLabel.textContent = "Enable End-to-End Encryption";

  const encryptionToggle = createToggle(false, theme, () => {});

  encryptionRow.appendChild(encryptionLabel);
  encryptionRow.appendChild(encryptionToggle);
  form.appendChild(encryptionRow);

  // Buttons
  const buttons = document.createElement("div");
  buttons.style.cssText = "display: flex; gap: 8px; justify-content: flex-end; margin-top: 8px;";

  const testBtn = createButton("Test Connection", () => testConnection(theme), theme, "secondary");
  const cancelBtn = createButton("Cancel", () => removeExistingModal(), theme, "secondary");
  const saveBtn = createButton("Save", () => saveProfile(theme, container), theme, "primary");

  buttons.appendChild(testBtn);
  buttons.appendChild(cancelBtn);
  buttons.appendChild(saveBtn);
  form.appendChild(buttons);

  modal.querySelector("#cloud-sync-modal-content")!.appendChild(form);
  document.body.appendChild(modal);
}

function createModalBase(title: string, theme: string): HTMLElement {
  const colors = getThemeColors(theme);
  const modal = document.createElement("div");
  modal.id = "cloud-sync-modal";
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

  const dialog = document.createElement("div");
  dialog.id = "cloud-sync-modal-content";
  dialog.style.cssText = `
    background: ${colors.bg};
    border-radius: 16px;
    padding: 24px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
    max-width: 500px;
    width: 90%;
    max-height: 80vh;
    overflow-y: auto;
  `;

  const header = document.createElement("div");
  header.style.cssText = `
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  `;

  const titleEl = document.createElement("h3");
  titleEl.style.cssText = `
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: ${colors.text};
  `;
  titleEl.textContent = title;

  const closeBtn = document.createElement("button");
  closeBtn.style.cssText = `
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: none;
    background: ${colors.bgTertiary};
    color: ${colors.textSecondary};
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  `;
  closeBtn.innerHTML = "✕";
  closeBtn.onclick = () => removeExistingModal();

  header.appendChild(titleEl);
  header.appendChild(closeBtn);
  dialog.appendChild(header);

  modal.appendChild(dialog);
  modal.onclick = (e) => {
    if (e.target === modal) removeExistingModal();
  };

  return modal;
}

function createFormField(
  label: string,
  type: string,
  placeholder: string,
  theme: string,
  id?: string,
): HTMLElement {
  const colors = getThemeColors(theme);
  const field = document.createElement("div");

  const labelEl = document.createElement("label");
  labelEl.style.cssText = `
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: ${colors.text};
    margin-bottom: 6px;
  `;
  labelEl.textContent = label;

  const input = createInput(type, "", placeholder, theme);
  if (id) input.id = id;

  field.appendChild(labelEl);
  field.appendChild(input);

  return field;
}

function createFormSelect(
  label: string,
  options: { value: string; label: string }[],
  defaultValue: string,
  theme: string,
): HTMLElement {
  const colors = getThemeColors(theme);
  const field = document.createElement("div");

  const labelEl = document.createElement("label");
  labelEl.style.cssText = `
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: ${colors.text};
    margin-bottom: 6px;
  `;
  labelEl.textContent = label;

  const select = createSelect(options, defaultValue, theme);
  select.id = "cloud-sync-provider";

  field.appendChild(labelEl);
  field.appendChild(select);

  return field;
}

async function testConnection(theme: string): Promise<void> {
  const provider = (document.getElementById("cloud-sync-provider") as HTMLSelectElement)?.value || "webdav";
  const server = (document.getElementById("cloud-sync-server") as HTMLInputElement)?.value || "";
  const username = (document.getElementById("cloud-sync-username") as HTMLInputElement)?.value || "";
  const password = (document.getElementById("cloud-sync-password") as HTMLInputElement)?.value || "";
  const remoteRoot =
    (document.getElementById("cloud-sync-remote-root") as HTMLInputElement)?.value || DEFAULT_REMOTE_ROOT;
  const isOAuthProvider = provider === "google_drive" || provider === "one_drive";

  if ((!isOAuthProvider && (!server || !username || !password)) || (isOAuthProvider && !password)) {
    showInlineNotice("Please fill in all required fields", theme, true);
    return;
  }

  const profileId = `temp_test_${Date.now()}`;
  try {
    const passwordRef = await invoke<SecretRef>("sync_secret_set", {
      profileId,
      key: "password",
      value: password,
    });
    const usernameRef = isOAuthProvider
      ? null
      : await invoke<SecretRef>("sync_secret_set", {
          profileId,
          key: "username",
          value: username,
        });
    await invoke("sync_profile_update", {
      profile: {
        id: profileId,
        name: "Test",
        provider,
        remote_root: buildProfileRemoteRoot(provider, server, remoteRoot),
        credential_refs: {
          ...(usernameRef ? { username: usernameRef.ref_id } : {}),
          password: passwordRef.ref_id,
        },
      },
    });
    const result = await invoke<ConnectionTestResult>("sync_test_connection", { profileId });

    if (result.success) {
      showInlineNotice("Connection successful", theme);
    } else {
      showInlineNotice(`Connection failed: ${result.message}`, theme, true);
    }
  } catch (error) {
    showInlineNotice(`Connection test failed: ${error}`, theme, true);
  } finally {
    invoke("sync_profile_delete", { profileId }).catch(() => {});
  }
}

async function saveProfile(theme: string, container: HTMLElement): Promise<void> {
  const provider = (document.getElementById("cloud-sync-provider") as HTMLSelectElement)?.value || "webdav";
  const server = (document.getElementById("cloud-sync-server") as HTMLInputElement)?.value || "";
  const username = (document.getElementById("cloud-sync-username") as HTMLInputElement)?.value || "";
  const password = (document.getElementById("cloud-sync-password") as HTMLInputElement)?.value || "";
  const remoteRoot =
    (document.getElementById("cloud-sync-remote-root") as HTMLInputElement)?.value || DEFAULT_REMOTE_ROOT;
  const isOAuthProvider = provider === "google_drive" || provider === "one_drive";

  if ((!isOAuthProvider && (!server || !username || !password)) || (isOAuthProvider && !password)) {
    showInlineNotice("Please fill in all required fields", theme, true);
    return;
  }

  try {
    const profileId = `profile_${Date.now()}`;
    const passwordRef = await invoke<SecretRef>("sync_secret_set", {
      profileId,
      key: "password",
      value: password,
    });
    const usernameRef = isOAuthProvider
      ? null
      : await invoke<SecretRef>("sync_secret_set", {
          profileId,
          key: "username",
          value: username,
        });
    const profile: SyncProfile = {
      id: profileId,
      name: "My Sync Profile",
      provider: provider as Provider,
      remote_root: buildProfileRemoteRoot(provider, server, remoteRoot),
      credential_refs: {
        ...(usernameRef ? { username: usernameRef.ref_id } : {}),
        password: passwordRef.ref_id,
      },
      sync_tabs: { mode: "all", selected_tab_ids: [] },
      sync_plugins: {
        mode: "selected",
        selected_plugin_ids: [],
        include_plugin_bundles: false,
        include_granted_permissions: false,
      },
      schedule: {
        manual: true,
        sync_on_startup: false,
        startup_delay_seconds: 15,
        sync_on_local_change: true,
        local_change_debounce_seconds: 30,
        interval_minutes: 0,
        retry_backoff_seconds: [30, 120, 300, 900],
        pause_on_metered_network: false,
      },
      encryption: { enabled: false, algorithm: "xchacha20poly1305", kdf: "argon2id" },
    };

    await invoke("sync_profile_update", { profile });

    removeExistingModal();

    // Reload profiles
    const profileList = document.getElementById("cloud-sync-profiles");
    if (profileList) {
      await loadProfiles(theme, profileList, container);
    }
  } catch (error) {
    showInlineNotice(`Failed to save profile: ${error}`, theme, true);
  }
}

function showEditProfileModal(profileId: string, theme: string, container: HTMLElement): void {
  removeExistingModal();

  const colors = getThemeColors(theme);
  const modal = createModalBase("Edit Profile", theme);

  const form = document.createElement("div");
  form.style.cssText = "display: flex; flex-direction: column; gap: 16px;";

  // Placeholder for loading
  const loading = document.createElement("div");
  loading.style.cssText = `text-align: center; padding: 24px; color: ${colors.textSecondary};`;
  loading.innerHTML = `<div style="display: inline-block;">${createLoadingSpinner(theme).outerHTML}</div><p>Loading profile...</p>`;
  form.appendChild(loading);

  modal.querySelector("#cloud-sync-modal-content")!.appendChild(form);
  document.body.appendChild(modal);

  // Load profile data
  loadProfileData(profileId, theme, form, container);
}

async function loadProfileData(
  profileId: string,
  theme: string,
  form: HTMLElement,
  container: HTMLElement,
): Promise<void> {
  const colors = getThemeColors(theme);

  try {
    const profile = await invoke<SyncProfile>("sync_profile_get", { profileId });

    form.innerHTML = "";

    // Profile name
    const nameField = createFormField("Profile Name", "text", profile.name, theme);
    (nameField.querySelector("input") as HTMLInputElement).id = "cloud-sync-edit-name";
    form.appendChild(nameField);

    // Provider (read-only)
    const providerRow = document.createElement("div");
    providerRow.style.cssText = `
      padding: 10px 12px;
      background: ${colors.bgTertiary};
      border-radius: 8px;
      font-size: 13px;
      color: ${colors.textSecondary};
    `;
    providerRow.textContent = `Provider: ${formatProvider(profile.provider)}`;
    form.appendChild(providerRow);

    // Remote root
    const rootField = createFormField("Remote Path", "text", "", theme);
    const rootInput = rootField.querySelector("input") as HTMLInputElement;
    rootInput.id = "cloud-sync-edit-root";
    rootInput.value = profile.remote_root;
    form.appendChild(rootField);

    // Tab sync selection
    const tabRow = document.createElement("div");
    tabRow.style.cssText = "display: flex; flex-direction: column; gap: 8px;";

    const tabLabel = document.createElement("label");
    tabLabel.style.cssText = `font-size: 13px; font-weight: 500; color: ${colors.text};`;
    tabLabel.textContent = "Tabs to Sync";

    const tabSelect = createSelect(
      [
        { value: "all", label: "All Tabs" },
        { value: "selected", label: "Custom Tabs" },
      ],
      profile.sync_tabs.mode,
      theme,
    );
    tabSelect.id = "cloud-sync-edit-tabs";

    tabRow.appendChild(tabLabel);
    tabRow.appendChild(tabSelect);
    form.appendChild(tabRow);

    // Schedule
    const scheduleRow = document.createElement("div");
    scheduleRow.style.cssText = "display: flex; align-items: center; justify-content: space-between;";

    const scheduleLabel = document.createElement("label");
    scheduleLabel.style.cssText = `font-size: 13px; color: ${colors.text};`;
    scheduleLabel.textContent = "Enable Auto-Sync";

    const scheduleToggle = createToggle(profile.schedule.sync_on_startup, theme, () => {});

    scheduleRow.appendChild(scheduleLabel);
    scheduleRow.appendChild(scheduleToggle);
    form.appendChild(scheduleRow);

    // Buttons
    const buttons = document.createElement("div");
    buttons.style.cssText = "display: flex; gap: 8px; justify-content: flex-end; margin-top: 8px;";

    const deleteBtn = createButton("Delete", () => deleteProfile(profileId, theme, container), theme, "danger");
    const cancelBtn = createButton("Cancel", () => removeExistingModal(), theme, "secondary");
    const saveBtn = createButton("Save Changes", () => updateProfile(profileId, theme, container), theme, "primary");

    buttons.appendChild(deleteBtn);
    buttons.appendChild(cancelBtn);
    buttons.appendChild(saveBtn);
    form.appendChild(buttons);
  } catch (error) {
    form.innerHTML = `<div style="color: ${colors.error}; padding: 16px; text-align: center;">Failed to load profile: ${error}</div>`;
  }
}

async function deleteProfile(profileId: string, theme: string, container: HTMLElement): Promise<void> {
  try {
    await invoke("sync_profile_delete", { profileId });
    removeExistingModal();

    const profileList = document.getElementById("cloud-sync-profiles");
    if (profileList) {
      await loadProfiles(theme, profileList, container);
    }
  } catch (error) {
    showInlineNotice(`Failed to delete profile: ${error}`, theme, true);
  }
}

async function updateProfile(profileId: string, theme: string, container: HTMLElement): Promise<void> {
  try {
    const profile = await invoke<SyncProfile>("sync_profile_get", { profileId });

    profile.remote_root =
      (document.getElementById("cloud-sync-edit-root") as HTMLInputElement)?.value.trim() || profile.remote_root;
    profile.sync_tabs.mode = (document.getElementById("cloud-sync-edit-tabs") as HTMLSelectElement)?.value as "all" | "selected";
    if (profile.sync_tabs.mode === "all") {
      profile.sync_tabs.selected_tab_ids = [];
    }

    await invoke("sync_profile_update", { profile });
    removeExistingModal();

    const profileList = document.getElementById("cloud-sync-profiles");
    if (profileList) {
      await loadProfiles(theme, profileList, container);
    }
  } catch (error) {
    showInlineNotice(`Failed to update profile: ${error}`, theme, true);
  }
}

function removeExistingModal(): void {
  const existing = document.getElementById("cloud-sync-modal");
  if (existing) existing.remove();
}

// ============================================================================
// Plugin Definition
// ============================================================================

const plugin: Plugin = {
  meta: {
    id: "com.cliporax.cloud-sync",
    name: "Cloud Sync",
    version: "1.0.0",
  },

  onActivate: (ctx: PluginContext) => {
    console.log("[CloudSync] Plugin activated");
  },

  onDeactivate: () => {
    console.log("[CloudSync] Plugin deactivated");
    removeExistingModal();
  },

  extensions: {
    CloudSyncSettings: {
      render: (props: ExtensionProps): HTMLElement | null => {
        const theme = props.context?.theme || "dark";
        return createCloudSyncSettings(theme);
      },

      shouldShow: (): boolean => true,
    },
  },
};

// ============================================================================
// Registration
// ============================================================================

declare global {
  interface Window {
    CliporaxPlugins: Record<string, Plugin>;
  }
}

if (typeof window !== "undefined") {
  window.CliporaxPlugins = window.CliporaxPlugins || {};
  window.CliporaxPlugins["com.cliporax.cloud-sync"] = plugin;
}

export default plugin;
type Provider = "webdav" | "sftp" | "google_drive" | "one_drive";
