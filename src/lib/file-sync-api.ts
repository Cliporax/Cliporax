import { invoke } from "@tauri-apps/api/core";

export type FileSyncStatus =
  | "queued"
  | "scanning"
  | "awaiting_confirmation"
  | "preparing"
  | "uploading"
  | "synced"
  | "remote"
  | "downloading"
  | "ready"
  | "cancelled"
  | "failed"
  | "deleted";

export interface FileSyncEntry {
  id: string;
  profile_id: string;
  origin_device_id: string;
  kind: "file" | "folder";
  display_name: string;
  total_size: number;
  file_count: number;
  revision: number;
  status: FileSyncStatus;
  confirmed: boolean;
  progress_bytes: number;
  error: string | null;
  synced_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface FileSyncConfig {
  default_profile_id: string | null;
  confirmation_threshold_bytes: number;
  chunk_size: number;
}

export interface FileSyncProfileOption {
  id: string;
  name: string;
  provider: "webdav" | "sftp" | "google_drive" | "one_drive";
  encryption_enabled: boolean;
}

export interface FileSyncEnqueueResult {
  entry_ids: string[];
}

export interface FileSyncClipboardItemStatus {
  visible: boolean;
  can_enqueue: boolean;
  reason: string | null;
}

export const FILE_SYNC_PLUGIN_ID = "com.cliporax.file-sync";

export const fileSync = {
  getConfig: (): Promise<FileSyncConfig> =>
    invoke("file_sync_get_config", { pluginId: FILE_SYNC_PLUGIN_ID }),
  setProfile: (profileId: string): Promise<void> =>
    invoke("file_sync_set_profile", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      profileId,
    }),
  profileOptions: (): Promise<FileSyncProfileOption[]> =>
    invoke("file_sync_profile_options", { pluginId: FILE_SYNC_PLUGIN_ID }),
  list: (profileId?: string): Promise<FileSyncEntry[]> =>
    invoke("file_sync_list", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      profileId,
    }),
  enqueueClipboardItem: (itemId: number): Promise<FileSyncEnqueueResult> =>
    invoke("file_sync_enqueue_clipboard_item", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      itemId,
    }),
  clipboardItemStatus: (itemId: number): Promise<FileSyncClipboardItemStatus> =>
    invoke("file_sync_clipboard_item_status", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      itemId,
    }),
  confirm: (entryId: string): Promise<void> =>
    invoke("file_sync_confirm", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      entryId,
    }),
  retry: (entryId: string): Promise<void> =>
    invoke("file_sync_retry", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      entryId,
    }),
  cancel: (entryId: string): Promise<void> =>
    invoke("file_sync_cancel", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      entryId,
    }),
  refresh: (profileId: string): Promise<void> =>
    invoke("file_sync_refresh", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      profileId,
    }),
  copy: (entryIds: string[]): Promise<void> =>
    invoke("file_sync_copy", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      entryIds,
    }),
  delete: (entryId: string): Promise<void> =>
    invoke("file_sync_delete", {
      pluginId: FILE_SYNC_PLUGIN_ID,
      entryId,
    }),
};
