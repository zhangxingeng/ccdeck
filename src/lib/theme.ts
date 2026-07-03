/**
 * Theme helpers — read/write the ccstudio-theme preference that controls
 * the [data-theme] attribute on <html>.  The initial bootstrap is already
 * done by a tiny inline <script> in app.html so there is no FOUC.
 */

const STORAGE_KEY = 'ccstudio-theme';

export type Theme = 'light' | 'dark';

/** Return the active theme ('light' | 'dark').  Defaults to 'light'. */
export function getTheme(): Theme {
  if (typeof localStorage === 'undefined') return 'light';
  return (localStorage.getItem(STORAGE_KEY) as Theme) ?? 'light';
}

/**
 * Toggle between light and dark, persist the choice, and update the
 * <html data-theme> attribute immediately.  Returns the new theme.
 */
export function toggleTheme(): Theme {
  const next: Theme = getTheme() === 'dark' ? 'light' : 'dark';
  localStorage.setItem(STORAGE_KEY, next);
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', next);
  }
  return next;
}
