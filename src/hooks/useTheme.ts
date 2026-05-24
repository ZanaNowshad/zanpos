import { useCallback, useEffect, useState } from "react";

export type Theme =
  | "dark"
  | "light"
  | "midnight"
  | "forest"
  | "rose"
  | "ocean"
  | "slate";

export interface ThemeMeta {
  id: Theme;
  label: string;
  icon: string;
}

export const THEMES: ThemeMeta[] = [
  { id: "dark",     label: "Dark",     icon: "🌑" },
  { id: "light",    label: "Light",    icon: "☀"  },
  { id: "midnight", label: "Midnight", icon: "✦"  },
  { id: "forest",   label: "Forest",   icon: "🌿" },
  { id: "rose",     label: "Rose",     icon: "🌸" },
  { id: "ocean",    label: "Ocean",    icon: "🌊" },
  { id: "slate",    label: "Slate",    icon: "🔮" },
];

const THEME_IDS = THEMES.map(t => t.id);
const STORAGE_KEY = "zanpos_theme";

function isValidTheme(v: string | null): v is Theme {
  return THEME_IDS.includes(v as Theme);
}

function applyTheme(theme: Theme) {
  document.documentElement.setAttribute("data-theme", theme);
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    return isValidTheme(saved) ? saved : "dark";
  });

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  /** Advance to the next theme in the rotation cycle */
  const toggle = useCallback(() => {
    setTheme(prev => {
      const idx   = THEME_IDS.indexOf(prev);
      const next  = THEME_IDS[(idx + 1) % THEME_IDS.length];
      localStorage.setItem(STORAGE_KEY, next);
      return next;
    });
  }, []);

  const currentMeta = THEMES.find(t => t.id === theme)!;

  return { theme, toggle, currentMeta } as const;
}
