/**
 * Plugin Type Definitions
 */

import type { ClipPacket } from "./packet";
import type { PermissionRequest, Permission } from "./permission";

/**
 * Plugin type enumeration
 */
export type PluginType = "source" | "transform" | "sink" | "router" | "hybrid";

/**
 * Plugin state enumeration
 */
export type PluginState =
  | "discovered"
  | "validated"
  | "loaded"
  | "pending-permission"
  | "active"
  | "inactive"
  | "unloaded"
  | { error: string };

/**
 * Author information
 */
export interface AuthorInfo {
  name: string;
  email?: string;
  url?: string;
}

/**
 * Extension point declaration
 */
export interface ExtensionDeclaration {
  point: string;
  component: string;
  condition?: string;
  priority?: number;
}

/**
 * Configuration schema field types
 */
export type ConfigFieldType =
  | "text"
  | "number"
  | "select"
  | "boolean"
  | "shortcut";

/**
 * Configuration field definition
 */
export interface ConfigField {
  /** Field key */
  key: string;
  /** Field type */
  type: ConfigFieldType;
  /** Display label */
  label: string;
  /** Description */
  description?: string;
  /** Default value */
  default?: any;
  /** Options for select type */
  options?: { value: string; label: string }[];
  /** Minimum value for number type */
  min?: number;
  /** Maximum value for number type */
  max?: number;
  /** Whether shortcut is global (true) or app-level (false) */
  global?: boolean;
}

/**
 * Configuration schema
 */
export interface ConfigSchema {
  /** Configuration fields */
  fields?: ConfigField[];
  /** Default configuration values */
  default?: Record<string, unknown>;
}

/**
 * Compatibility information
 */
export interface CompatibilityInfo {
  maxAppVersion?: string;
  platforms?: Platform[];
}

/**
 * Platform type
 */
export type Platform = "windows" | "linux" | "macos";

/**
 * Plugin manifest - defines plugin metadata and configuration
 */
export interface PluginManifest {
  /** Plugin unique identifier (reverse domain format) */
  id: string;

  /** Plugin display name */
  name: string;

  /** Version number (semantic versioning) */
  version: string;

  /** Plugin description */
  description: string;

  /** Author information */
  author: AuthorInfo;

  /** Main entry file */
  main?: string;

  /** Plugin type */
  type: PluginType;

  /** Requested permissions */
  permissions: PermissionRequest[];

  /** Extension point declarations */
  extensions?: ExtensionDeclaration[];

  /** Configuration schema */
  configSchema?: ConfigSchema;

  /** Compatibility requirements */
  compatibility?: CompatibilityInfo;

  /** Icon path */
  icon?: string;

  /** Keywords for search */
  keywords?: string[];

  /** Homepage URL */
  homepage?: string;

  /** Repository URL */
  repository?: string;

  /** License */
  license?: string;

  /** Minimum application version */
  minAppVersion?: string;
}

/**
 * Plugin statistics
 */
export interface PluginStatistics {
  activatedCount: number;
  totalRuntimeMs: number;
  lastActivated?: string;
  errorCount: number;
}

/**
 * Plugin instance information
 */
export interface PluginInstance {
  id: string;
  manifest: PluginManifest;
  state: PluginState;
  grantedPermissions: string[];
  config: unknown;
  statistics: PluginStatistics;
}

/**
 * Plugin info for list display
 */
export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  icon?: string;
  state: PluginState;
  permissions: PermissionRequest[];
  type: PluginType;
}

/**
 * Plugin detail for detailed view
 */
export interface PluginDetail {
  manifest: PluginManifest;
  state: PluginState;
  grantedPermissions: string[];
  config: unknown;
  statistics: PluginStatistics;
}

/**
 * Load result
 */
export type LoadResult =
  | { success: true }
  | { permissionRequired: PermissionRequest[] };

/**
 * Plugin context API - exposed to plugin runtime
 */
export interface PluginContext {
  plugin: PluginInfo;
  clipboard: ClipboardAPI;
  storage: StorageAPI;
  ui: UIAPI;
  network: NetworkAPI;
  events: EventAPI;
  logger: LoggerAPI;
  config: ConfigAPI;
}

/**
 * Clipboard API
 */
export interface ClipboardAPI {
  read(): Promise<ClipPacket | null>;
  write(packet: ClipPacket): Promise<void>;
  getHistory(options?: HistoryOptions): Promise<ClipPacket[]>;
  search(query: string, options?: SearchOptions): Promise<ClipPacket[]>;
  onChange(callback: (packet: ClipPacket) => void): () => void;
}

export interface HistoryOptions {
  limit?: number;
  offset?: number;
  type?: string;
}

export interface SearchOptions {
  fuzzy?: boolean;
  regex?: boolean;
  caseSensitive?: boolean;
}

/**
 * Storage API
 */
export interface StorageAPI {
  get<T = unknown>(key: string): Promise<T | null>;
  set<T>(key: string, value: T): Promise<void>;
  delete(key: string): Promise<void>;
  clear(): Promise<void>;
  keys(): Promise<string[]>;
}

/**
 * UI API
 */
export interface UIAPI {
  notify(message: string, options?: NotifyOptions): Promise<void>;
  showDialog(options: DialogOptions): Promise<DialogResult>;
  registerCommand(command: CommandDefinition): Promise<void>;
  updateExtension(point: string, data: unknown): Promise<void>;
}

export interface NotifyOptions {
  type?: "info" | "success" | "warning" | "error";
  duration?: number;
}

export interface DialogOptions {
  title: string;
  content: string;
  actions?: DialogAction[];
}

export interface DialogAction {
  label: string;
  style?: "default" | "primary" | "danger";
}

export interface DialogResult {
  action: string;
  data?: unknown;
}

export interface CommandDefinition {
  id: string;
  label: string;
  shortcut?: string;
  handler: () => void | Promise<void>;
}

/**
 * Network API
 */
export interface NetworkAPI {
  fetch(url: string, options?: FetchOptions): Promise<Response>;
}

export interface FetchOptions {
  method?: "GET" | "POST" | "PUT" | "DELETE";
  headers?: Record<string, string>;
  body?: string;
  timeout?: number;
}

/**
 * Event API
 */
export interface EventAPI {
  emit(event: string, data?: unknown): void;
  on(event: string, callback: (data: unknown) => void): () => void;
  once(event: string, callback: (data: unknown) => void): () => void;
}

/**
 * Logger API
 */
export interface LoggerAPI {
  debug(message: string, ...args: unknown[]): void;
  info(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
  error(message: string, ...args: unknown[]): void;
}

/**
 * Config API
 */
export interface ConfigAPI {
  get(): Promise<Record<string, unknown>>;
  update(config: Record<string, unknown>): Promise<void>;
  onChange(callback: (config: Record<string, unknown>) => void): () => void;
}
