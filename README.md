<div align="center">
  <img src="public/icon.png" width="96" alt="Cliporax logo">
  <h1>Cliporax</h1>
  <p>A fast, local-first clipboard manager for Windows, macOS, and Linux.</p>

  [![Latest release](https://img.shields.io/github/v/release/Cliporax/Cliporax?style=flat-square)](https://github.com/Cliporax/Cliporax/releases/latest)
  [![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-5b5bf7?style=flat-square)](#install)
  [![License](https://img.shields.io/github/license/Cliporax/Cliporax?style=flat-square)](LICENSE)

  [Download](https://github.com/Cliporax/Cliporax/releases/latest) · [Quick start](#quick-start) · [Plugins](#extend-with-plugins) · [简体中文](README.zh-CN.md)
</div>

<br>

![Cliporax clipboard history](docs/images/cliporax-clipboard.png)

Cliporax keeps copied text, commands, links, and notes close at hand. Open it from anywhere, search your history, and paste an item back into the app you were using. Your history stays on your device unless you explicitly configure sync.

## Install

Download the newest build from [GitHub Releases](https://github.com/Cliporax/Cliporax/releases/latest).

| Platform | Package | Install |
| --- | --- | --- |
| Windows | `.exe` (NSIS) | Run the installer and follow the prompts. |
| macOS | `.dmg` for Apple Silicon or Intel | Open the DMG and drag Cliporax into Applications. |
| Linux | `.AppImage`, `.deb`, or `.rpm` | Use the portable AppImage or install the package for your distribution. |

macOS release builds are currently not notarized. If Gatekeeper blocks the first launch, open **System Settings → Privacy & Security** and choose **Open Anyway** for Cliporax.

On Linux, installing `xclip` and `x11-utils` is recommended for clipboard paste-back support:

```bash
sudo apt install xclip x11-utils
```

## Quick start

1. Copy text as usual. Cliporax saves it to your local history.
2. Press <kbd>Ctrl/Cmd</kbd> + <kbd>Shift</kbd> + <kbd>V</kbd> to open Cliporax anywhere.
3. Double-click an item to paste it back into the previous app.
4. Press <kbd>Ctrl/Cmd</kbd> + <kbd>F</kbd> to search. Prefix a query with `regx:` for regular-expression search.

![Search clipboard history in Cliporax](docs/images/cliporax-search.png)

Pin frequently used items, organize history into tabs, or use <kbd>Ctrl/Cmd</kbd> + click to select several items for a bulk action. Shortcuts can be changed in **Settings → Shortcuts**.

## Extend with plugins

Open **Settings → Plugins → Market**, choose a plugin, review its requested permissions, and select **Install**. Installed content-tab plugins appear in the navigation bar; action plugins appear only when they apply to the selected clipboard item.

The official market currently includes:

- **TODO** — turn Cliporax into a lightweight grouped task list.
- **File Sync** — sync selected files and folder snapshots through a configured cloud profile.
- **Clipboard Import** — import text history from CopyQ, GPaste, Ditto, Klipper, Maccy, Raycast, or NDJSON exporters.
- **Translate** — translate selected clipboard text with a configurable provider.
- **QR Code & QR Scanner** — generate QR codes or scan one from a screen region.
- **Image Preview** — open clipboard images in a resizable, zoomable window.

<table>
  <tr>
    <td width="50%"><img src="docs/images/cliporax-todo.png" alt="Cliporax TODO plugin"></td>
    <td width="50%"><img src="docs/images/cliporax-file-sync.png" alt="Cliporax File Sync plugin"></td>
  </tr>
  <tr>
    <td align="center"><strong>TODO</strong> — grouped tasks beside your clipboard</td>
    <td align="center"><strong>File Sync</strong> — synced, remote, and in-progress files</td>
  </tr>
</table>

Plugin source, manifests, and releases live in the [Cliporax plugin market](https://github.com/Cliporax/cliporax-plugin-market).

## Build from source

Install [Node.js](https://nodejs.org/), Rust 1.77.2 or newer, and the [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/), then run:

```bash
npm install
npm run tauri:dev
```

Create a release bundle with `npm run tauri:build`. CLI usage and plugin development are documented separately:

- [CLI usage](docs/cli-usage.md)
- [Plugin system](docs/plugin-system-design.md)

## Help and contribute

Found a bug or have an idea? [Open an issue](https://github.com/Cliporax/Cliporax/issues). Contributions are welcome.

Cliporax is available under the [MIT License](LICENSE).
