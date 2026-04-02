import { createContext, useContext, useState, useCallback, useEffect, useMemo, type ReactNode } from 'react';
import type { ColorScheme } from '../services/preferences';
import { getPreference, setPreference } from '../services/preferences';

type EffectiveTheme = 'dark' | 'light';

interface ThemeContextType {
  colorScheme: ColorScheme;
  setColorScheme: (scheme: ColorScheme) => void;
  /** Cycle through auto → dark → light → auto */
  cycleColorScheme: () => void;
  /** The resolved theme after applying 'auto' logic */
  effectiveTheme: EffectiveTheme;
}

const ThemeContext = createContext<ThemeContextType | null>(null);

const DARK_MEDIA_QUERY = '(prefers-color-scheme: dark)';

function getSystemTheme(): EffectiveTheme {
  return window.matchMedia(DARK_MEDIA_QUERY).matches ? 'dark' : 'light';
}

const CYCLE_ORDER: ColorScheme[] = ['auto', 'dark', 'light'];

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [colorScheme, setColorSchemeState] = useState<ColorScheme>(
    () => getPreference('colorScheme')
  );

  const [systemTheme, setSystemTheme] = useState<EffectiveTheme>(getSystemTheme);

  // Listen for OS theme changes
  useEffect(() => {
    const mql = window.matchMedia(DARK_MEDIA_QUERY);
    const handler = (e: MediaQueryListEvent) => {
      setSystemTheme(e.matches ? 'dark' : 'light');
    };
    mql.addEventListener('change', handler);
    return () => mql.removeEventListener('change', handler);
  }, []);

  const effectiveTheme: EffectiveTheme = colorScheme === 'auto' ? systemTheme : colorScheme;

  // Apply class to <html> element
  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove('dark', 'light');
    root.classList.add(effectiveTheme);
  }, [effectiveTheme]);

  const setColorScheme = useCallback((scheme: ColorScheme) => {
    setPreference('colorScheme', scheme);
    setColorSchemeState(scheme);
  }, []);

  const cycleColorScheme = useCallback(() => {
    setColorSchemeState((current) => {
      const idx = CYCLE_ORDER.indexOf(current);
      const next = CYCLE_ORDER[(idx + 1) % CYCLE_ORDER.length];
      setPreference('colorScheme', next);
      return next;
    });
  }, []);

  const value = useMemo(() => ({
    colorScheme,
    setColorScheme,
    cycleColorScheme,
    effectiveTheme,
  }), [colorScheme, setColorScheme, cycleColorScheme, effectiveTheme]);

  return (
    <ThemeContext.Provider value={value}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextType {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}
