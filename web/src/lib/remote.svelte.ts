// The remote for what plays, shared by the mini-player and the full player (decision 0018): the commands (shown at
// once, see live.svelte.ts `expect`), the running clock, the favourite, the cover and the colour taken from it.
import { untrack } from 'svelte';
import { ApiError, coverUrl, incomingCoverUrl, setFavorite, setTrackFavorite, transport, type NowPlaying, type Transport, type Upcoming } from './api';
import { expect, expectFailed, live, refresh } from './live.svelte';
import { local, jump as localJump, next as localNext, previous as localPrevious, togglePlay as localToggle, seek as localSeek } from './localPlayer.svelte';
import { isLocal, localNowPlaying, localState, tookLocal } from './output.svelte';
import { toast } from './toast.svelte';

/** What plays where the output is: the server's player, or this browser (decision 0022). */
export const playing = () => (isLocal() ? localNowPlaying() : (live.view?.now_playing ?? null));

/** Actions on what plays wait: a server's player command is shown before it was confirmed, or the local track's
 *  playlists and favourite are still being asked for. */
export const pending = () => (isLocal() ? localState.loading : live.pending);
/** A command to the server's player is not confirmed yet: what plays may still turn out otherwise. */
export const moving = () => !isLocal() && live.pending;

// Cover: the library's cover file for a library track; otherwise (or when there is none) the album's image from the
// server, which also covers music outside the library, such as Incoming. Candidates that fail are skipped.
const failed = $state<Record<string, boolean>>({});
export function coverSrc(): string | null {
  const np = playing();
  if (!np) return null;
  const library = coverUrl(np);
  const album = np.album_id ? incomingCoverUrl(np.source, np.album_id) : null;
  const candidates = (np.addable ? [library, album] : [album, library]).filter((u): u is string => !!u);
  return candidates.find((u) => !failed[u]) ?? null;
}
export const coverFailed = (src: string) => (failed[src] = true);

// The cover shown: a new one takes the old one's place only once it is loaded and decoded, so a track change never
// shows a frame without a cover, and the same picture at another track's address shows no change at all. A cover
// that fails is skipped (`coverSrc` offers the next candidate).
const shown = $state<{ src: string | null }>({ src: null });
$effect.root(() => {
  $effect(() => {
    const want = coverSrc();
    if (!want) {
      shown.src = null;
      return;
    }
    if (want === untrack(() => shown.src)) return;
    let current = true;
    const img = new Image();
    img.src = want;
    img.decode().then(
      () => current && (shown.src = want),
      () => current && coverFailed(want),
    );
    return () => (current = false);
  });
});
export const shownCover = () => shown.src;

// The clock: between polls the position runs locally. It is re-anchored to the server only when the server disagrees
// with it (track, pause state, or more than 1.5 s off: a seek or drift), so it moves smoothly.
type Anchor = { id: string; pos: number; at: number; paused: boolean };
const clock = $state<{ anchor: Anchor | null; tick: number }>({ anchor: null, tick: performance.now() });

$effect.root(() => {
  $effect(() => {
    const np = playing();
    const p = np?.progress;
    if (!np || !p) {
      clock.anchor = null;
      return;
    }
    const now = performance.now();
    const a = untrack(() => clock.anchor);
    const predicted = a ? a.pos + (a.paused ? 0 : (now - a.at) / 1000) : 0;
    if (!a || a.id !== np.id || a.paused !== p.paused || Math.abs(predicted - p.position_secs) > 1.5) {
      clock.anchor = { id: np.id, pos: p.position_secs, at: now, paused: p.paused };
    }
  });
  $effect(() => {
    const a = clock.anchor;
    if (!a || a.paused) return;
    // Finamp's position stream (its audio_service fork, `AudioService.position`): 800 updates per track, every
    // length / 800, kept between 16 and 200 ms; the bar moves with each (no smoothing in between). Every 200 ms for a
    // track over 160 s, more often for shorter ones; each step is a fraction of a pixel.
    const length = untrack(() => duration());
    const every = Math.min(200, Math.max(16, length ? (length * 1000) / 800 : 200));
    const t = setInterval(() => {
      if (!document.hidden) clock.tick = performance.now();
    }, every);
    return () => clearInterval(t);
  });
});

export const duration = (): number | null => playing()?.progress?.duration_secs ?? null;
export function position(): number {
  if (isLocal()) return local.position;
  const a = clock.anchor;
  if (!a) return 0;
  const p = a.paused ? a.pos : a.pos + Math.max(0, clock.tick - a.at) / 1000;
  const d = duration();
  return d ? Math.min(p, d) : p;
}
export const paused = (): boolean => playing()?.progress?.paused ?? false;

export function clockText(secs: number) {
  const s = Math.floor(secs);
  const mm = String(Math.floor((s % 3600) / 60));
  const ss = String(s % 60).padStart(2, '0');
  return s >= 3600 ? `${Math.floor(s / 3600)}:${mm.padStart(2, '0')}:${ss}` : `${mm}:${ss}`;
}

// Commands. What they lead to is shown at once and the server confirms it; only a failure says something, in a toast.
/** Within this many seconds of the start, Previous goes to the track before; later it restarts the track. */
const RESTART_SECS = 3;

/** What is shown when `next` starts playing. */
function starting(next: { id: string; name: string; artist: string | null; album: string | null; album_id: string | null; addable: boolean }, duration: number | null, upcoming: Upcoming[]): NowPlaying {
  const at = playing()!;
  return {
    ...at,
    ...next,
    progress: { position_secs: 0, duration_secs: duration, paused: at.progress?.paused ?? false, estimated: false },
    // The heart stays as it was until the server tells (it waits, disabled, meanwhile): no blink between tracks.
    favorite: at.favorite,
    upcoming,
  };
}

function show(command: Transport) {
  const was = playing();
  if (!was) return;
  const at = position();
  if (command === 'next' && was.upcoming.length) {
    const [next, ...rest] = was.upcoming;
    expect(starting(next, next.duration_secs, rest), (s) => (!s ? 'wait' : s.id === next.id ? 'yes' : s.id === was.id ? 'wait' : 'no'));
  } else if (command === 'previous' && at > RESTART_SECS) {
    expect({ ...was, progress: was.progress && { ...was.progress, position_secs: 0 } }, (s) =>
      !s ? 'wait' : s.id !== was.id ? 'no' : (s.progress?.position_secs ?? 0) < Math.max(RESTART_SECS, at - 2) ? 'yes' : 'wait',
    );
  } else if (command === 'previous' && live.before && live.before.id !== was.id) {
    const back = live.before;
    const here: Upcoming = { ...was, duration_secs: was.progress?.duration_secs ?? null };
    expect(
      starting(back, back.progress?.duration_secs ?? null, [here, ...was.upcoming]),
      (s) => (!s ? 'wait' : s.id === back.id ? 'yes' : s.id === was.id ? 'wait' : 'no'),
      // The player may restart the track instead: then it stays, without an error.
      { ms: 4000, quiet: true },
    );
  } else if (command === 'play-pause' && was.progress) {
    const want = !was.progress.paused;
    expect({ ...was, progress: { ...was.progress, paused: want, position_secs: at } }, (s) =>
      !s ? 'no' : s.id !== was.id ? 'no' : s.progress?.paused === want ? 'yes' : 'wait',
    );
  }
}

export async function send(command: Transport) {
  if (isLocal()) {
    if (command === 'play-pause') localToggle();
    else if (command === 'next') localNext();
    else if (command === 'previous') localPrevious();
    return;
  }
  const np = playing();
  if (!np) return;
  show(command);
  try {
    await transport(command, np.source);
    setTimeout(refresh, 700); // the player reports back within a moment
  } catch (e) {
    expectFailed(e instanceof ApiError && !e.generic ? e.message : "The player didn't respond");
  }
}

/** Plays upcoming track `i` (the queue view's tap): within the local stream, or the server's player jumps. */
export async function playUpcoming(i: number) {
  if (isLocal()) return localJump(local.index + 1 + i);
  const np = playing();
  const t = np?.upcoming[i];
  if (!np || !t) return;
  const { jumpTo } = await import('./api');
  await jumpTo(t.id, np.source);
  setTimeout(refresh, 700);
}

/** Seeks the local player (the remote one cannot be dragged yet, decision 0019). */
export const seekTo = (secs: number) => isLocal() && localSeek(secs);

// Favourite: shown as the server reports it; a tap shows the new state at once and asks for it.
const fav = $state<{ wanted: { id: string; on: boolean } | null }>({ wanted: null });
export function favorite(): boolean {
  const np = playing();
  return np && fav.wanted?.id === np.id ? fav.wanted.on : (np?.favorite ?? false);
}
export async function toggleFavorite() {
  const np = playing();
  if (!np || np.favorite === null || pending()) return;
  const want = { id: np.id, on: !favorite() };
  fav.wanted = want;
  try {
    if (isLocal()) tookLocal(np.source, np.id, await setTrackFavorite(np.source, np.id, want.on));
    else live.view = await setFavorite(want.on, np.source);
  } catch (e) {
    toast.fail(e, "The favourite didn't change");
  } finally {
    // By value: the state proxy is never `===` the plain object it was given.
    if (fav.wanted?.id === want.id && fav.wanted.on === want.on) fav.wanted = null;
  }
}

// The colour taken from the cover (decision 0018, as Finamp's player): its most vivid colour, made readable against
// the player's background (the cover under a dark veil, or a light one in the light theme). null: a grey cover, or
// not known yet; the theme's accent applies then.
export const art = $state<{ accent: string | null }>({ accent: null });
let artFor: string | null = null;

$effect.root(() => {
  $effect(() => {
    const src = shownCover();
    if (src === artFor) return;
    artFor = src;
    if (!src) {
      art.accent = null;
      return;
    }
    const img = new Image();
    img.decoding = 'async';
    img.onload = () => {
      if (artFor !== src) return;
      try {
        art.accent = accentFrom(img);
      } catch {
        art.accent = null; // an image the page may not read
      }
    };
    img.onerror = () => artFor === src && (art.accent = null);
    img.src = src;
  });
});

/** The accent for any cover (the menus, decision 0021); null for a grey cover or one that does not load. */
export function accentOf(src: string): Promise<string | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.decoding = 'async';
    img.onload = () => {
      try {
        resolve(accentFrom(img));
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = src;
  });
}

function dark() {
  const t = document.documentElement.dataset.theme;
  return t ? t === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
}

/** RGB 0..255 to HSL with h in 0..360, s and l in 0..1. */
function hsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s, l];
}

function rgbOf(h: number, s: number, l: number): [number, number, number] {
  const k = (n: number) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return [f(0) * 255, f(8) * 255, f(4) * 255];
}

function luminance([r, g, b]: [number, number, number]) {
  const c = (v: number) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b);
}

const contrast = (a: [number, number, number], b: [number, number, number]) => {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};

/** The cover's most vivid hue (hue buckets weighted by saturation, away from black and white), its lightness moved
 *  until it reads at 5.5:1 against the veiled cover. */
function accentFrom(img: HTMLImageElement): string | null {
  const N = 40;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = N;
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.drawImage(img, 0, 0, N, N);
  const px = ctx.getImageData(0, 0, N, N).data;
  const bins = Array.from({ length: 24 }, () => ({ w: 0, r: 0, g: 0, b: 0 }));
  const avg = [0, 0, 0];
  for (let i = 0; i < px.length; i += 4) {
    const [r, g, b] = [px[i], px[i + 1], px[i + 2]];
    avg[0] += r;
    avg[1] += g;
    avg[2] += b;
    const [h, s, l] = hsl(r, g, b);
    const w = s * s * (1 - Math.abs(l - 0.5) * 2);
    const bin = bins[Math.floor(h / 15) % 24];
    bin.w += w;
    bin.r += r * w;
    bin.g += g * w;
    bin.b += b * w;
  }
  const best = bins.reduce((a, b) => (b.w > a.w ? b : a));
  if (best.w < N * N * 0.01) return null; // a grey cover: no colour to take
  const [h, s] = hsl(best.r / best.w, best.g / best.w, best.b / best.w);
  // The background the accent sits on: the cover's average under the veil (black 67.5 %, or white 75 % when light).
  const isDark = dark();
  const veil = isDark ? 0 : 255;
  const share = isDark ? 0.675 : 0.75;
  const n = px.length / 4;
  const surface = avg.map((v) => (v / n) * (1 - share) + veil * share) as [number, number, number];
  const sat = Math.max(0.45, s);
  let l = isDark ? 0.5 : 0.45;
  for (let i = 0; i < 40 && contrast(rgbOf(h, sat, l), surface) < 5.5; i++) l = isDark ? Math.min(0.95, l + 0.02) : Math.max(0.05, l - 0.02);
  const [r, g, b] = rgbOf(h, sat, l).map(Math.round);
  return `rgb(${r} ${g} ${b})`;
}
