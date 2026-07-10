/**
 * Extension Point Type Definitions
 */

import type React from "react";

/**
 * React component type
 */
export type ReactComponent<T = {}> = React.ComponentType<T>;

/**
 * Extension point type
 */
export type ExtensionPointType =
  | "settings-panel"
  | "card"
  | "sidebar"
  | "preview"
  | "context-menu"
  | "toolbar"
  | "content-tab"
  | `custom:${string}`;

/**
 * Extension point definition
 */
export interface ExtensionPointDefinition<T = unknown> {
  /** Extension point ID */
  id: string;

  /** Extension point type */
  type: ExtensionPointType;

  /** Render component */
  component: ReactComponent<ExtensionProps<T>>;

  /** Data provider */
  dataProvider?: (context: ExtensionContext) => T | Promise<T>;

  /** Filter function */
  filter?: (
    extension: RegisteredExtension,
    context: ExtensionContext,
  ) => boolean;

  /** Sort function */
  sort?: (a: RegisteredExtension, b: RegisteredExtension) => number;
}

/**
 * Extension props
 */
export interface ExtensionProps<T = unknown> {
  /** Extension data */
  data: T;

  /** Extension configuration */
  config: Record<string, unknown>;

  /** Context information */
  context: ExtensionContext;

  /** Update callback */
  onUpdate?: (data: T) => void;
}

/**
 * Extension context
 */
export interface ExtensionContext {
  /** Current theme */
  theme: "light" | "dark";

  /** Current selected clipboard item */
  selectedItem?: unknown;

  /** Application settings */
  settings: Record<string, unknown>;

  /** Plugin instance */
  plugin: {
    id: string;
    name: string;
    version: string;
  };
}

/**
 * Registered extension
 */
export interface RegisteredExtension {
  /** Extension ID */
  id: string;

  /** Plugin ID */
  pluginId: string;

  /** Extension point ID */
  pointId: string;

  /** Component path */
  component: string;

  /** Optional icon name */
  icon?: string;

  /** Configuration */
  config: Record<string, unknown>;

  /** Priority */
  priority: number;

  /** Condition expression */
  condition?: string;
}

/**
 * Extension registry interface
 */
export interface ExtensionRegistry {
  /** Register extension point */
  registerPoint(definition: ExtensionPointDefinition): void;

  /** Unregister extension point */
  unregisterPoint(pointId: string): void;

  /** Register extension */
  registerExtension(extension: RegisteredExtension): void;

  /** Unregister extension */
  unregisterExtension(extensionId: string): void;

  /** Get extensions for a point */
  getExtensions(pointId: string): RegisteredExtension[];

  /** Render extension point */
  renderPoint(pointId: string, context: ExtensionContext): React.ReactNode;
}

/**
 * Card extension specific types
 */
export interface CardExtensionData {
  /** Card item */
  item: unknown;

  /** Position */
  position: "badge" | "action" | "header" | "footer";
}

export interface CardMatcher {
  type?: string | string[];
  mimeType?: string | RegExp;
  custom?: (item: unknown) => boolean;
}

/**
 * Settings panel extension specific types
 */
export interface SettingsPanelData {
  /** Tab ID */
  tabId: string;

  /** Tab title */
  title: string;

  /** Tab icon */
  icon?: string;

  /** Order */
  order?: number;
}

/**
 * Sidebar extension specific types
 */
export interface SidebarData {
  /** Panel ID */
  panelId: string;

  /** Panel title */
  title: string;

  /** Panel icon */
  icon: string;

  /** Position */
  position: "left" | "right" | "bottom";
}

export interface ContentTabData {
  /** Stable navigation ID owned by the host */
  tabId: string;
  /** User-visible title */
  title: string;
  /** Optional icon name */
  icon?: string;
  /** Sort order */
  order?: number;
}
