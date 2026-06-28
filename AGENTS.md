# Cliporax Agent Instructions

You are working on Cliporax, a privacy-first cross-platform clipboard manager built with Tauri 2, React, TypeScript, Rust, SQLx/SQLite, Tailwind CSS v4, Zustand, and a local plugin system.

This file is the agent entry point. Keep detailed procedures in `agent/skills/` and deterministic checks in `scripts/agent/`.

## Operating Mode

- Read the codebase before changing it; prefer existing patterns over new abstractions.
- Implement the requested change unless the user explicitly asks for planning or review only.
- Keep changes scoped. Do not refactor unrelated code.
- After completing code changes, run the post-change review flow to verify the requested behavior, diff scope, and regression risk before the final response.
- Never revert user changes or untracked work unless explicitly asked.
- Stage explicit paths only. Do not commit build artifacts, experimental bridge code, or unrelated untracked files.

## Skill Routing

Use the smallest relevant skill set. Skills are grouped by task phase:

- Workflow:
  - `agent/skills/workflow/auto-plan.md`: medium/large work, cross frontend/backend changes, schema/data-flow changes.
  - `agent/skills/workflow/post-change-review.md`: after implementing any requested code change; verify logic correctness and unchanged behavior outside the intended scope.
- Quality gates:
  - `agent/skills/quality/targeted-test.md`: choose fast verification after changes.
  - `agent/skills/quality/cross-platform-check.md`: system APIs, windowing, clipboard, files, shortcuts, CSS/WebView/browser API changes, and before commits.
  - `agent/skills/quality/code-review.md`: review requests, risky changes, or pre-commit review.
  - `agent/skills/quality/git-hygiene.md`: staging, committing, pushing, artifact/lockfile checks.
- Domain contracts:
  - `agent/skills/domain/tauri-ipc-contract.md`: adding/changing Tauri commands, events, invoke wrappers, or shared TS/Rust contracts.
  - `agent/skills/domain/sync-engine.md`: cloud sync, profiles, cursors, item maps, tombstones, conflicts, encryption, scheduler.
- Debugging:
  - `agent/skills/debug/dev-log.md`: debugging runtime logs, IPC traces, lock contention, clipboard/window behavior.

Do not produce long template reports for small tasks. Use compact plans and targeted verification.

## Architecture Map

- `src-tauri/`: Rust backend.
- `src-tauri/src/main.rs`: app setup, command registration, managed state.
- `src-tauri/src/commands/`: Tauri IPC command handlers.
- `src-tauri/src/clipboard.rs`: clipboard monitoring and clipboard writes.
- `src-tauri/src/db/`: SQLx/SQLite database layer.
- `src-tauri/src/sync/`: cloud sync models, repository, service, engine, providers, crypto, secrets.
- `src-tauri/src/plugin/`: plugin registry, lifecycle, sandbox, permissions.
- `src/`: React frontend.
- `src/lib/tauri-api.ts`: typed frontend IPC wrappers. Use this instead of raw invoke calls in components.
- `src/components/`, `src/stores/`, `src/contexts/`, `src/plugin/`: UI, state, context, plugin frontend.
- `plugins/`: plugin packages. Existing cloud-sync plugin uses `yarn.lock`; do not add `package-lock.json` unless intentionally migrating.

## Hard Rules

### Cross-Platform

- Cliporax must support macOS, Linux, and Windows.
- Platform-specific behavior must be guarded with `#[cfg(...)]`, a shared helper, or a clear fallback.
- Frontend must not use `confirm()`, `alert()`, `prompt()`, `navigator.clipboard`, Service Workers, or WebGPU.
- Linux window/focus/clipboard behavior is fragile: do not assume `set_focus()` succeeds; avoid hiding a window before clipboard writes complete.
- Rust paths should use `PathBuf` / `.join()` instead of string separator assembly.
- WebKit-prefixed CSS must have a standard property when applicable.

Run before commits or platform-sensitive changes:

```bash
scripts/agent/cross-platform-check.sh
```

### Privacy and Security

- No telemetry or data collection by default.
- Do not log secrets, credentials, tokens, decrypted payloads, or full clipboard content.
- Sensitive clipboard items must be detected/flagged and must not be stored in plain text when a secure path exists.
- IPC command inputs must be validated for empty strings, length, numeric ranges, list sizes, and unsupported enum values.
- Plugin permissions must stay least-privilege.

### Concurrency

- Do not hold locks across long-running operations, blocking calls, infinite loops, external processes, database-heavy loops, or `.await` chains unless the lock scope is intentionally tiny.
- Prefer `Arc<T>` with fine-grained internal locks over wrapping whole services in one outer mutex.
- In Tauri commands, clone managed state quickly (`state.as_ref().clone()`) and perform work on the clone.
- Long-running background loops should own cloned handles and lock only the fields they need.

### SQLite

SQLite foreign keys are off by default. Enable them after pool creation:

```rust
sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await?;
```

When deleting parent rows, defensively delete child rows first instead of relying only on cascade behavior.

## Development Commands

```bash
npm install
npm run dev
npm run tauri:dev
npm run build
npm run test:run
cd src-tauri && cargo test
```

Fast verification:

```bash
scripts/agent/targeted-test.sh
```

Pre-commit hygiene:

```bash
scripts/agent/git-hygiene-check.sh
git status --short
git diff --cached --stat
```

## Logging

Use structured, contextual logs. Avoid clipboard content and secrets.

- Format convention: `[Component/Module] Level: Message`
- Development log files are date-rotated under app data: `logs/dev-YYYY-MM-DD.log`
- See `agent/skills/debug/dev-log.md` for exact platform paths and grep/jq commands.

## IPC Contract

When adding or changing commands:

- Add/modify Rust command and register it in `invoke_handler`.
- Add/update typed wrapper and types in `src/lib/tauri-api.ts`.
- Handle frontend errors with state-level error handling or `try/catch`.
- Add focused tests for important success/failure behavior.
- Use `agent/skills/domain/tauri-ipc-contract.md` for the full checklist.

## Sync Engine Invariants

For `src-tauri/src/sync/` changes:

- `sync_item_map.item_key` is the durable remote/local identity bridge.
- Tombstones must delete by mapped `item_key`, not content hash fallback.
- Do not advance a remote cursor when a remote change failed to download, decode, or apply.
- Partial success must preserve error details in run report/status.
- Conflict resolution must be explicit and auditable.
- Use `agent/skills/domain/sync-engine.md` before editing sync behavior.

## Frontend Guidance

- Build the actual tool UI, not marketing pages.
- Keep the desktop app dense, quiet, and task-focused.
- Use Tailwind v4 and existing components/styles.
- Use Lucide icons where available.
- Avoid native browser dialogs; use React UI or direct action for non-critical operations.
- Ensure text does not overflow or overlap on narrow app widths.

## Git Hygiene

- Check status before staging.
- Stage explicit paths, not broad `git add .`, unless the change is intentionally repository-wide.
- Do not commit package/build artifacts (`*.zip`, `*.dmg`, `*.msi`, `*.AppImage`, `dist/`, `target/`).
- Do not mix `package-lock.json` into a plugin package that already uses `yarn.lock` unless migrating package managers.
- If push fails due to auth, report the local commit hash and exact error.

### Commit Message and Changelog Rules

- GitHub release changelogs are generated from commit prefixes. Only `feat:` and `fix:` commits are listed; commits without these prefixes are intentionally omitted from release notes.
- Use `feat:` only for substantial user-visible features. If the feature change is under 100 changed lines and does not add a meaningful new workflow or capability, do not use `feat:`.
- Use `fix:` only for severe or clearly user-visible bugs. For small/non-severe bug fixes, do not use `fix:`; use an unprefixed concise message instead.
- Ordinary `fix:` entries may be summarized in release notes as `修复一下bug`. Serious fixes can use `fix!:` or include clear severity words in the subject so release notes can show the specific fix.
- Use unprefixed or non-release prefixes such as `chore:`, `test:`, `refactor:`, or plain concise messages for maintenance, small fixes, tests, docs, and internal cleanup that should not appear in release notes.
