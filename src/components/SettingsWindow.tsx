/**
 * Settings Window Component
 * Used to display the settings page in a standalone window
 */

import React, { useState, useCallback } from "react";
import Settings, {
  loadSettings,
  GeneralSettings,
  ShortcutSettings,
} from "./Settings";
import { PluginProvider } from "../plugin";
import { createLogger } from "../utils/logger";
import { useSettingsSync } from "../hooks/useSettingsSync";

const logger = createLogger("SettingsWindow");

const SettingsWindow: React.FC = () => {
  const [settings, setSettings] = useState(loadSettings());

  // Local settings change callback triggered by Settings component actions
  // The button onClick already calls both syncSettingsToBackend and onSettingsChange
  // Only update local state here; no need to sync to the backend again
  const handleLocalSettingsChange = useCallback(
    (general: GeneralSettings, shortcuts: ShortcutSettings) => {
      logger.info(
        "[LOCAL] handleLocalSettingsChange called, lineHeight:",
        general.lineHeight,
      );
      setSettings({ general, shortcuts });
      // Note: do not call syncSettingsToBackend here because button onClick already called it
    },
    [],
  );

  // Remote settings change callback triggered by settings:changed events from other windows
  // Only update local state; do not sync to the backend again or it would create a loop
  const handleRemoteSettingsChange = useCallback(
    (general: GeneralSettings, shortcuts: ShortcutSettings) => {
      logger.info(
        "[REMOTE] handleRemoteSettingsChange called, lineHeight:",
        general.lineHeight,
      );
      logger.info(
        "[REMOTE] Updating local state only (NO syncSettingsToBackend call)",
      );
      setSettings({ general, shortcuts });
    },
    [],
  );

  // Sync settings across windows - listen for remote changes only
  useSettingsSync({
    onRemoteSettingsChange: handleRemoteSettingsChange,
    enabled: true,
  });

  return (
    <PluginProvider>
      <Settings
        isWindow={true}
        initialGeneralSettings={settings.general}
        initialShortcutSettings={settings.shortcuts}
        onSettingsChange={handleLocalSettingsChange}
      />
    </PluginProvider>
  );
};

export default SettingsWindow;
