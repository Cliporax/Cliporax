/**
 * Plugin System Module
 *
 * This module provides the frontend implementation for the Cliporax plugin system.
 */

export * from "./types";
export * from "./api/pluginApi";
export { PluginProvider, usePlugin } from "./context/PluginContext";
export { PluginList } from "./components/PluginList";
export { PluginCard } from "./components/PluginCard";
export { PluginDetailPanel } from "./components/PluginDetail";
export { PluginDetailModal } from "./components/PluginDetailModal";
export { PermissionPrompt } from "./components/PermissionPrompt";
export {
  Combobox,
  createCombobox,
  type ComboboxInstance,
  type ComboboxOption,
  type ComboboxOptions,
  type ComboboxProps,
} from "../components/Combobox";
export {
  ExtensionManagerProvider,
  PluginContentTab,
  PluginSidebarExtensions,
  useExtensionManager,
  useCardExtensions,
  useContentTabExtensions,
} from "./extensions";
