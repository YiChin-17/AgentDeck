import {
  useState,
  useEffect,
  useCallback,
  useSyncExternalStore,
} from "react";
import * as api from "../lib/tauri";

export type Theme = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "theme";

function getSystemTheme(): ResolvedTheme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

function subscribeSystemTheme(onChange: () => void) {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}

function applyThemeClass(resolved: ResolvedTheme) {
  const root = document.documentElement;
  if (resolved === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
}

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark" || stored === "system")
      return stored;
    // Light is the product default; a stored or backend preference still wins.
    return "light";
  });

  // 訂閱系統外觀而非只讀一次，系統切換時才會重繪，讓 resolvedTheme 的消費端拿到新值
  const systemTheme = useSyncExternalStore(subscribeSystemTheme, getSystemTheme);

  const resolvedTheme: ResolvedTheme =
    theme === "system" ? systemTheme : theme;

  // Apply class on mount and theme change
  useEffect(() => {
    applyThemeClass(resolvedTheme);
  }, [resolvedTheme]);

  // Load from Tauri settings on mount
  useEffect(() => {
    api.getSettings("theme").then((v) => {
      if (v === "light" || v === "dark" || v === "system") {
        setThemeState(v);
        localStorage.setItem(STORAGE_KEY, v);
      }
    });
  }, []);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    localStorage.setItem(STORAGE_KEY, next);
    api.setSettings("theme", next);
  }, []);

  return { theme, setTheme, resolvedTheme };
}
