# Cliporax Agent Instructions

Cliporax is a privacy-first clipboard manager for macOS, Linux, and Windows, built with Tauri 2, React, TypeScript, Rust, SQLx/SQLite, Tailwind CSS v4, Zustand, and local plugins.

This file contains project-wide constraints and navigation. Read detailed procedures in `agent/skills/` only when relevant; use `scripts/agent/` for repeatable checks. Keep this entry point concise and avoid duplicating procedures here.

## Working Agreement

- Deliver the requested outcome end to end: inspect, implement, verify, and review. Stop at analysis or planning only when requested.
- Inspect the relevant code, callers, tests, and Git status before editing. Prefer existing patterns and the smallest coherent change; avoid unrelated refactors or speculative abstractions.
- Make routine, reversible implementation decisions autonomously. Ask only when missing information materially changes the outcome, or an action requires authorization that has not already been given. Continue independent work while clarification is pending.
- Scale planning to uncertainty and risk. Small changes need no formal plan; cross-subsystem or high-risk work needs a short plan with observable completion criteria. Update it when evidence changes the approach.
- Preserve existing staged, unstaged, and untracked work. Do not revert, overwrite, or stage others' changes without authorization, or claim them as your own. A dirty worktree alone is not a blocker.
- Treat source, logs, test output, and measurements as evidence. Check assumptions that affect correctness; distinguish verified behavior from inference and unavailable checks.
- Finish with a concise account of what changed, what was verified, and any remaining limitation. Do not claim a test passed unless it ran successfully; avoid template reports for small tasks.

## Read on Demand

Paths below are relative to the repository root. Load the smallest relevant set, before the corresponding work.

| Trigger | Procedure |
| --- | --- |
| Medium/large work, schema or cross-layer data flow | `agent/skills/workflow/auto-plan.md` |
| After any code change, before the final response | `agent/skills/workflow/post-change-review.md` |
| Choosing verification for changed behavior | `agent/skills/quality/targeted-test.md` |
| System APIs, windowing, clipboard, files, shortcuts, CSS/WebView/browser APIs; before commits | `agent/skills/quality/cross-platform-check.md` |
| Requested review, risky changes, pre-commit review | `agent/skills/quality/code-review.md` |
| Staging, committing, pushing, release hygiene | `agent/skills/quality/git-hygiene.md` |
| Tauri commands, events, invoke wrappers, shared TS/Rust contracts | `agent/skills/domain/tauri-ipc-contract.md` |
| Cloud sync, profiles, cursors, identity maps, tombstones, conflicts, encryption, scheduler | `agent/skills/domain/sync-engine.md` |
| Runtime logs, IPC traces, lock contention, clipboard/window debugging | `agent/skills/debug/dev-log.md` |

## Code Map

- `src-tauri/src/main.rs`: app setup, command registration, managed state.
- `src-tauri/src/commands/`, `src-tauri/src/clipboard.rs`, `src-tauri/src/window_utils.rs`: IPC, clipboard monitoring/writes, window behavior.
- `src-tauri/src/db/`: SQLx/SQLite persistence.
- `src-tauri/src/sync/`, `src-tauri/src/file_sync/`: cloud sync and file sync.
- `src-tauri/src/plugin/`, `plugins/`: backend plugin system and plugin packages.
- `src/lib/tauri-api.ts`: typed frontend IPC wrappers; use these instead of raw component-level `invoke` calls.
- `src/components/`, `src/stores/`, `src/contexts/`, `src/plugin/`: React UI, state, context, plugin frontend.

## Project Constraints

### Platform and UI

- All three desktop platforms must remain supported. Guard platform-specific code with `#[cfg(...)]`, a shared helper, or an explicit fallback.
- Do not use `confirm()`, `alert()`, `prompt()`, `navigator.clipboard`, Service Workers, or WebGPU in the frontend. Use React dialogs and the existing Tauri clipboard path.
- On Linux, do not assume `set_focus()` succeeds, and do not hide a window before clipboard writes complete.
- Use `PathBuf` / `.join()` for Rust paths. Pair WebKit-prefixed CSS with standard properties where applicable.
- Keep the desktop UI dense, quiet, and task-focused. Reuse components, Tailwind v4 styles, and available Lucide icons; prevent overflow and overlap at narrow widths.

### Privacy and Boundaries

- No telemetry or data collection by default. Never log secrets, credentials, tokens, decrypted payloads, or full clipboard content.
- Detect/flag sensitive clipboard items; do not store them in plaintext when a secure path exists.
- Validate IPC inputs at the backend boundary: empty values, lengths, numeric ranges, collection sizes, and supported enum values as applicable. Keep plugin permissions least-privilege.
- Keep Rust commands, `invoke_handler` registration, typed wrappers, and shared types consistent. Handle frontend failures through state-level error handling or `try/catch`; test important success and failure behavior.
- Use contextual logs following `[Component/Module] Level: Message`. Development logs live under app data at `logs/dev-YYYY-MM-DD.log`; use the debug procedure for platform paths.

### Concurrency and Persistence

- Keep locks narrowly scoped. Do not hold them across blocking work, external processes, long database loops, or unrelated `.await` operations. If an async operation requires a lock, make the scope and ordering rationale explicit.
- Prefer `Arc<T>` with fine-grained internal locks. Clone owned managed-state handles early in Tauri commands and background loops; avoid mechanically cloning the service itself.
- Ensure SQLite foreign keys are enabled on every pooled connection, using connection options or per-connection initialization. A single `PRAGMA foreign_keys = ON` executed through a pool does not configure all connections.
- When deleting parent rows, defensively delete child rows first; do not rely only on cascade behavior.

### Sync Invariants

- `sync_item_map.item_key` is the durable remote/local identity bridge. Apply tombstones through the mapped `item_key`, never a content-hash fallback.
- Do not advance a remote cursor past a change that failed to download, decode, or apply.
- Preserve partial-failure details in run reports/status. Conflict resolution must be explicit and auditable.

### Production and Development Isolation

- Production: `src-tauri/tauri.conf.json`, identifier `com.cliporax.app`, Linux data at `~/.local/share/com.cliporax.app/`.
- Development: `src-tauri/tauri.dev.conf.json`, identifier `com.cliporax.app.dev`, Linux data at `~/.local/share/com.cliporax.app.dev/`.
- Treat these as separate applications and data stores. Never replace production with a dev build or put a dev binary at a path that shadows the production command, including `~/.local/bin/cliporax`.
- “Install locally” means a production-identifier build unless the user requests development. Report unavailable privileges; do not silently substitute a dev build.
- Before installing or restarting, verify the build identifier, package name, target executable, command resolution (`command -v cliporax` on Unix or `Get-Command cliporax` on PowerShell), and running Cliporax executables. After launch, verify the process opened the intended data directory.
- Do not copy, move, merge, rename, or redirect one build's database directory into the other without explicit migration authorization covering the exact source and destination.
- Before an authorized database migration/recovery, stop every Cliporax process, preserve `cliporax.db`, `cliporax.db-wal`, and `cliporax.db-shm` together, and validate the snapshot with a read-only SQLite integrity check before modifying data.
- Installing plugins into the dev app authorizes development verification only; it does not authorize modifying or replacing production.

## Verification and Commands

Completion means the requested behavior is implemented, relevant checks have run (or their blockers are stated), and the final diff has been reviewed for correctness, scope, and regressions.

- Choose checks by affected behavior and risk. Cover meaningful failure paths and regressions; do not add tests that merely repeat implementation details.
- After code changes, follow the post-change review procedure and use `scripts/agent/targeted-test.sh` when practical. It inspects all changes against HEAD, including pre-existing work; use explicit scoped checks when that would select unrelated work.
- Run `scripts/agent/cross-platform-check.sh` for platform-sensitive changes and before commits. A local static check is not evidence that all operating systems were tested.
- For documentation-only changes, inspect the diff, referenced paths, and instruction consistency; application builds/tests are unnecessary unless the documentation change affects executable behavior.
- Run `.sh` scripts through an available Bash environment. If unavailable, run applicable underlying checks directly and report what could not be checked. Do not present a missing tool as a passing check.

| Purpose | Command |
| --- | --- |
| Install dependencies, when needed | `npm install` |
| Frontend dev server | `npm run dev` |
| Tauri development app (dev identifier) | `npm run tauri:dev` |
| Frontend type check and build | `npm run build` |
| Frontend tests | `npm run test:run` |
| Browser end-to-end tests | `npm run test:e2e` |
| Rust tests (from repository root) | `cargo test --manifest-path src-tauri/Cargo.toml` |
| Infer checks from changed paths | `bash scripts/agent/targeted-test.sh` |
| Cross-platform static checks | `bash scripts/agent/cross-platform-check.sh` |
| Pre-commit hygiene | `bash scripts/agent/git-hygiene-check.sh` |

## Git and Releases

- Commit, push, and release only within the user's authorized scope. Check status before staging, stage explicit paths, and review `git diff --cached --stat` and the staged diff before committing.
- Do not commit build/package artifacts (`*.zip`, `*.dmg`, `*.msi`, `*.AppImage`, `dist/`, `target/`), experimental bridge code, or unrelated untracked files.
- Respect each package's package manager. In particular, plugins using `yarn.lock` must not gain `package-lock.json` unless intentionally migrating.
- If push authentication fails, report the local commit hash and exact error, with credentials redacted.
- Release tags default to `vX.X.X`. Update all relevant version files, commit the version bump, then create and push the matching tag. Never push a release tag whose version does not match the project.
- Write commit messages in English. Release notes derive from `feat:` / `fix:` prefixes; reserve them for substantial user-visible features and severe or clearly user-visible bugs.
- A feature under 100 changed lines without a meaningful new workflow/capability must not use `feat:`. Use an unprefixed message or `chore:`, `test:`, `refactor:` for maintenance, docs, internal cleanup, and small/non-severe fixes.
- Ordinary `fix:` entries may appear as `修复一下bug`; serious fixes can use `fix!:` or clear severity words to retain a specific release-note description.
