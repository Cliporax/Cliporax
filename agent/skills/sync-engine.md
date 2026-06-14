# Sync Engine Skill

Use this for Cliporax cloud sync changes: profiles, cursors, item maps, tombstones, conflicts, provider behavior, encryption, or scheduler logic.

## Invariants

- `sync_item_map.item_key` is the durable remote/local identity bridge.
- Remote tombstones must delete by mapped `item_key`, not by hash fallback.
- Do not advance a remote cursor when a change failed to download, decode, or apply.
- Partial success must preserve error details in the run report and status.
- Conflict resolution must be explicit: use local, use remote, keep both, or merge variants.
- Never log decrypted content, credentials, or full clipboard payloads.

## Required Verification

```bash
cd src-tauri && cargo test sync::
scripts/agent/cross-platform-check.sh
```

## Review Focus

- duplicate content across devices
- same hash but distinct item keys
- pending local changes vs remote update conflict
- tombstone idempotency
- encrypted profile locked/unlocked behavior
- scheduler retry/backoff and pause semantics
