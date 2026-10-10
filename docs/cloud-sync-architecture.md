# Cloud Sync Architecture

Cloud Sync lets Cliporax synchronize selected clipboard history and plugin settings across devices without syncing the raw SQLite database file. The feature is designed as a built-in backend capability with a plugin-powered settings UI.

This document is the public architecture overview. It intentionally omits internal review notes, release-readiness findings, and implementation task history.

## Goals

- Sync clipboard items incrementally across devices.
- Let users choose which tabs and plugin settings are included.
- Support manual sync, scheduled sync, status reporting, logs, and conflict review.
- Keep provider credentials in backend-managed storage instead of plugin JavaScript.
- Support optional end-to-end encryption for remote payloads.
- Stay portable across macOS, Linux, and Windows.

## Current Scope

The current codebase contains the Cloud Sync backend foundation and the built-in Cloud Sync settings UI. Providers and sync behavior are still being hardened, so this should be treated as an evolving feature rather than a fully mature cloud backup product.

Supported provider models in the codebase:

- WebDAV
- SFTP
- Google Drive
- OneDrive

## Non-Goals

- Syncing `cliporax.db` directly.
- Granting third-party plugins direct database, file-system, network, or credential access.
- Automatically merging conflicting clipboard content without user choice.
- Syncing plugin code bundles by default.
- Real-time collaborative replication.

## Architecture

```text
Cloud Sync settings UI
  -> Tauri sync IPC commands
    -> Rust SyncService
      -> SyncEngine
        -> local repositories
        -> provider abstraction
        -> credential and encryption services
```

Cloud Sync is not implemented as an ordinary third-party sandbox plugin. The UI is plugin-shaped so it can live inside the settings surface, but privileged work belongs to Rust backend services.

This split keeps the sensitive parts in one place:

- Provider IO is limited to configured sync profiles.
- Credentials are stored and accessed through backend services.
- SQLite reads and writes go through repository code.
- Long-running sync work can report progress, be cancelled, and recover after failure.

## Data Model

Cliporax syncs logical records, not database files. Clipboard items, tab selection, plugin configuration, device state, conflicts, and sync logs are represented by sync-specific models and tables.

Each synced clipboard object needs a stable remote identity so updates and tombstones can refer to the same item across retries. The backend maintains that identity bridge instead of deriving deletes from content hashes alone.

## Remote Layout

Remote storage is organized as ordinary files so users can inspect and back up the sync folder.

```text
/Cliporax/v1/
  manifest.json
  sync.lock
  items/<range>-<hash>.json
  order/<hash>.json
  plugins/<hash>.json
  blobs/<hash>.bin
  .tmp/
```

The exact file layout can evolve with schema versions. Providers should treat remote paths as provider-specific normalized paths, while the sync engine works with logical object names.

## Recovery and Conflicts

A snapshot hash mismatch can mean a legacy upload updated an object without publishing its manifest. If the object can still be decrypted and parsed, sync recovers its records and reports a warning. Authenticated decryption failures and unreadable records are retained for retry; healthy records continue downloading and uploading. Partial pulls keep the original remote references and defer cleanup so unavailable data is not discarded.

New snapshots use content-addressed item shards, order files, and plugin files. Changed objects are uploaded and read back before the manifest is published, so an interrupted upload leaves the previous manifest's objects intact. Existing schema-2 clients can follow these references.

Concurrent clipboard and plugin edits create durable pending conflicts. The settings screen previews both versions. Clipboard conflicts support keeping local, remote, or both; plugin settings support keeping local or remote. Pending conflicts retain local outbox entries and publish the remote side until the user chooses. Successfully applied or published plugin values have a digest baseline in `sync_state` so ordinary local edits do not create conflicts against an unchanged remote version.

Partial progress does not trigger profile-wide failure backoff. Manual conflicts alone do not wake the local-change scheduler, while automatic policies and new publishable changes do; unreadable remote records are retried on the configured periodic schedule.

## Credentials

Credentials must not be stored in plugin JSON config. The plugin UI sends secrets to dedicated backend IPC commands, and saved profiles reference those credentials by opaque IDs.

For public builds, document the credential-storage guarantees clearly. If platform keychains are unavailable for a provider or platform, the UI should explain the fallback before the user enables sync.

## Encryption

End-to-end encryption is optional at the sync profile level. When enabled, remote payloads are encrypted before upload, and unlock state is managed by the backend. Remote manifests may include schema, algorithm, and key-derivation metadata, but not plaintext passwords or decrypted content.

Sensitive clipboard items should not be uploaded unless the user explicitly allows it for that sync profile.

## Conflict Handling

Conflicts are expected in multi-device sync. Cliporax should preserve enough local and remote context for users to choose a resolution direction, such as keeping the local version, keeping the remote version, or keeping both.

The default is manual selection. Each profile can explicitly save `prefer_local` or `prefer_remote` as its automatic conflict policy. Automatic processing resolves only entities successfully read from the current remote snapshot; unreadable data remains pending. Choices are recorded in `sync_conflicts` and queued as `sync_resolution` changes.

The local/remote buttons save the choice and immediately run sync. Resolution waits for an active run of the same profile, then holds that profile's operation gate until publication finishes. Automatic resolution happens between pull and upload within the same remote lock. Chosen clipboard and plugin versions receive a timestamp newer than both conflicting versions so other devices accept the decision even with clock skew. The consumed remote timestamp or plugin digest is retained as the baseline, so a failed publication can retry without recreating the same conflict.

The resolution IPC returns a report or a separate publication error. If publishing fails, the saved choice and outbox entries stay durable and the UI shows that a retry is needed. Partial success preserves error details in sync reports so a later run can retry safely.

## Plugin Settings Sync

Plugin configuration can be synchronized separately from clipboard content. Plugin code bundles are not synced by default, and enabling a plugin on one device should not automatically grant permissions on another device.

If a plugin configuration contains secrets, those fields must be stored through backend credential services and represented by secret references before sync.

## Cross-Platform Notes

- Use Rust `PathBuf` and provider path helpers instead of hand-built path separators.
- SFTP behavior should not depend on a system `ssh` binary or `ssh-agent`.
- Sync work should not block the UI thread.
- IPC inputs should validate IDs, list sizes, string lengths, provider names, and unsupported enum values.

## Related Code

- `src-tauri/src/sync/`
- `src/components/Settings/CloudSyncTab.tsx`
- Official plugin packages live in the separate `CliporaxPlugins/` market repository; Cloud Sync's app UI and services live in this repository.

## Status

Cloud Sync is available as a built-in settings surface backed by Rust services. Provider support, conflict handling, encryption UX, and integration coverage should continue to mature before presenting it as a finished, production-grade sync system.
