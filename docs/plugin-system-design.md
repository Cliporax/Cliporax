# Cliporax Plugin System Design

This document describes the plugin system used by Cliporax: market package layout, manifests, permissions, lifecycle, backend IPC, frontend extension points, and the current implementation roadmap.

## 1. Goals

The plugin system is designed to let users extend clipboard workflows without giving plugins broad access to the app by default.

Key goals:

- Keep installed plugin packages local and inspectable.
- Require explicit permission declarations in `manifest.json`.
- Keep permissions least-privilege and auditable.
- Support UI extension points such as card actions and settings panels.
- Keep plugin enablement and permission grants persistent across app restarts.
- Avoid remote code execution by default.

Non-goals for the current implementation:

- Keeping official plugin source copies inside the main app repository.
- Running arbitrary native binaries as plugins.
- Granting unrestricted filesystem, clipboard, or network access.

## 2. High-Level Architecture

```text
CliporaxPlugins/plugins/
└── com.example.myplugin/
    ├── manifest.json
    ├── main.js
    ├── assets/
    └── src/

app-data/plugins/
└── com.example.myplugin/
    ├── manifest.json
    ├── main.js
    └── assets/

src-tauri/src/plugin/
├── manifest.rs
├── types.rs
├── commands.rs
├── lifecycle/
│   ├── registry.rs
│   └── state.rs
└── permission/
    ├── checker.rs
    └── definition.rs

src/plugin/
├── components/
├── context/
├── extensions/
└── types/
```

The backend owns plugin discovery, manifest parsing, lifecycle state, permission definitions, and IPC commands. The frontend owns plugin management UI, extension rendering, and user-facing permission prompts.

## 3. Plugin Lifecycle

Plugins move through a small set of explicit states:

| State | Meaning |
| --- | --- |
| `Discovered` | The package exists under the app data `plugins/` directory. |
| `Validated` | The manifest was parsed and passed validation. |
| `Loaded` | Metadata and script content are available to the runtime. |
| `PendingPermission` | Required permissions have not been granted yet. |
| `Active` | The plugin is enabled and its extensions may run. |
| `Inactive` | The plugin is installed but disabled. |
| `Unloaded` | Runtime state has been released. |

Lifecycle operations are exposed through Tauri commands. The frontend should call the typed wrappers in `src/plugin/api/pluginApi.ts` instead of raw `invoke` calls.

## 4. Manifest Format

Each plugin package must include a `manifest.json`.

```json
{
  "id": "com.example.ocr",
  "name": "OCR Text Recognition",
  "version": "1.0.0",
  "description": "Extracts text from image clipboard items.",
  "author": {
    "name": "Example Developer",
    "email": "dev@example.com"
  },
  "type": "hybrid",
  "main": "main.js",
  "permissions": [
    {
      "permission": "data:read",
      "reason": "Read image clipboard data for OCR."
    },
    {
      "permission": "data:write",
      "reason": "Save OCR results back to clipboard history.",
      "required": false
    }
  ],
  "extensions": [
    {
      "id": "ocr-card-action",
      "type": "card_action",
      "label": "OCR",
      "icon": "scan-text"
    }
  ],
  "configSchema": {
    "type": "object",
    "properties": {
      "ocr_language": {
        "type": "string",
        "label": "OCR Language",
        "default": "eng"
      }
    }
  },
  "minAppVersion": "1.0.0",
  "compatibility": {
    "platforms": ["linux", "macos", "windows"]
  }
}
```

### Manifest Fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | string | yes | Reverse-domain plugin ID, such as `com.example.plugin`. |
| `name` | string | yes | Display name. |
| `version` | string | yes | Semantic version. |
| `description` | string | yes | User-facing plugin description. |
| `author` | object | yes | Author metadata: `name`, optional `email`, optional `url`. |
| `type` | string | yes | Plugin type: `source`, `transform`, `sink`, `router`, or `hybrid`. |
| `main` | string | no | Entry script, default `main.js`. |
| `permissions` | array | yes | Permission requests with reasons. |
| `extensions` | array | no | Declared UI extension points. |
| `configSchema` | object | no | JSON-schema-like configuration fields. |
| `minAppVersion` | string | no | Minimum supported Cliporax version. |
| `compatibility` | object | no | Platform and compatibility metadata. |

## 5. Permission Model

Plugins declare all requested permissions in their manifest. The backend validates the requested permission IDs against built-in definitions and tracks grants in persistent plugin state.

### Built-In Permissions

| Permission | Category | Risk | Description | Implies |
| --- | --- | --- | --- | --- |
| `ui:card-action` | UI | Low | Add action buttons to clipboard cards. | - |
| `ui:settings-panel` | UI | Low | Add a settings panel. | - |
| `ui:context-menu` | UI | Low | Add context menu entries. | - |
| `data:read` | Data | Low | Read clipboard history. | - |
| `data:write` | Data | High | Modify or add clipboard content. | `data:read` |
| `data:delete` | Data | Dangerous | Delete clipboard records. | `data:read` |
| `data:sensitive` | Data | Dangerous | Access sensitive clipboard data. | `data:read` |
| `data:transform` | Data | Medium | Transform clipboard data. | `data:read` |
| `system:storage` | System | Low | Use plugin-local storage. | - |
| `system:clipboard-read` | System | Medium | Read the system clipboard. | `data:read` |
| `system:clipboard-write` | System | High | Write to the system clipboard. | `system:clipboard-read`, `data:write` |
| `system:process` | System | Dangerous | Execute external processes. | - |
| `system:file-read` | System | High | Read local files. | - |
| `system:file-write` | System | Dangerous | Write local files. | `system:file-read` |
| `network:fetch` | Network | High | Make HTTP requests. | - |
| `network:websocket` | Network | High | Open WebSocket connections. | `network:fetch` |
| `network:sync` | Network | Dangerous | Sync clipboard data to a remote service. | `network:fetch`, `data:read`, `data:write` |
| `network:localhost` | Network | Medium | Access localhost services. | - |

### Risk Levels

| Level | Meaning | Handling |
| --- | --- | --- |
| `Low` | Low risk | May be granted automatically. |
| `Medium` | Moderate risk | Prompt on first use. |
| `High` | High risk | Requires explicit user confirmation. |
| `Dangerous` | Sensitive or destructive | Requires explicit grant and additional confirmation. |

### Permission Request Shape

```ts
interface PermissionRequest {
  permission: string;
  reason: string;
  required?: boolean;
}
```

Example:

```json
{
  "permissions": [
    {
      "permission": "data:read",
      "reason": "Read clipboard text to generate QR codes.",
      "required": true
    },
    {
      "permission": "network:fetch",
      "reason": "Fetch data from a remote QR generation service.",
      "required": false
    }
  ]
}
```

## 6. Plugin API Surface

The frontend plugin runtime exposes a limited API object to active plugins. API access should map back to permission checks where appropriate.

```ts
interface CliporaxPluginApi {
  plugin: {
    id: string;
    name: string;
    version: string;
  };
  clipboard: ClipboardApi;
  storage: StorageApi;
  network: NetworkApi;
  events: EventApi;
  logger: LoggerApi;
  config: ConfigApi;
}
```

Example subsystem shapes:

```ts
interface ClipboardApi {
  list(options?: { limit?: number; tabId?: number }): Promise<unknown[]>;
  get(id: string): Promise<unknown | null>;
  writeText(text: string): Promise<void>;
}

interface StorageApi {
  get<T>(key: string): Promise<T | null>;
  set<T>(key: string, value: T): Promise<void>;
  remove(key: string): Promise<void>;
}

interface NetworkApi {
  fetch(input: RequestInfo | URL, init?: RequestInit & { timeout?: number }): Promise<Response>;
}

interface EventApi {
  on(event: string, handler: (payload: unknown) => void): () => void;
  emit(event: string, payload?: unknown): void;
}

interface LoggerApi {
  debug(message: string, context?: unknown): void;
  info(message: string, context?: unknown): void;
  warn(message: string, context?: unknown): void;
  error(message: string, context?: unknown): void;
}

interface ConfigApi {
  get<T>(): Promise<T>;
  update<T>(patch: Partial<T>): Promise<void>;
}
```

### Network requests from UI extensions

Use the `network` object included in an extension's render context rather than
depending on a global `fetch` binding. It permits requests only when the active
plugin has `network:fetch` or an implied network permission.

```js
function render(props) {
  const button = document.createElement("button");
  button.textContent = "Fetch";
  button.onclick = async () => {
    const response = await props.context.network.fetch(
      "https://example.com/api/data",
      { headers: { Accept: "application/json" }, timeout: 10_000 },
    );
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
    const result = await response.json();
    // render result
  };
  return button;
}
```

The method returns the standard Web `Response`; HTTP error statuses are not
thrown. A timeout rejects with the standard abort error. The host does not log
request bodies, response bodies, or credentials.

### Shared UI controls

DOM-based extensions receive `context.ui.createCombobox`, which renders the
same non-native combobox as the host UI. It supports keyboard selection,
search, disabled options, and the active application theme. Do not create a
native `<select>` in a plugin when this control fits the interaction.

```js
const targetLanguage = props.context.ui.createCombobox({
  value: "en",
  options: [
    { value: "en", label: "English" },
    { value: "zh-CN", label: "Chinese (Simplified)" },
  ],
  searchable: true,
  onChange: (value) => saveTargetLanguage(value),
});

container.append(targetLanguage.element);
// Call targetLanguage.destroy() when the plugin view is removed.
```

## 7. IPC Commands

| Command | Description | Parameters | Returns |
| --- | --- | --- | --- |
| `plugin_get_all` | Get all plugins. | - | `PluginInfo[]` |
| `plugin_get_detail` | Get plugin details. | `pluginId: string` | `PluginDetail` |
| `plugin_load` | Load a plugin. | `pluginId: string` | `LoadResult` |
| `plugin_activate` | Activate a plugin. | `pluginId: string` | `void` |
| `plugin_deactivate` | Deactivate a plugin. | `pluginId: string` | `void` |
| `plugin_unload` | Unload a plugin. | `pluginId: string` | `void` |
| `plugin_grant_permission` | Grant a permission. | `pluginId, permission: string` | `void` |
| `plugin_get_config` | Get plugin config. | `pluginId: string` | `unknown` |
| `plugin_update_config` | Update plugin config. | `pluginId, config: unknown` | `void` |
| `plugin_discover` | Discover plugin directories. | - | `string[]` |
| `plugin_get_state` | Get plugin lifecycle state. | `pluginId: string` | `PluginState` |
| `plugin_read_script` | Read plugin entry script. | `pluginId: string` | `string` |
| `plugin_get_permission_definitions` | Get permission definitions. | - | `Permission[]` |

When adding or changing commands, update both the Rust command registration and the typed frontend wrappers in `src/lib/tauri-api.ts`.

## 8. Backend Architecture

Backend plugin code lives under `src-tauri/src/plugin/`.

```text
plugin/
├── mod.rs
├── types.rs
├── manifest.rs
├── commands.rs
├── lifecycle/
│   ├── registry.rs
│   └── state.rs
└── permission/
    ├── checker.rs
    └── definition.rs
```

Important backend responsibilities:

- Discover plugin directories.
- Parse and validate manifests.
- Track lifecycle state.
- Persist enabled state and permission grants.
- Validate permission IDs and implied permissions.
- Serve plugin metadata and scripts over IPC.
- Avoid logging secrets, credentials, or full clipboard content.

Persistent plugin state is stored in the app data plugin directory as `.plugin_state.json`. On startup, the app discovers plugin directories, reads saved state, restores enabled plugins, restores granted permissions, and loads active plugins.

## 9. Frontend Integration

The frontend integrates plugins through an extension manager and context provider. Active plugins can contribute UI at known extension points.

Example extension rendering:

```tsx
function ClipboardCardActions({ item }: { item: ClipboardItem }) {
  const extensions = usePluginExtensions("card_action");

  return (
    <div className="card-actions">
      <button onClick={handleCopy}>Copy</button>
      <button onClick={handleDelete}>Delete</button>
      {extensions.map((extension) => (
        <PluginActionButton
          key={extension.id}
          extension={extension}
          item={item}
        />
      ))}
    </div>
  );
}
```

Frontend responsibilities:

- Show installed plugins and lifecycle state.
- Prompt users for permission grants.
- Render settings panels and config fields.
- Load active plugin scripts through backend IPC.
- Keep extension rendering isolated from core UI behavior.
- Handle plugin errors without taking down the main app.

## 10. Example Plugin

```ts
/**
 * Complete Cliporax plugin example.
 */
const plugin = {
  id: "com.example.qrcode",
  name: "QR Code Generator",

  async activate(api: CliporaxPluginApi) {
    api.logger.info("QR Code plugin activated");

    api.events.on("clipboard:item-selected", async (item) => {
      api.logger.debug("Selected item changed", { itemId: item.id });
    });
  },

  async deactivate(api: CliporaxPluginApi) {
    api.logger.info("QR Code plugin deactivated");
  },

  extensions: {
    card_action: [
      {
        id: "generate-qr",
        label: "QR",
        icon: "qr-code",
        async onClick(item, api) {
          const qrData = await generateQrCode(item.content);
          showQrModal(qrData);
        },
      },
    ],
  },
};

registerPlugin(plugin);
```

## 11. Implementation Roadmap

| Phase | Scope | Status |
| --- | --- | --- |
| Phase 1 | Foundation: manifest, registry, state | Complete |
| Phase 2 | Permission system: checker, risk levels | Complete |
| Phase 3 | IPC communication: commands and API wrappers | Complete |
| Phase 4 | Extension points: card, settings, sidebar | Complete |
| Phase 5 | Frontend integration: extension manager and plugin context | Complete |
| Phase 6 | Script loading through IPC | Complete |
| Phase 7 | Persistent state: auto-save and restore | Complete |
| Phase 8 | Plugin marketplace UI | Planned |
| Phase 9 | Developer tools: CLI and debugging | Planned |
| Phase 10 | GitHub integration and update flow | Planned |

## 12. Naming Conventions

- **Plugin ID**: reverse-domain format, such as `com.example.plugin-name`.
- **Extension ID**: `{pluginId}.{extensionName}`, such as `com.example.ocr.card-badge`.
- **Config key**: lower snake case, such as `ocr_language`.
- **Event name**: lower-case colon-separated form, such as `clipboard:change` or `sync:status`.
- **Permission ID**: `namespace:action`, such as `data:read` or `network:fetch`.

## 13. References

- [Tauri plugin development](https://tauri.app/v2/guides/plugins/)
- [Obsidian plugin API](https://docs.obsidian.md/Plugins/Getting+started/Plugin+anatomy)
- [VS Code extension API](https://code.visualstudio.com/api)
