// A track's snippet heard (0.12, the owner's mockup A2; lib/snippet picks it): from a track's menu as it opens, or from
// a crate search row's button. On this device only, never on a Play On player, never touching the queue: in its own
// audio element, from the stream at its start, faded in and out; what plays here eased under it (DUCK) and back. In
// the app the app plays it (the page's own sound beside the app's player would take Android's audio focus from it).
// One at a time: a new one ends the one before.
import { BANDCAMP } from './bandcampPreview';
import { inApp, onAppSnippet, toApp } from './app.svelte';
import { local, pageVolume, setSnippetDuck } from './localPlayer.svelte';
import { playsHere } from './output.svelte';
import { playCollection } from './play.svelte';
import { clockText, playing, seekTo } from './remote.svelte';
import { reach } from './reach.svelte';
import { DUCK, FADE_MS, SNIPPET_S, START_DELAY_MS, duckGain, snippetGain, snippetOf, type Stretch } from './snippet';
import { decode } from './visuals/beats';
import { heardOf, loadWaveform, waveformOf } from './waveforms.svelte';

export type SnippetTrack = { source: string; id: string; name?: string | null; duration_secs?: number | null };

/** The snippet asked for (`key`: its track's `source/id`), its stretch (s) once known, where it is (s), and whether it
 *  sounds now. */
export const snippet = $state({ key: null as string | null, from: 0, to: 0, at: 0, sounding: false });

const keyOf = (t: SnippetTrack) => `${t.source}/${t.id}`;
/** Whether a track can have a snippet: one Cratarr streams (not the device's own music), and Cratarr in reach (decision
 *  0085: out of reach the menu stays silent and shows no strip). */
export const snippetable = (t: SnippetTrack | null | undefined) => !!t && t.source !== 'device' && !reach.away;

/** Its length (s): as listed, else as a Bandcamp preview's analysis found it. */
export const lengthOf = (t: SnippetTrack) => t.duration_secs ?? (t.source === BANDCAMP ? (heardOf(t.source, t.id)?.duration_secs ?? null) : null);

/** A track's stored beats (seconds), null where none is stored; asked once per track, never worked out for a menu
 *  (2026-10-30: a menu never asks the server to work anything out: `stored=1` answers 404 rather than decode). */
const storedBeats = new Map<string, Float64Array | null>();
let beatsAsk: AbortController | null = null;
function askBeats(t: SnippetTrack) {
  const key = keyOf(t);
  if (storedBeats.has(key)) return;
  // One at a time: a new menu's ask cancels the one before.
  beatsAsk?.abort();
  const ctl = (beatsAsk = new AbortController());
  fetch(`/api/track/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}/beats?b=2&stored=1`, { signal: ctl.signal })
    .then(async (r) => {
      if (r.ok) storedBeats.set(key, decode(await r.arrayBuffer())?.at ?? null);
      else if (r.status === 404) storedBeats.set(key, null);
    })
    .catch(() => {})
    .finally(() => beatsAsk === ctl && (beatsAsk = null));
}

/** Asks for what a track's snippet is found by, as far as it is stored: its waveform (the server's stored one; a
 *  Bandcamp preview's only as already worked out on this device, never fetched whole for a menu) and its beats. Not
 *  from a derivation: the asking writes state. */
export function prepareSnippet(t: SnippetTrack) {
  if (!snippetable(t) || t.source === BANDCAMP) return;
  loadWaveform(t.source, t.id);
  askBeats(t);
}

/** Where a Bandcamp preview whose length is not known yet starts (s): its length comes only from working its file out,
 *  which a menu does not do. */
const UNKNOWN_FROM = 30;

/** The track's snippet stretch as far as is known now (its waveform; its stored beats with `beats`); with no stored
 *  waveform, from FIXED_AT of its length. Asks for nothing. */
export function stretchOf(t: SnippetTrack, beats = false): Stretch | null {
  if (!snippetable(t)) return null;
  const duration = lengthOf(t);
  if (!duration) return t.source === BANDCAMP ? { from: UNKNOWN_FROM, to: UNKNOWN_FROM + SNIPPET_S } : null;
  const b = beats ? (storedBeats.get(keyOf(t)) ?? null) : null;
  return snippetOf(waveformOf(t.source, t.id) ?? null, duration, b);
}

let timer: ReturnType<typeof setTimeout> | null = null;
let element: HTMLAudioElement | null = null;
let raf = 0;
/** The duck as it stands, and where it goes. */
let duck = 1;
let duckRaf = 0;
/** The fade's own volume, for a fade out from wherever it is. */
let gain = 0;
let ending = false;

function setDuck(down: boolean) {
  cancelAnimationFrame(duckRaf);
  // Only what plays here ducks: a player driven from here plays elsewhere, left alone.
  if (down && !(playsHere() && !local.paused)) return;
  if (!down && duck === 1) return;
  const from = duck;
  const t0 = performance.now();
  const step = () => {
    duck = duckGain(performance.now() - t0, down, from);
    setSnippetDuck(duck);
    if ((down ? duck > DUCK : duck < 1) && performance.now() - t0 < FADE_MS) duckRaf = requestAnimationFrame(step);
    else setSnippetDuck((duck = down ? DUCK : 1));
  };
  duckRaf = requestAnimationFrame(step);
}

/** Plays `t`'s snippet in START_DELAY_MS, unless it is ended before (a quick ⋮ and away plays nothing). */
export function startSnippet(t: SnippetTrack) {
  if (!snippetable(t)) return;
  const key = keyOf(t);
  if (snippet.key === key && (timer || snippet.sounding)) return;
  endNow();
  snippet.key = key;
  snippet.at = 0;
  prepareSnippet(t);
  const s = stretchOf(t);
  if (s) Object.assign(snippet, { from: s.from, to: s.to, at: s.from });
  timer = setTimeout(() => {
    timer = null;
    if (snippet.key !== key) return;
    // Its stretch again: the waveform and the beats may have come meanwhile.
    const now = stretchOf(t, true);
    if (!now || now.to <= now.from) return;
    Object.assign(snippet, { from: now.from, to: now.to, at: now.from });
    sound(t, now);
  }, START_DELAY_MS);
}

function sound(t: SnippetTrack, s: Stretch) {
  if (inApp) {
    toApp({ type: 'snippet', play: { source: t.source, id: t.id, from_ms: Math.round(s.from * 1000), to_ms: Math.round(s.to * 1000), fade_ms: FADE_MS, duck: DUCK } });
    snippet.sounding = true;
    return;
  }
  const a = new Audio(`/api/stream/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}?from_ms=${Math.round(s.from * 1000)}&mp3_from=1`);
  a.volume = 0;
  a.preload = 'auto';
  element = a;
  ending = false;
  const length = (s.to - s.from) * 1000;
  const step = () => {
    if (element !== a) return;
    const ms = a.currentTime * 1000;
    snippet.at = s.from + a.currentTime;
    if (!ending) {
      gain = snippetGain(ms, length);
      a.volume = Math.max(0, Math.min(1, gain * (pageVolume.level / 100)));
      if (ms >= length) return void endSnippet();
    }
    raf = requestAnimationFrame(step);
  };
  a.addEventListener('playing', () => {
    if (element !== a) return;
    snippet.sounding = true;
    setDuck(true);
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(step);
  });
  a.addEventListener('error', () => element === a && endNow());
  a.play().catch(() => element === a && endNow());
}

/** Ends the snippet, faded out (FADE_MS) and what plays here back up. */
export function endSnippet() {
  if (timer) clearTimeout(timer);
  timer = null;
  snippet.key = null;
  if (inApp) {
    if (snippet.sounding) toApp({ type: 'snippet', stop: true });
    snippet.sounding = false;
    return;
  }
  const a = element;
  if (!a || ending) return void (!a && setDuck(false));
  ending = true;
  setDuck(false);
  const from = gain;
  const t0 = performance.now();
  const fade = () => {
    if (element !== a) return;
    const k = 1 - (performance.now() - t0) / FADE_MS;
    if (k <= 0) return void endNow();
    a.volume = Math.max(0, Math.min(1, from * k * (pageVolume.level / 100)));
    requestAnimationFrame(fade);
  };
  requestAnimationFrame(fade);
}

/** Ends it at once (another snippet comes, or the track plays for real). */
function endNow() {
  if (timer) clearTimeout(timer);
  timer = null;
  cancelAnimationFrame(raf);
  const a = element;
  element = null;
  if (a) {
    a.pause();
    a.removeAttribute('src');
    a.load();
  }
  if (inApp && snippet.sounding) toApp({ type: 'snippet', stop: true });
  snippet.sounding = false;
  ending = false;
  gain = 0;
  setDuck(false);
}

// The app's snippet as it plays: its place, and its end.
onAppSnippet((atMs) => {
  if (atMs === null) {
    snippet.sounding = false;
    snippet.key = null;
  } else snippet.at = atMs / 1000;
});

/** Plays `t` for real from `secs` in (the snippet's moment): the snippet ended, the track played as any track, then
 *  moved there once it plays. */
/** Where Play from here starts the track (s): its snippet's start while it sounds, else its stretch's as known now; 0
 *  when not known. */
export function startOf(t: SnippetTrack): number {
  return snippet.key === keyOf(t) && snippet.to > snippet.from ? snippet.from : (stretchOf(t)?.from ?? 0);
}

/** Play from here's words: from when (the owner's audit, 2026-10-30: say from when), else plainly. */
export const fromLabel = (secs: number) => (secs > 0 ? `Play from ${clockText(secs)}` : 'Play from here');

export async function playFromHere(t: SnippetTrack, secs: number) {
  endNow();
  await playCollection({ kind: 'tracks', source: t.source, ids: [t.id] }, 'play', t.name ?? '');
  const until = performance.now() + 5000;
  while (performance.now() < until) {
    const np = playing();
    if (np && np.source === t.source && np.id === t.id && np.progress) {
      seekTo(secs);
      return;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
}
