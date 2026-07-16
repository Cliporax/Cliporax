import { useEffect } from "react";
import { Clipboard, ListTodo, Puzzle } from "lucide-react";
import { useTabStore } from "../stores/tabStore";
import { useUIStore } from "../stores/uiStore";
import { useContentTabExtensions } from "../plugin/extensions";

const OPEN_FILE_SYNC_EVENT = "cliporax:open-file-sync";
const FILE_SYNC_TAB_ID = "plugin:com.cliporax.file-sync:FileSyncView";

function PluginTabIcon({ icon, iconDataUrl }: { icon?: string; iconDataUrl?: string }) {
  if (iconDataUrl) {
    return <img src={iconDataUrl} alt="" aria-hidden="true" className="h-4 w-4 object-contain" />;
  }
  if (icon === "list-todo") return <ListTodo size={16} aria-hidden="true" />;
  return <Puzzle size={16} aria-hidden="true" />;
}

export function BottomNavigation() {
  const { activePluginTabId, setActivePluginTab } = useTabStore();
  const { setSearchQuery } = useUIStore();
  const pluginTabs = useContentTabExtensions();

  useEffect(() => {
    if (activePluginTabId && !pluginTabs.some((tab) => tab.id === activePluginTabId)) {
      setActivePluginTab(null);
    }
  }, [activePluginTabId, pluginTabs, setActivePluginTab]);

  useEffect(() => {
    const openFileSync = () => {
      if (!pluginTabs.some((tab) => tab.id === FILE_SYNC_TAB_ID)) return;
      setActivePluginTab(FILE_SYNC_TAB_ID);
      setSearchQuery("");
    };
    window.addEventListener(OPEN_FILE_SYNC_EVENT, openFileSync);
    return () => window.removeEventListener(OPEN_FILE_SYNC_EVENT, openFileSync);
  }, [pluginTabs, setActivePluginTab, setSearchQuery]);

  const openClipboard = () => {
    setActivePluginTab(null);
    setSearchQuery("");
  };

  return (
    <nav className="flex h-12 shrink-0 items-stretch border-t border-gray-200 bg-white px-2 dark:border-gray-700 dark:bg-gray-800" aria-label="Main navigation">
      <button
        type="button"
        onClick={openClipboard}
        aria-current={activePluginTabId === null ? "page" : undefined}
        className={`flex min-w-20 flex-col items-center justify-center gap-0.5 rounded-md px-3 text-[10px] font-medium transition-colors ${activePluginTabId === null ? "text-indigo-600 dark:text-indigo-400" : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700"}`}
      >
        <Clipboard size={16} aria-hidden="true" />
        <span>Clipboard</span>
      </button>
      {pluginTabs.map((tab) => {
        const active = tab.id === activePluginTabId;
        return (
          <button
            key={tab.id}
            type="button"
            onClick={() => {
              setActivePluginTab(tab.id);
              setSearchQuery("");
            }}
            aria-current={active ? "page" : undefined}
            className={`flex min-w-20 flex-col items-center justify-center gap-0.5 rounded-md px-3 text-[10px] font-medium transition-colors ${active ? "text-indigo-600 dark:text-indigo-400" : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700"}`}
          >
            <PluginTabIcon icon={tab.icon} iconDataUrl={tab.iconDataUrl} />
            <span className="max-w-20 truncate">{tab.title}</span>
          </button>
        );
      })}
    </nav>
  );
}
