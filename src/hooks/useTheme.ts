import { useCallback, useEffect, useState } from "react";

export type Theme =
  | "dark"
  | "light"
  | "midnight"
  | "blossom"
  | "forest"
  | "sky"
  | "sage";

export interface ThemeMeta {
  id: Theme;
  label: string;
  icon: string;
  /** true = light background, false = dark */
  isLight: boolean;
}

export const THEMES: ThemeMeta[] = [
  { id: "dark",     label: "Dark",     icon: "🌑", isLight: false },
  { id: "light",    label: "Light",    icon: "☀",  isLight: true  },
  { id: "midnight", label: "Midnight", icon: "✦",  isLight: false },
  { id: "blossom",  label: "Blossom",  icon: "🌸", isLight: true  },
  { id: "forest",   label: "Forest",   icon: "🌿", isLight: false },
  { id: "sky",      label: "Sky",      icon: "🌤", isLight: true  },
  { id: "sage",     label: "Sage",     icon: "🍃", isLight: true  },
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
    // Light is the product's design default: the reference designs the shell is
    // built against are light, and a retail back office is used under shop
    // lighting rather than in a dark room. Every other theme, including dark,
    // remains available and a stored choice always wins.
    return isValidTheme(saved) ? saved : "light";
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
