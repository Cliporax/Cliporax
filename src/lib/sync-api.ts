import { invoke } from "@tauri-apps/api/core";

const log = (
  level: "info" | "debug" | "error",
  component: string,
  message: string,
  ...args: unknown[]
) => {
  const timestamp = new Date().toISOString();
  const formatted = `[${timestamp}] [${component}] ${level.toUpperCase()}: ${message}`;
  if (level === "error") {
    console.error(formatted, ...args);
  } else if (level === "debug") {
    console.debug(formatted, ...args);
  } else {
    console.log(formatted, ...args);
  }
};

export interface SyncProfileSummary {
  id: string;
  name: string;
  provider: "webdav" | "sftp" | "google_drive" | "one_drive";
  remote_root: string;
  encryption_enabled: boolean;
  last_sync_at: string | null;
  status: string;
}

export interface SyncProfile {
  id: string;
  name: string;
  provider: "webdav" | "sftp" | "google_drive" | "one_drive";
  remote_root: string;
  sync_tabs: TabSyncSelection;
  sync_plugins: PluginSyncSelection;
  encryption: EncryptionConfig;
  credential_refs: CredentialRefs;
  schedule: SyncScheduleConfig;
  created_at: string | null;
  updated_at: string | null;
}

export interface TabSyncSelection {
  mode: "all" | "selected";
  selected_tab_ids: number[];
}

export interface PluginSyncSelection {
  mode: "selected";
  selected_plugin_ids: string[];
  include_plugin_bundles: boolean;
  include_granted_permissions: boolean;
}

export interface EncryptionConfig {
  enabled: boolean;
  algorithm: string;
  kdf: string;
  salt_b64?: string | null;
  memory_kb?: number;
  iterations?: number;
  parallelism?: number;
}

export interface CredentialRefs {
  username?: string;
  password?: string;
  private_key?: string;
  passphrase?: string;
}

export interface SyncScheduleConfig {
  manual: boolean;
  sync_on_startup: boolean;
  startup_delay_seconds: number;
  sync_on_local_change: boolean;
  local_change_debounce_seconds: number;
  interval_minutes: number;
  retry_backoff_seconds: number[];
  pause_on_metered_network: boolean;
  paused?: boolean;
}

export interface SyncProfileInput {
  id: string;
  name: string;
  provider: string;
  remote_root: string;
  sync_tabs?: TabSyncSelection;
  sync_plugins?: PluginSyncSelection;
  encryption?: EncryptionConfig;
  credential_refs?: CredentialRefs;
  schedule?: SyncScheduleConfig;
}

export interface SecretRef {
  ref_id: string;
  profile_id: string;
  key: string;
}

export interface ConnectionTestResult {
  success: boolean;
  message: string;
  server_info?: string;
}

export interface SftpHostKeyTrustResult {
  host: string;
  port: number;
  fingerprint_sha256: string;
  known_hosts_path: string;
}

export interface SyncRunReport {
  profile_id: string;
  run_id: string;
  status: SyncRunStatus;
  started_at: string;
  completed_at: string | null;
  items_uploaded: number;
  items_downloaded: number;
  items_deleted: number;
  conflicts_found: number;
  errors: string[];
}

export type SyncRunStatus =
  | "idle"
  | "waiting_for_lock"
  | "pulling"
  | "applying_remote"
  | "uploading"
  | "committing"
  | "completed"
  | "partial_success"
  | "failed";

export interface SyncStatus {
  profile_id: string;
  status: SyncRunStatus;
  phase: string | null;
  progress: number | null;
  last_sync_at: string | null;
  next_sync_at: string | null;
  is_paused: boolean;
  is_locked: boolean;
  backoff_reason: string | null;
}

export interface SyncConflict {
  id: number;
  entity_type: string;
  entity_key: string;
  local_payload: string;
  remote_payload: string;
  reason: string;
  status: string;
  resolution: string | null;
  created_at: string;
  resolved_at: string | null;
}

export type ConflictResolutionInput =
  | "use_local"
  | "use_remote"
  | "keep_both"
  | "merge_with_local_primary"
  | "merge_with_remote_primary";

export interface SyncTabOption {
  id: number;
  name: string;
}

export interface SyncPluginOption {
  id: string;
  name: string;
  is_active: boolean;
}

export interface SyncLogEntry {
  timestamp: string;
  level: string;
  message: string;
  profile_id: string | null;
  run_id: string | null;
}

export const sync = {
  profileList: async (): Promise<SyncProfileSummary[]> => {
    log("info", "API", "sync.profileList() called");
    try {
      const result = await invoke<SyncProfileSummary[]>("sync_profile_list");
      log(
        "info",
        "API",
        "sync.profileList() returned",
        result.length,
        "profiles",
      );
      return result;
    } catch (error) {
      log("error", "API", "sync.profileList() failed", error);
      throw error;
    }
  },

  profileGet: async (profileId: string): Promise<SyncProfile> => {
    log("info", "API", "sync.profileGet() called - profileId:", profileId);
    try {
      const result = await invoke<SyncProfile>("sync_profile_get", {
        profileId,
      });
      log("info", "API", "sync.profileGet() success");
      return result;
    } catch (error) {
      log("error", "API", "sync.profileGet() failed", error);
      throw error;
    }
  },

  profileUpdate: async (profile: SyncProfileInput): Promise<void> => {
    log("info", "API", "sync.profileUpdate() called - profileId:", profile.id);
    try {
      await invoke<void>("sync_profile_update", { profile });
      log("info", "API", "sync.profileUpdate() success");
    } catch (error) {
      log("error", "API", "sync.profileUpdate() failed", error);
      throw error;
    }
  },

  profileDelete: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profileDelete() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_delete", { profileId });
      log("info", "API", "sync.profileDelete() success");
    } catch (error) {
      log("error", "API", "sync.profileDelete() failed", error);
      throw error;
    }
  },

  profilePause: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profilePause() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_pause", { profileId });
      log("info", "API", "sync.profilePause() success");
    } catch (error) {
      log("error", "API", "sync.profilePause() failed", error);
      throw error;
    }
  },

  profileResume: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profileResume() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_resume", { profileId });
      log("info", "API", "sync.profileResume() success");
    } catch (error) {
      log("error", "API", "sync.profileResume() failed", error);
      throw error;
    }
  },

  secretSet: async (
    profileId: string,
    key: string,
    value: string,
  ): Promise<SecretRef> => {
    log(
      "info",
      "API",
      "sync.secretSet() called - profileId:",
      profileId,
      "key:",
      key,
    );
    try {
      const result = await invoke<SecretRef>("sync_secret_set", {
        profileId,
        key,
        value,
      });
      log("info", "API", "sync.secretSet() success");
      return result;
    } catch (error) {
      log("error", "API", "sync.secretSet() failed", error);
      throw error;
    }
  },

  secretDelete: async (secretRef: string): Promise<void> => {
    log("info", "API", "sync.secretDelete() called - ref:", secretRef);
    try {
      await invoke<void>("sync_secret_delete", { secretRef });
      log("info", "API", "sync.secretDelete() success");
    } catch (error) {
      log("error", "API", "sync.secretDelete() failed", error);
      throw error;
    }
  },

  profileUnlock: async (
    profileId: string,
    password: string,
    rememberWithSystemKeychain: boolean,
  ): Promise<void> => {
    log("info", "API", "sync.profileUnlock() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_unlock", {
        profileId,
        password,
        rememberWithSystemKeychain,
      });
      log("info", "API", "sync.profileUnlock() success");
    } catch (error) {
      log("error", "API", "sync.profileUnlock() failed", error);
      throw error;
    }
  },

  profileLock: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.profileLock() called - profileId:", profileId);
    try {
      await invoke<void>("sync_profile_lock", { profileId });
      log("info", "API", "sync.profileLock() success");
    } catch (error) {
      log("error", "API", "sync.profileLock() failed", error);
      throw error;
    }
  },

  testConnection: async (profileId: string): Promise<ConnectionTestResult> => {
    log("info", "API", "sync.testConnection() called - profileId:", profileId);
    try {
      const result = await invoke<ConnectionTestResult>(
        "sync_test_connection",
        { profileId },
      );
      log("info", "API", "sync.testConnection() success:", result.success);
      return result;
    } catch (error) {
      log("error", "API", "sync.testConnection() failed", error);
      throw error;
    }
  },

  trustSftpHostKey: async (
    profileId: string,
  ): Promise<SftpHostKeyTrustResult> => {
    log(
      "info",
      "API",
      "sync.trustSftpHostKey() called - profileId:",
      profileId,
    );
    try {
      const result = await invoke<SftpHostKeyTrustResult>(
        "sync_trust_sftp_host_key",
        { profileId },
      );
      log(
        "info",
        "API",
        "sync.trustSftpHostKey() success - host:",
        result.host,
      );
      return result;
    } catch (error) {
      log("error", "API", "sync.trustSftpHostKey() failed", error);
      throw error;
    }
  },

  runNow: async (profileId: string): Promise<SyncRunReport> => {
    log("info", "API", "sync.runNow() called - profileId:", profileId);
    try {
      const result = await invoke<SyncRunReport>("sync_run_now", {
        profileId,
      });
      log("info", "API", "sync.runNow() success - status:", result.status);
      return result;
    } catch (error) {
      log("error", "API", "sync.runNow() failed", error);
      throw error;
    }
  },

  cancelRun: async (profileId: string): Promise<void> => {
    log("info", "API", "sync.cancelRun() called - profileId:", profileId);
    try {
      await invoke<void>("sync_cancel_run", { profileId });
      log("info", "API", "sync.cancelRun() success");
    } catch (error) {
      log("error", "API", "sync.cancelRun() failed", error);
      throw error;
    }
  },

  getStatus: async (profileId: string): Promise<SyncStatus> => {
    log("info", "API", "sync.getStatus() called - profileId:", profileId);
    try {
      const result = await invoke<SyncStatus>("sync_get_status", {
        profileId,
      });
      log("info", "API", "sync.getStatus() success - status:", result.status);
      return result;
    } catch (error) {
      log("error", "API", "sync.getStatus() failed", error);
      throw error;
    }
  },

  getLastReport: async (profileId: string): Promise<SyncRunReport | null> => {
    log("info", "API", "sync.getLastReport() called - profileId:", profileId);
    try {
      const result = await invoke<SyncRunReport | null>(
        "sync_get_last_report",
        { profileId },
      );
      log("info", "API", "sync.getLastReport() success");
      return result;
    } catch (error) {
      log("error", "API", "sync.getLastReport() failed", error);
      throw error;
    }
  },

  getConflicts: async (profileId: string): Promise<SyncConflict[]> => {
    log("info", "API", "sync.getConflicts() called - profileId:", profileId);
    try {
      const result = await invoke<SyncConflict[]>("sync_get_conflicts", {
        profileId,
      });
      log(
        "info",
        "API",
        "sync.getConflicts() returned",
        result.length,
        "conflicts",
      );
      return result;
    } catch (error) {
      log("error", "API", "sync.getConflicts() failed", error);
      throw error;
    }
  },

  resolveConflict: async (
    profileId: string,
    conflictId: number,
    resolution: ConflictResolutionInput,
  ): Promise<void> => {
    log("info", "API", "sync.resolveConflict() called - conflictId:", conflictId);
    try {
      await invoke<void>("sync_resolve_conflict", {
        profileId,
        conflictId,
        resolution,
      });
      log("info", "API", "sync.resolveConflict() success");
    } catch (error) {
      log("error", "API", "sync.resolveConflict() failed", error);
      throw error;
    }
  },

  getTabOptions: async (): Promise<SyncTabOption[]> => {
    log("info", "API", "sync.getTabOptions() called");
    try {
      const result = await invoke<SyncTabOption[]>("sync_get_tab_options");
      log("info", "API", "sync.getTabOptions() returned", result.length, "tabs");
      return result;
    } catch (error) {
      log("error", "API", "sync.getTabOptions() failed", error);
      throw error;
    }
  },

  getPluginOptions: async (): Promise<SyncPluginOption[]> => {
    log("info", "API", "sync.getPluginOptions() called");
    try {
      const result = await invoke<SyncPluginOption[]>(
        "sync_get_plugin_options",
      );
      log(
        "info",
        "API",
        "sync.getPluginOptions() returned",
        result.length,
        "plugins",
      );
      return result;
    } catch (error) {
      log("error", "API", "sync.getPluginOptions() failed", error);
      throw error;
    }
  },

  getLogEntries: async (
    profileId: string,
    limit: number,
  ): Promise<SyncLogEntry[]> => {
    log(
      "info",
      "API",
      "sync.getLogEntries() called - profileId:",
      profileId,
      "limit:",
      limit,
    );
    try {
      const result = await invoke<SyncLogEntry[]>("sync_get_log_entries", {
        profileId,
        limit,
      });
      log(
        "info",
        "API",
        "sync.getLogEntries() returned",
        result.length,
        "entries",
      );
      return result;
    } catch (error) {
      log("error", "API", "sync.getLogEntries() failed", error);
      throw error;
    }
  },
};
