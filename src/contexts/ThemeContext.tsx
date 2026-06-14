import React, {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  useRef,
} from "react";
import { createLogger } from "../utils/logger";

const logger = createLogger("Theme");

type Theme = "light" | "dark" | "system";

interface ThemeContextType {
  theme: Theme;
  resolvedTheme: "light" | "dark";
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

const STORAGE_KEY = "cliporax-theme";

// Cross-window theme sync event name
const THEME_EVENT = "theme:changed";

function resolveThemeValue(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return theme;
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => {
    if (typeof window !== "undefined") {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored === "light" || stored === "dark" || stored === "system") {
        return stored;
      }
    }
    return "system";
  });

  const [resolvedTheme, setResolvedTheme] = useState<"light" | "dark">(() => {
    if (typeof window !== "undefined") {
      const stored = localStorage.getItem(STORAGE_KEY) as Theme | null;
      if (stored) return resolveThemeValue(stored);
    }
    return "dark";
  });

  // Marks whether this change was initiated by this window to avoid duplicate storage-event handling
  const isLocalChange = useRef(false);

  useEffect(() => {
    setResolvedTheme(resolveThemeValue(theme));

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => {
      if (theme === "system") {
        setResolvedTheme(resolveThemeValue(theme));
      }
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [theme]);

  useEffect(() => {
    const root = document.documentElement;
    const body = document.body;
    logger.info("Theme applied:", resolvedTheme);
    if (resolvedTheme === "dark") {
      root.classList.add("dark");
      body.classList.add("dark");
    } else {
      root.classList.remove("dark");
      body.classList.remove("dark");
    }
  }, [resolvedTheme]);

  const setTheme = useCallback((newTheme: Theme) => {
    logger.info("Theme set to:", newTheme);
    isLocalChange.current = true;
    setThemeState(newTheme);
    localStorage.setItem(STORAGE_KEY, newTheme);

    // Sync to the Rust backend, which broadcasts theme:changed to all windows
    try {
      import("../lib/tauri-api")
        .then(({ settings }) => {
          settings.update({ theme: newTheme }).catch((e: unknown) => {
            logger.error("[Theme] Failed to sync theme to backend:", e);
          });
        })
        .catch(() => {});
    } catch {
      // Ignore
    }
  }, []);

  const toggleTheme = useCallback(() => {
    const newTheme: Theme = resolvedTheme === "dark" ? "light" : "dark";
    setTheme(newTheme);
    logger.info("Theme toggled:", resolvedTheme, "→", newTheme);
  }, [resolvedTheme, setTheme]);

  // Listen for cross-window theme changes - method 1: Tauri events
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setupListener = async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<Theme>(THEME_EVENT, (event) => {
          const newTheme = event.payload;
          // Skip events emitted by this window
          if (isLocalChange.current) {
            isLocalChange.current = false;
            return;
          }
          logger.info(
            "[Theme] Received theme change via Tauri event:",
            newTheme,
          );
          setThemeState(newTheme);
        });
        logger.debug("[Theme] Tauri event listener registered");
      } catch {
        // Ignore in non-Tauri environments
      }
    };

    setupListener();
    return () => {
      unlisten?.();
    };
  }, []);

  // Listen for cross-window theme changes - method 2: storage events, the most reliable native browser option
  // When another same-origin window modifies localStorage, the current window receives a storage event
  useEffect(() => {
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key !== STORAGE_KEY) return;
      const newTheme = e.newValue as Theme | null;
      if (!newTheme) return;
      if (newTheme !== "light" && newTheme !== "dark" && newTheme !== "system")
        return;

      // Storage events fire only for changes from other windows, not this one
      logger.info("[Theme] Received theme change via storage event:", newTheme);
      setThemeState(newTheme);
    };

    window.addEventListener("storage", handleStorageChange);
    return () => window.removeEventListener("storage", handleStorageChange);
  }, []);

  return (
    <ThemeContext.Provider
      value={{ theme, resolvedTheme, setTheme, toggleTheme }}
    >
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (context === undefined) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
