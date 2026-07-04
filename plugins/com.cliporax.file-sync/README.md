# File Sync

Synchronizes individual local files and immutable folder snapshots through an
existing Cliporax cloud sync profile.

- File and folder clipboard items expose an “Add to File Sync” action.
- Entries larger than 100 MiB wait for confirmation in the File Sync tab.
- Transfers use resumable 8 MiB chunks managed by the Cliporax Rust backend.
- Copying a remote entry downloads it to managed cache and writes a local file
  reference to the system clipboard.

The plugin is a UI shell. It never receives provider credentials, encryption
keys, or raw file contents.
