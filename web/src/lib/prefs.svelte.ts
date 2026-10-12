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
  /** Source this browser follows when several are active; '' = whichever started playing most recently. */
  source: string;
  /** Insights: the open tab, and which signals bring playlists together in Related. */
  insightsTab: 'statistics' | 'related';
  signals: { tracks: boolean; genre: boolean; artists: boolean };
};

const KEY = 'cratarr-prefs';
export const ZOOM_STEPS = 6;
export const DEFAULTS: Prefs = {
  theme: 'auto',
  zoom: 2,
  membersFirst: false,
  showProgress: true,
  source: '',
  insightsTab: 'statistics',
  signals: { tracks: true, genre: true, artists: true },
};

function read(): Prefs {
  const p = { ...DEFAULTS, signals: { ...DEFAULTS.signals } };
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (saved && typeof saved === 'object') {
      if (['auto', 'light', 'dark'].includes(saved.theme)) p.theme = saved.theme;
      if (Number.isInteger(saved.zoom) && saved.zoom >= 0 && saved.zoom < ZOOM_STEPS) p.zoom = saved.zoom;
      if (typeof saved.membersFirst === 'boolean') p.membersFirst = saved.membersFirst;
      if (typeof saved.showProgress === 'boolean') p.showProgress = saved.showProgress;
      if (typeof saved.source === 'string') p.source = saved.source;
      if (saved.insightsTab === 'statistics' || saved.insightsTab === 'related') p.insightsTab = saved.insightsTab;
      if (saved.signals && typeof saved.signals === 'object') {
        for (const k of ['tracks', 'genre', 'artists'] as const) {
          if (typeof saved.signals[k] === 'boolean') p.signals[k] = saved.signals[k];
        }
      }
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

/** Back to the default look; the followed source is not a display matter and stays. */
export function resetPrefs() {
  Object.assign(prefs, { ...DEFAULTS, signals: { ...DEFAULTS.signals }, source: prefs.source });
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
