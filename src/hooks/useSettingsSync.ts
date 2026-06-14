import { useCallback, useEffect, useRef } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { createLogger } from "../utils/logger";
import {
  backendToFrontendSettings,
  GeneralSettings,
  ShortcutSettings,
} from "../components/Settings";
import { AppSettings, settings as settingsApi } from "../lib/tauri-api";

const logger = createLogger("useSettingsSync");

interface UseSettingsSyncOptions {
  onRemoteSettingsChange?: (
    general: GeneralSettings,
    shortcuts: ShortcutSettings,
  ) => void;
  enabled?: boolean;
}

let globalChangeId = 0;

export function getNextChangeId(): number {
  return ++globalChangeId;
}

export function useSettingsSync({
  onRemoteSettingsChange,
  enabled = true,
}: UseSettingsSyncOptions = {}) {
  const lastLocalChangeIdRef = useRef(0);
  const lastProcessedRemoteRef = useRef<string>("");

  const markAsLocalChange = useCallback(() => {
    lastLocalChangeIdRef.current = getNextChangeId();
    logger.debug("Marked as local change, id:", lastLocalChangeIdRef.current);
  }, []);

  useEffect(() => {
    if (!enabled) return;

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    const applyBackendSettings = (
      backendSettings: AppSettings,
      source: "initial" | "changed",
    ) => {
      if (cancelled) return;

      const frontendSettings = backendToFrontendSettings(backendSettings);
      const snapshot = JSON.stringify(frontendSettings);

      logger.info("[EVENT] settings received:", {
        source,
        lineHeight: frontendSettings.general.lineHeight,
        shortcut: frontendSettings.shortcuts.toggleWindow,
        isDuplicate: snapshot === lastProcessedRemoteRef.current,
      });

      if (snapshot === lastProcessedRemoteRef.current) {
        return;
      }

      lastProcessedRemoteRef.current = snapshot;
      onRemoteSettingsChange?.(
        frontendSettings.general,
        frontendSettings.shortcuts,
      );
    };

    const setupListener = async () => {
      try {
        unlisten = await listen<AppSettings>("settings:changed", (event) => {
          applyBackendSettings(event.payload, "changed");
        });
        logger.info("settings:changed listener registered");

        const initialSettings = await settingsApi.getAll();
        applyBackendSettings(initialSettings, "initial");
      } catch (error) {
        logger.error("Failed to setup settings sync:", error);
      }
    };

    setupListener();

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
        logger.info("settings:changed listener removed");
      }
    };
  }, [enabled, onRemoteSettingsChange]);

  return {
    markAsLocalChange,
  };
}

export default useSettingsSync;
