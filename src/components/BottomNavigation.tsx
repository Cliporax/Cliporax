import { useEffect } from "react";
import { Clipboard, ListTodo, Puzzle } from "lucide-react";
import { useTabStore } from "../stores/tabStore";
import { useUIStore } from "../stores/uiStore";
import { useContentTabExtensions } from "../plugin/extensions";

const OPEN_FILE_SYNC_EVENT = "cliporax:open-file-sync";
const FILE_SYNC_TAB_ID = "plugin:com.cliporax.file-sync:FileSyncView";

function PluginTabIcon({ icon, iconDataUrl }: { icon?: string; iconDataUrl?: string }) {
  if (iconDataUrl) {
    return <img src={iconDataUrl} alt="" aria-hidden="true" className="h-3.5 w-3.5 object-contain" />;
  }
  if (icon === "list-todo") return <ListTodo size={14} aria-hidden="true" />;
  return <Puzzle size={14} aria-hidden="true" />;
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
    <nav className="flex h-11 shrink-0 items-stretch gap-1 border-t border-gray-200 bg-white px-2 dark:border-gray-700 dark:bg-gray-800" aria-label="Main navigation">
      <button
        type="button"
        onClick={openClipboard}
        aria-current={activePluginTabId === null ? "page" : undefined}
        className={`relative flex min-w-16 flex-col items-center justify-center gap-px rounded-md px-2 text-[9px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-indigo-500 ${activePluginTabId === null ? "bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300" : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700"}`}
      >
        {activePluginTabId === null ? (
          <span aria-hidden="true" className="absolute inset-x-2 top-0 h-0.5 rounded-full bg-indigo-500 dark:bg-indigo-400" />
        ) : null}
        <Clipboard size={14} aria-hidden="true" />
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
            className={`relative flex min-w-16 flex-col items-center justify-center gap-px rounded-md px-2 text-[9px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-indigo-500 ${active ? "bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300" : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700"}`}
          >
            {active ? (
              <span aria-hidden="true" className="absolute inset-x-2 top-0 h-0.5 rounded-full bg-indigo-500 dark:bg-indigo-400" />
            ) : null}
            <PluginTabIcon icon={tab.icon} iconDataUrl={tab.iconDataUrl} />
            <span className="max-w-16 truncate">{tab.title}</span>
          </button>
        );
      })}
    </nav>
  );
}
