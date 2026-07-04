const PLUGIN_ID = "com.cliporax.file-sync";
const OPEN_FILE_SYNC_EVENT = "cliporax:open-file-sync";
const MAX_BATCH_COPY = 32;
const isChinese = navigator.language.toLowerCase().startsWith("zh");
const messages = isChinese
  ? {
      hostUnavailable: "Cliporax 宿主 API 不可用。",
      notSynced: "尚未同步",
      unknown: "未知",
      addToSync: "添加到文件同步",
      refresh: "刷新",
      copySelected: "复制所选",
      chooseProfile: "选择同步配置…",
      noProfile: "尚未配置云同步",
      encrypted: "已加密",
      empty: "请在本地文件或文件夹的剪贴板条目上点击同步按钮。",
      files: "个文件",
      device: "设备",
      largeWarning: "这是大型快照，确认后才会占用磁盘并开始上传。",
      confirmSync: "确认同步",
      retry: "重试",
      copy: "复制",
      cancel: "取消",
      delete: "删除",
      deleteConfirm: "确认删除？",
      batchLimit: `一次最多复制 ${MAX_BATCH_COPY} 项`,
    }
  : {
      hostUnavailable: "Cliporax host API is unavailable.",
      notSynced: "Not synced",
      unknown: "Unknown",
      addToSync: "Add to File Sync",
      refresh: "Refresh",
      copySelected: "Copy selected",
      chooseProfile: "Choose sync profile…",
      noProfile: "No cloud sync profile configured",
      encrypted: "encrypted",
      empty: "Use the sync action on a local file or folder clipboard item.",
      files: "files",
      device: "device",
      largeWarning: "Large snapshot: disk staging and upload start only after confirmation.",
      confirmSync: "Confirm sync",
      retry: "Retry",
      copy: "Copy",
      cancel: "Cancel",
      delete: "Delete",
      deleteConfirm: "Delete?",
      batchLimit: `Select at most ${MAX_BATCH_COPY} entries`,
    };

interface FileSyncConfig {
  default_profile_id: string | null;
  confirmation_threshold_bytes: number;
  chunk_size: number;
}

interface ProfileOption {
  id: string;
  name: string;
  provider: string;
  encryption_enabled: boolean;
}

interface FileSyncEntry {
  id: string;
  profile_id: string;
  origin_device_id: string;
  kind: "file" | "folder";
  display_name: string;
  total_size: number;
  file_count: number;
  revision: number;
  status: string;
  confirmed: boolean;
  progress_bytes: number;
  error: string | null;
  synced_at: string | null;
  created_at: string;
  updated_at: string;
}

interface FileSyncClipboardItemStatus {
  visible: boolean;
  can_enqueue: boolean;
  reason: string | null;
}

interface ExtensionProps {
  data?: {
    item?: {
      id: number;
      type: string;
      content: string;
    };
  };
  context?: {
    theme?: "light" | "dark";
  };
}

interface TauriInternals {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

interface RuntimePlugin {
  meta: { id: string; name: string; version: string };
  onActivate(): void;
  onDeactivate(): void;
  extensions: Record<
    string,
    {
      render(props: ExtensionProps): HTMLElement | null;
      shouldShow?(props: ExtensionProps): boolean;
    }
  >;
}

interface CliporaxWindow extends Window {
  __TAURI_INTERNALS__?: TauriInternals;
  CliporaxPlugins?: Record<string, RuntimePlugin>;
}

const hostWindow = window as CliporaxWindow;

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const api = hostWindow.__TAURI_INTERNALS__;
  if (!api) return Promise.reject(new Error(messages.hostUnavailable));
  return api.invoke<T>(command, { pluginId: PLUGIN_ID, ...args });
}

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function formatTime(value: string | null): string {
  if (!value) return messages.notSynced;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? messages.unknown : date.toLocaleString();
}

function createButton(label: string, action: () => Promise<void>, destructive = false): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.style.cssText = `
    border: 0;
    border-radius: 6px;
    padding: 5px 9px;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    color: white;
    background: ${destructive ? "#dc2626" : "#4f46e5"};
  `;
  button.onclick = async (event) => {
    event.stopPropagation();
    button.disabled = true;
    try {
      await action();
    } finally {
      button.disabled = false;
    }
  };
  return button;
}

function isFileReferenceItem(item: { type: string; content: string }): boolean {
  if (item.type === "file") return true;
  if (item.type !== "text") return false;
  const lines = item.content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  return lines.length > 0 && lines.every((line) => line.startsWith("file://"));
}

function renderFileSyncButton(props: ExtensionProps): HTMLElement | null {
  const item = props.data?.item;
  if (!item || !isFileReferenceItem(item)) return null;
  const dark = props.context?.theme === "dark";
  let itemStatus: FileSyncClipboardItemStatus | null = null;
  const button = document.createElement("button");
  button.type = "button";
  button.title = messages.addToSync;
  button.setAttribute("aria-label", messages.addToSync);
  button.textContent = "⇅";
  button.style.cssText = `
    display: none;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
    align-items: center;
    justify-content: center;
    color: ${dark ? "#94a3b8" : "#71717a"};
    background: ${dark ? "rgba(255, 255, 255, 0.1)" : "rgba(0, 0, 0, 0.05)"};
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    box-shadow: ${
      dark
        ? "0 2px 8px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255,255,255,0.1)"
        : "0 2px 8px rgba(0, 0, 0, 0.08)"
    };
    transition: all 0.15s ease;
    font-size: 14px;
    line-height: 22px;
  `;
  button.onmouseenter = () => {
    button.style.transform = "scale(1.05)";
    button.style.background = dark
      ? "rgba(255, 255, 255, 0.15)"
      : "rgba(0, 0, 0, 0.08)";
  };
  button.onmouseleave = () => {
    button.style.transform = "scale(1)";
    button.style.background = dark
      ? "rgba(255, 255, 255, 0.1)"
      : "rgba(0, 0, 0, 0.05)";
  };
  void invoke<FileSyncClipboardItemStatus>("file_sync_clipboard_item_status", {
    itemId: item.id,
  })
    .then((status) => {
      if (!status.visible) return;
      itemStatus = status;
      button.style.display = "flex";
      button.title = status.reason || messages.addToSync;
    })
    .catch(() => {
      button.style.display = "none";
    });
  button.onclick = async (event) => {
    event.stopPropagation();
    window.dispatchEvent(new CustomEvent(OPEN_FILE_SYNC_EVENT));
    if (!itemStatus?.can_enqueue) return;

    button.disabled = true;
    button.textContent = "…";
    let completed = false;
    try {
      await invoke("file_sync_enqueue_clipboard_item", { itemId: item.id });
      button.textContent = "✓";
      completed = true;
    } catch (error) {
      button.textContent = "!";
      button.title = error instanceof Error ? error.message : String(error);
    } finally {
      if (!completed) {
        window.setTimeout(() => {
          button.disabled = false;
          button.textContent = "⇅";
        }, 1600);
      }
    }
  };
  return button;
}

function renderFileSyncView(props: ExtensionProps): HTMLElement {
  const dark = props.context?.theme === "dark";
  const colors = {
    background: dark ? "#111827" : "#f8fafc",
    panel: dark ? "#1f2937" : "#ffffff",
    border: dark ? "#374151" : "#e5e7eb",
    text: dark ? "#e5e7eb" : "#1f2937",
    muted: dark ? "#9ca3af" : "#6b7280",
  };
  const root = document.createElement("section");
  root.style.cssText = `
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    color: ${colors.text};
    background: ${colors.background};
    font-family: system-ui, sans-serif;
  `;

  const toolbar = document.createElement("div");
  toolbar.style.cssText = "display:flex;align-items:center;gap:8px;min-width:0;";
  const selectedEntries = new Set<string>();
  const profileSelect = document.createElement("select");
  profileSelect.style.cssText = `
    min-width: 0;
    flex: 1;
    border: 1px solid ${colors.border};
    border-radius: 7px;
    padding: 6px 8px;
    color: ${colors.text};
    background: ${colors.panel};
    font-size: 12px;
  `;
  const refreshButton = createButton(messages.refresh, async () => {
    if (profileSelect.value) {
      await invoke("file_sync_refresh", { profileId: profileSelect.value });
    }
    await loadEntries();
  });
  const copySelectedButton = createButton(messages.copySelected, async () => {
    if (selectedEntries.size === 0) return;
    await run(() =>
      invoke("file_sync_copy", { entryIds: Array.from(selectedEntries) }),
    );
    selectedEntries.clear();
    copySelectedButton.disabled = true;
  });
  copySelectedButton.disabled = true;
  toolbar.append(profileSelect, refreshButton, copySelectedButton);

  const message = document.createElement("div");
  message.style.cssText = `min-height:16px;font-size:11px;color:${colors.muted};`;
  const list = document.createElement("div");
  list.style.cssText = "flex:1;overflow:auto;display:flex;flex-direction:column;gap:6px;";
  root.append(toolbar, message, list);

  async function run(action: () => Promise<void>): Promise<void> {
    message.textContent = "";
    try {
      await action();
      await loadEntries();
    } catch (error) {
      message.textContent = error instanceof Error ? error.message : String(error);
    }
  }

  async function loadProfiles(): Promise<void> {
    const [profiles, config] = await Promise.all([
      invoke<ProfileOption[]>("file_sync_profile_options"),
      invoke<FileSyncConfig>("file_sync_get_config"),
    ]);
    profileSelect.replaceChildren();
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = profiles.length ? messages.chooseProfile : messages.noProfile;
    profileSelect.append(placeholder);
    for (const profile of profiles) {
      const option = document.createElement("option");
      option.value = profile.id;
      option.textContent = `${profile.name} · ${profile.provider}${profile.encryption_enabled ? ` · ${messages.encrypted}` : ""}`;
      option.selected = profile.id === config.default_profile_id;
      profileSelect.append(option);
    }
    profileSelect.onchange = () =>
      run(async () => {
        if (profileSelect.value) {
          await invoke("file_sync_set_profile", { profileId: profileSelect.value });
        }
      });
  }

  async function loadEntries(): Promise<void> {
    const entries = await invoke<FileSyncEntry[]>("file_sync_list", {
      profileId: profileSelect.value || null,
    });
    const copyableIds = new Set(
      entries
        .filter((entry) => ["synced", "remote", "ready"].includes(entry.status))
        .map((entry) => entry.id),
    );
    for (const selectedId of selectedEntries) {
      if (!copyableIds.has(selectedId)) selectedEntries.delete(selectedId);
    }
    copySelectedButton.disabled = selectedEntries.size === 0;
    list.replaceChildren();
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.textContent = messages.empty;
      empty.style.cssText = `padding:24px 8px;text-align:center;font-size:12px;color:${colors.muted};`;
      list.append(empty);
      return;
    }
    for (const entry of entries) {
      const row = document.createElement("article");
      row.style.cssText = `
        display:grid;
        grid-template-columns:minmax(0,1fr) auto;
        gap:8px;
        align-items:center;
        padding:8px 10px;
        border:1px solid ${colors.border};
        border-radius:8px;
        background:${colors.panel};
      `;
      const details = document.createElement("div");
      details.style.cssText = "min-width:0;";
      const name = document.createElement("div");
      name.textContent = entry.display_name;
      name.title = entry.display_name;
      name.style.cssText = "overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:12px;font-weight:650;";
      const metadata = document.createElement("div");
      const progress =
        entry.total_size > 0 && ["preparing", "uploading", "downloading"].includes(entry.status)
          ? ` · ${Math.min(100, Math.round((entry.progress_bytes / entry.total_size) * 100))}%`
          : "";
      metadata.textContent = `${entry.kind} · ${entry.kind === "folder" ? `${entry.file_count} ${messages.files} · ` : ""}${formatSize(entry.total_size)} · ${formatTime(entry.synced_at)} · ${messages.device} ${entry.origin_device_id} · ${entry.status}${progress}`;
      metadata.style.cssText = `margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:10px;color:${colors.muted};`;
      details.append(name, metadata);
      if (entry.status === "awaiting_confirmation") {
        const warning = document.createElement("div");
        warning.textContent = messages.largeWarning;
        warning.style.cssText = "margin-top:3px;font-size:10px;color:#d97706;";
        details.append(warning);
      }
      if (entry.error) {
        const error = document.createElement("div");
        error.textContent = entry.error;
        error.style.cssText = "margin-top:3px;font-size:10px;color:#ef4444;";
        details.append(error);
      }

      const actions = document.createElement("div");
      actions.style.cssText = "display:flex;align-items:center;gap:5px;";
      if (copyableIds.has(entry.id)) {
        const select = document.createElement("input");
        select.type = "checkbox";
        select.checked = selectedEntries.has(entry.id);
        select.setAttribute("aria-label", `${messages.copySelected}: ${entry.display_name}`);
        select.onchange = () => {
          if (select.checked && selectedEntries.size >= MAX_BATCH_COPY) {
            select.checked = false;
            message.textContent = messages.batchLimit;
            return;
          }
          if (select.checked) selectedEntries.add(entry.id);
          else selectedEntries.delete(entry.id);
          copySelectedButton.disabled = selectedEntries.size === 0;
        };
        actions.append(select);
      }
      if (entry.status === "awaiting_confirmation") {
        actions.append(
          createButton(messages.confirmSync, () =>
            run(() => invoke("file_sync_confirm", { entryId: entry.id })),
          ),
        );
      } else if (entry.status === "failed" || entry.status === "cancelled") {
        actions.append(
          createButton(messages.retry, () => run(() => invoke("file_sync_retry", { entryId: entry.id }))),
        );
      } else if (["synced", "remote", "ready"].includes(entry.status)) {
        actions.append(
          createButton(messages.copy, () =>
            run(() => invoke("file_sync_copy", { entryIds: [entry.id] })),
          ),
        );
      }
      if (["queued", "scanning", "preparing", "uploading", "downloading"].includes(entry.status)) {
        actions.append(
          createButton(
            messages.cancel,
            () => run(() => invoke("file_sync_cancel", { entryId: entry.id })),
            true,
          ),
        );
      }
      if (!["queued", "scanning", "preparing", "uploading", "downloading"].includes(entry.status)) {
        const deleteButton = createButton(
          messages.delete,
          async () => {
            if (deleteButton.dataset.armed !== "true") {
              deleteButton.dataset.armed = "true";
              deleteButton.textContent = messages.deleteConfirm;
              window.setTimeout(() => {
                deleteButton.dataset.armed = "false";
                deleteButton.textContent = messages.delete;
              }, 3000);
              return;
            }
            await run(() => invoke("file_sync_delete", { entryId: entry.id }));
          },
          true,
        );
        actions.append(deleteButton);
      }
      row.append(details, actions);
      row.ondblclick = () => {
        if (["synced", "remote", "ready"].includes(entry.status)) {
          void run(() => invoke("file_sync_copy", { entryIds: [entry.id] }));
        }
      };
      list.append(row);
    }
  }

  void run(async () => {
    await loadProfiles();
    await loadEntries();
  });
  const poll = window.setInterval(() => {
    if (!root.isConnected) {
      window.clearInterval(poll);
      return;
    }
    void loadEntries().catch(() => {});
  }, 1500);
  return root;
}

const plugin: RuntimePlugin = {
  meta: { id: PLUGIN_ID, name: "File Sync", version: "0.1.0" },
  onActivate() {},
  onDeactivate() {},
  extensions: {
    FileSyncButton: {
      shouldShow: (props) => {
        const item = props.data?.item;
        return Boolean(item && isFileReferenceItem(item));
      },
      render: renderFileSyncButton,
    },
    FileSyncView: {
      render: renderFileSyncView,
    },
  },
};

hostWindow.CliporaxPlugins = hostWindow.CliporaxPlugins ?? {};
hostWindow.CliporaxPlugins[PLUGIN_ID] = plugin;
