import { useState, useEffect } from "react";
import { Monitor, Sun, Moon } from "lucide-react";
import type { AppSettings } from "../../stores/settingsStore";

interface GeneralTabProps {
  settings: AppSettings;
  onUpdate: (updates: Partial<AppSettings>) => Promise<void>;
}

export function GeneralTab({ settings, onUpdate }: GeneralTabProps) {
  const [localSettings, setLocalSettings] = useState(settings);

  useEffect(() => {
    setLocalSettings(settings);
  }, [settings]);

  const handleThemeChange = (theme: "light" | "dark" | "system") => {
    setLocalSettings({ ...localSettings, theme });
    onUpdate({ theme });
  };

  const handleMaxItemsChange = (max_items: number) => {
    setLocalSettings({ ...localSettings, max_items });
    onUpdate({ max_items });
  };

  const handleMaxImagesChange = (max_images: number) => {
    setLocalSettings({ ...localSettings, max_images });
    onUpdate({ max_images });
  };

  const handleLineHeightChange = (
    line_height: "small" | "medium" | "large",
  ) => {
    setLocalSettings({ ...localSettings, line_height });
    onUpdate({ line_height });
  };

  const toggleAutoStart = () => {
    const auto_start = !localSettings.auto_start;
    setLocalSettings({ ...localSettings, auto_start });
    onUpdate({ auto_start });
  };

  const toggleAutoHide = () => {
    const auto_hide = !localSettings.auto_hide;
    setLocalSettings({ ...localSettings, auto_hide });
    onUpdate({ auto_hide });
  };

  return (
    <div className="space-y-6">
      {/* Theme Selection */}
      <div>
        <label className="block text-sm font-medium mb-3">Theme</label>
        <div className="flex gap-3">
          <button
            onClick={() => handleThemeChange("light")}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${
              localSettings.theme === "light"
                ? "bg-indigo-500 text-white"
                : "bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600"
            }`}
          >
            <Sun size={16} />
            Light
          </button>
          <button
            onClick={() => handleThemeChange("dark")}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${
              localSettings.theme === "dark"
                ? "bg-indigo-500 text-white"
                : "bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600"
            }`}
          >
            <Moon size={16} />
            Dark
          </button>
          <button
            onClick={() => handleThemeChange("system")}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${
              localSettings.theme === "system"
                ? "bg-indigo-500 text-white"
                : "bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600"
            }`}
          >
            <Monitor size={16} />
            System
          </button>
        </div>
      </div>

      {/* Max Items */}
      <div>
        <label className="block text-sm font-medium mb-2">
          Maximum text items: {localSettings.max_items}
        </label>
        <input
          type="range"
          min="100"
          max="5000"
          step="100"
          value={localSettings.max_items}
          onChange={(e) => handleMaxItemsChange(Number(e.target.value))}
          className="w-full"
        />
      </div>

      {/* Max Images */}
      <div>
        <label className="block text-sm font-medium mb-2">
          Maximum image items: {localSettings.max_images}
        </label>
        <input
          type="range"
          min="50"
          max="2000"
          step="50"
          value={localSettings.max_images}
          onChange={(e) => handleMaxImagesChange(Number(e.target.value))}
          className="w-full"
        />
      </div>

      {/* Line Height */}
      <div>
        <label className="block text-sm font-medium mb-3">Line height</label>
        <div className="flex gap-3">
          {(["small", "medium", "large"] as const).map((height) => (
            <button
              key={height}
              onClick={() => handleLineHeightChange(height)}
              className={`px-4 py-2 rounded-lg ${
                localSettings.line_height === height
                  ? "bg-indigo-500 text-white"
                  : "bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600"
              }`}
            >
              {height === "small"
                ? "Compact"
                : height === "medium"
                  ? "Standard"
                  : "Relaxed"}
            </button>
          ))}
        </div>
      </div>

      {/* Auto Start */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium">Launch at startup</label>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            The app will start automatically when the system starts
          </p>
        </div>
        <button
          onClick={toggleAutoStart}
          className={`relative w-12 h-6 rounded-full transition-colors ${
            localSettings.auto_start
              ? "bg-indigo-500"
              : "bg-gray-300 dark:bg-gray-600"
          }`}
        >
          <span
            className={`absolute top-1 left-1 w-4 h-4 bg-white rounded-full transition-transform ${
              localSettings.auto_start ? "translate-x-6" : "translate-x-0"
            }`}
          />
        </button>
      </div>

      {/* Auto Hide */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium">Auto-hide</label>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            Automatically hide the window when it loses focus
          </p>
        </div>
        <button
          onClick={toggleAutoHide}
          className={`relative w-12 h-6 rounded-full transition-colors ${
            localSettings.auto_hide
              ? "bg-indigo-500"
              : "bg-gray-300 dark:bg-gray-600"
          }`}
        >
          <span
            className={`absolute top-1 left-1 w-4 h-4 bg-white rounded-full transition-transform ${
              localSettings.auto_hide ? "translate-x-6" : "translate-x-0"
            }`}
          />
        </button>
      </div>
    </div>
  );
}
