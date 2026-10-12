// Display preferences: per browser, kept in localStorage (which may be unavailable; then defaults apply and nothing
// is remembered). Server-wide settings live on the server, see Settings.svelte.

export type Theme = 'auto' | 'light' | 'dark';

export type Prefs = {
  theme: Theme;
  /** Index into the playlist chip widths (Playlists.svelte). */
  zoom: number;
  /** Playlists the track is in come first. */
  membersFirst: boolean;
  showProgress: boolean;
};

const KEY = 'cratarr-prefs';
export const ZOOM_STEPS = 6;
export const DEFAULTS: Prefs = { theme: 'auto', zoom: 2, membersFirst: false, showProgress: true };

function read(): Prefs {
  const p = { ...DEFAULTS };
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (saved && typeof saved === 'object') {
      if (['auto', 'light', 'dark'].includes(saved.theme)) p.theme = saved.theme;
      if (Number.isInteger(saved.zoom) && saved.zoom >= 0 && saved.zoom < ZOOM_STEPS) p.zoom = saved.zoom;
      if (typeof saved.membersFirst === 'boolean') p.membersFirst = saved.membersFirst;
      if (typeof saved.showProgress === 'boolean') p.showProgress = saved.showProgress;
    } else {
      // Before there were preferences, only the zoom was remembered.
      const z = Number(localStorage.getItem('cratarr-zoom') ?? NaN);
      if (Number.isInteger(z) && z >= 0 && z < ZOOM_STEPS) p.zoom = z;
    }
  } catch {
    /* defaults */
  }
  return p;
}

export const prefs = $state<Prefs>(read());

export function resetPrefs() {
  Object.assign(prefs, DEFAULTS);
}

// Remember every change and apply the theme (app.css reads data-theme; no attribute = follow the system).
$effect.root(() => {
  $effect(() => {
    const snapshot = JSON.stringify(prefs);
    try {
      localStorage.setItem(KEY, snapshot);
    } catch {
      /* not remembered */
    }
    if (prefs.theme === 'auto') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.dataset.theme = prefs.theme;
  });
});
