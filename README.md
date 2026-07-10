# Cliporax

[English](README.md) | [Simplified Chinese](README.zh-CN.md)

**Local-first, searchable, extensible clipboard history for desktop.**

Cliporax solves a small but constant problem: copied text, images, commands, links, and temporary notes should not disappear just because you switched windows or overwrote the clipboard. It stores clipboard history in a local SQLite database by default, then gives you fast search, tabs, pinning, multi-select, paste-back, plugins, shortcuts, a CLI, and optional cloud sync.

The app is built with Tauri 2, React, TypeScript, Rust, and SQLite. The goal is a practical desktop tool for daily use, not a cloud clipboard service.

## Design Choices

- **Local by default**: history, settings, and plugin state live on the user's machine first; telemetry is not part of the default app.
- **Keyboard and search first**: optimized for frequent copy, search, and paste-back workflows, not just displaying a clipboard list.
- **Real desktop behavior**: Linux, macOS, and Windows are all targets, including focus handling, tray behavior, and paste-back.
- **Plugin-based extension**: QR workflows, image preview, sync UI, and other actions are added through local plugins.
- **Clear feature boundaries**: general OCR, semantic search, summaries, and SQLCipher full-database encryption are roadmap items, not shipped features.

## Current Features

- Text and image clipboard monitoring with automatic writes to a local SQLite database.
- Clipboard history list with virtual scrolling, search, `regx:` regex search, pinning, deletion, multi-select, and drag reordering.
- Multi-tab management, including moving or copying items between tabs.
- Sensitive-content marking and clearing, with default keyword detection for `password`, `code`, `otp`, `verification code`, `secret`, `key`, and related terms.
- Global shortcut for showing the main window. The default is `CmdOrControl+Shift+V` and can be changed in settings.
- Copying history items and pasting them back into the previous window, including Linux/macOS/Windows window and focus handling.
- System tray integration, frameless window, pin/unpin, auto-hide, and saved window size and position.
- Settings window for theme, list density, shortcuts, plugins, and sync configuration.
- Plugin system with discovery, loading, enable/disable, permission grants, configuration fields, and UI extension points.
- Built-in plugin examples: QR code generation, QR code scanning, image preview, and the Cloud Sync settings panel.
- Cloud Sync backend foundation with configuration models, credential storage, sync status, logs, and conflict handling entry points for WebDAV, SFTP, Google Drive, and OneDrive.
- `cliporax-cli` command-line tool for reading, searching, copying, and saving clipboard history.
- English and Chinese UI strings.

## Tech Stack

- Desktop framework: Tauri 2
- Frontend: React 19, TypeScript, Vite, Tailwind CSS v4
- State management: Zustand
- Backend: Rust, Tokio, SQLx
- Database: SQLite
- Plugins: local plugin packages, manifest permission declarations, frontend extension points, and backend lifecycle management
- Tests: Vitest and Rust unit tests

## Project Structure

```text
.
├── src/                    # React frontend, state, components, and plugin frontend runtime
├── src-tauri/              # Rust/Tauri backend, database, IPC, sync, and plugin lifecycle
├── scripts/                # Plugin builds, CLI preparation, and agent check scripts
├── docs/                   # Public technical documentation
├── agent/skills/           # Project collaboration and check workflows
└── package.json            # Frontend and Tauri development commands
```

## Quick Start

### Requirements

- Node.js and npm
- Rust stable. The project requires Rust `1.77.2+`.
- System dependencies required by Tauri 2
- For Linux packaging and clipboard support, `xclip` and `x11-utils` are recommended.

### Install Dependencies

```bash
npm install
```

Plugin packages are maintained in the separate `CliporaxPlugins/` market repository. The main app no longer keeps plugin source copies under `plugins/`.

### Start Development

```bash
npm run tauri:dev
```

This command prepares the CLI, runs the plugin preparation no-op for compatibility, then starts Vite and the Tauri application.

You can also start only the frontend:

```bash
npm run dev
```

## Common Scripts

```bash
npm run build              # Type-check and build the frontend
npm run tauri:build        # Build desktop application bundles
npm run test:run           # Run frontend tests
npm run plugins:dev        # Compatibility no-op when no app-local plugins exist
npm run codegen:types      # Export TypeScript types from Rust
npm run cli:build          # Build cliporax-cli
npm run cli -- list        # Run a CLI example
```

Rust tests:

```bash
cd src-tauri
cargo test
```

Fast project checks:

```bash
scripts/agent/targeted-test.sh
scripts/agent/cross-platform-check.sh
scripts/agent/git-hygiene-check.sh
```

## CLI Examples

```bash
npm run cli -- list --limit 10
npm run cli -- get latest --raw
npm run cli -- search "token"
npm run cli -- copy "hello from Cliporax" --save
npm run cli -- save --file ./notes.txt
```

The CLI connects to the local SQLite database created by Cliporax, so the desktop app must have been run at least once to initialize the app data directory.

## Plugin System

Plugin packages are distributed through the plugin market. Official plugin source and package metadata live in `../CliporaxPlugins/`; installed plugins are copied into the app data plugin directory at runtime.

Legacy plugin preparation commands remain for packaging compatibility and are no-ops when `plugins/` is absent:

```bash
npm run plugins:build
npm run plugins:install
npm run plugins:dev
```

## Data And Privacy

- Clipboard history is stored by default in `cliporax.db` under the local app data directory.
- Settings are stored in `cliporax/settings.json` under the user configuration directory.
- The project does not include telemetry by default.
- Backend logs should avoid full clipboard content, keys, tokens, and decrypted sensitive data.
- Sync credentials are saved by the backend. The sync module includes encryption and unlock models, but the main SQLite clipboard database is not currently encrypted with SQLCipher.

## Cloud Sync Status

The codebase includes Cloud Sync configuration UI, provider abstractions, WebDAV/SFTP/Google Drive/OneDrive providers, sync profiles, backend credential references, encryption and unlock models, scheduler state, run reports, logs, conflict handling, and plugin configuration sync entry points.

It is no longer only a settings shell, but it should still be treated as a usable foundation that needs more production refinement.

Sync-related code is mainly located in:

- `src-tauri/src/sync/`
- `src/components/Settings/CloudSyncTab.tsx`

## Release Packaging

GitHub release builds for macOS must be signed and notarized. Unsigned DMGs downloaded from GitHub are blocked by Gatekeeper and can show `"Cliporax" is damaged and can't be opened`.

Set these repository secrets before creating a macOS release:

- `APPLE_CERTIFICATE`: base64-encoded `.p12` Developer ID Application certificate.
- `APPLE_CERTIFICATE_PASSWORD`: password for the `.p12` certificate.
- `APPLE_SIGNING_IDENTITY`: Developer ID Application signing identity.
- `APPLE_ID`: Apple ID used for notarization.
- `APPLE_PASSWORD`: app-specific password for the Apple ID.
- `APPLE_TEAM_ID`: Apple Developer team ID.

If any secret is missing, release CI still compiles both macOS targets and
uploads unsigned `.app` ZIPs as workflow artifacts for inspection. It does not
attach an unsigned DMG to the GitHub release, because downloaded unsigned DMGs
are rejected by Gatekeeper. Add all six secrets to publish macOS release assets.

## Roadmap Notes

- Plugin system: discovery, loading, enable/disable, runtime unload, permission grants, configuration fields, frontend extension points, and plugin market installation are implemented. Official plugin source has moved to `CliporaxPlugins/`; next steps should focus on market release operations, update UX, and more real plugin integration testing.
- AI features: general image OCR, local semantic search, and text summaries are not implemented. The QR scanner plugin can recognize QR codes, but that is not equivalent to OCR or AI retrieval.
- Local encryption: the sync module has a remote-sync encryption model based on Argon2id and authenticated encryption, and provider credentials are saved through the backend. The main SQLite clipboard database is not currently encrypted with SQLCipher.
- Cloud Sync: settings UI, sync profiles, WebDAV/SFTP/Google Drive/OneDrive providers, credential references, connection tests, scheduling, logs, conflict entry points, and optional encryption model are implemented. Next steps should focus on verification with real services, conflict UX, credential storage hardening, and cross-platform runtime testing.
- Packaging and release: Tauri build scripts, GitHub release workflow, Linux `deb`/`rpm`, macOS DMG, and Windows NSIS bundle configuration exist. macOS releases require Apple signing and notarization secrets in GitHub Actions.

## Documentation

- [CLI Usage](docs/cli-usage.md)
- [Plugin System Design](docs/plugin-system-design.md)
- [Cloud Sync Architecture](docs/cloud-sync-architecture.md)

## License

Cliporax is licensed under the [MIT License](LICENSE).
