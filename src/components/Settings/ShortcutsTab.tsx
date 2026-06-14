import { useState, useEffect } from "react";
import { Keyboard } from "lucide-react";
import type { AppSettings } from "../../stores/settingsStore";

interface ShortcutsTabProps {
  settings: AppSettings;
  onUpdate: (updates: Partial<AppSettings>) => Promise<void>;
}

export function ShortcutsTab({ settings, onUpdate }: ShortcutsTabProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [recordedShortcut, setRecordedShortcut] = useState("");

  useEffect(() => {
    if (isRecording) {
      const handleKeyDown = (e: KeyboardEvent) => {
        e.preventDefault();
        e.stopPropagation();

        const keys: string[] = [];
        if (e.ctrlKey) keys.push("Ctrl");
        if (e.shiftKey) keys.push("Shift");
        if (e.altKey) keys.push("Alt");
        if (e.metaKey) keys.push("Meta");

        if (
          e.key !== "Control" &&
          e.key !== "Shift" &&
          e.key !== "Alt" &&
          e.key !== "Meta"
        ) {
          keys.push(e.key.toUpperCase());
        }

        const shortcut = keys.join("+");
        setRecordedShortcut(shortcut);

        if (keys.length >= 2) {
          onUpdate({ shortcut_toggle_window: shortcut });
          setIsRecording(false);
        }
      };

      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }
  }, [isRecording, onUpdate]);

  const startRecording = () => {
    setIsRecording(true);
    setRecordedShortcut("");
  };

  return (
    <div className="space-y-6">
      {/* Toggle Window Shortcut */}
      <div>
        <label className="block text-sm font-medium mb-2">Show/hide window</label>
        <div className="flex gap-3">
          <input
            type="text"
            value={
              isRecording
                ? recordedShortcut || "Press shortcut..."
                : settings.shortcut_toggle_window
            }
            readOnly
            className="flex-1 px-3 py-2 bg-gray-100 dark:bg-gray-700 rounded-lg"
            placeholder="Click to start recording"
          />
          <button
            onClick={startRecording}
            disabled={isRecording}
            className={`px-4 py-2 rounded-lg flex items-center gap-2 ${
              isRecording
                ? "bg-indigo-500 text-white animate-pulse"
                : "bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600"
            }`}
          >
            <Keyboard size={16} />
            {isRecording ? "Recording..." : "Record"}
          </button>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          Press a new shortcut combination to set it, for example Ctrl+Shift+V
        </p>
      </div>

      {/* Shortcut Info */}
      <div className="bg-gray-50 dark:bg-gray-800 rounded-lg p-4">
        <h4 className="text-sm font-medium mb-3">Shortcut reference</h4>
        <ul className="text-xs space-y-2 text-gray-600 dark:text-gray-300">
          <li>
            <kbd className="px-1 bg-gray-200 dark:bg-gray-700 rounded">
              Ctrl+F
            </kbd>{" "}
            - Show search bar
          </li>
          <li>
            <kbd className="px-1 bg-gray-200 dark:bg-gray-700 rounded">
              Ctrl+Enter
            </kbd>{" "}
            - Save edited content
          </li>
          <li>
            <kbd className="px-1 bg-gray-200 dark:bg-gray-700 rounded">Esc</kbd>{" "}
            - Close modal / cancel action
          </li>
          <li>
            <kbd className="px-1 bg-gray-200 dark:bg-gray-700 rounded">Del</kbd>{" "}
            - Delete selected item
          </li>
          <li>
            <kbd className="px-1 bg-gray-200 dark:bg-gray-700 rounded">
              Ctrl+Click
            </kbd>{" "}
            - Multi-select mode
          </li>
        </ul>
      </div>
    </div>
  );
}
