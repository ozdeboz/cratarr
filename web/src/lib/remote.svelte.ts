// The remote for what plays, shared by the mini-player and the full player (decision 0018): the commands (shown at
// once, see live.svelte.ts `expect`), the running clock, the favourite, the cover and the colour taken from it.
import { untrack } from 'svelte';
import { coverUrl, albumCoverUrl, type NowPlaying, type Transport, type Upcoming } from './api';
import { expect, live } from './live.svelte';
import { rewindNow, sought } from './localPlayer.svelte';
import { port } from './player/port.svelte';
import { heardAt, placeOf, type Anchor } from './player/clock';
import { followingNow } from './following.svelte';
import { setFavorite as setRowFavorite } from './favourites.svelte';
import { on } from './events.svelte';
import { toast } from './toast.svelte';
import { ramping, rampPressed, rampTurned } from './ramping.svelte';
import { appInfo, inApp } from './app.svelte';
import { startStopNow } from './device.svelte';
import { RESTART_AFTER, windUpMs } from './localPlayer.svelte';

/** What plays where the output is: the server's player, or this browser (decision 0022). */
export const playing = () => port().playing();

/** Actions on what plays wait: a server's player command is shown before it was confirmed, or the local track's
 *  playlists and favourite are still being asked for. */
export const pending = () => port().pending();
/** A command to the server's player is not confirmed yet: what plays may still turn out otherwise. */
export const moving = () => port().moving();

// Cover: the library's cover file for a library track; otherwise (or when there is none) the album's image from the
// server, which also covers music outside the library, such as Incoming. Candidates that fail are skipped.
const failed = $state<Record<string, boolean>>({});
function coverFor(t: { source: string; id: string; album_id: string | null; addable: boolean }): string | null {
  const library = coverUrl(t);
  const album = t.album_id ? albumCoverUrl(t.source, t.album_id) : null;
  const candidates = (t.addable ? [library, album] : [album, library]).filter((u): u is string => !!u);
  return candidates.find((u) => !failed[u]) ?? null;
}
export function coverSrc(): string | null {
  const np = playing();
  return np ? coverFor(np) : null;
}
export const coverFailed = (src: string) => (failed[src] = true);

// Covers ready ahead (the owner, 2026-10-30: Next within an album darkened the cover and turned the ring, though what
// comes next and what came before are known): the next track's cover and the one before are loaded and decoded while
// this one plays, and the last few shown are kept, so Next and Previous show theirs at once. Held here, so the browser
// keeps them decoded. Not while the tape winds (lib/oneInFlight: over plain http each request holds one of six
// connections; a wind through the queue changes track every second or two, and its stream comes first).
const WARM_KEEP = 4;
const warm = new Map<string, HTMLImageElement>();
const warming = new Set<string>();
function keepWarm(src: string, img: HTMLImageElement) {
  warm.delete(src);
  warm.set(src, img);
  while (warm.size > WARM_KEEP) warm.delete(warm.keys().next().value!);
}
$effect.root(() => {
  $effect(() => {
    if (rewindNow.on) return;
    const np = playing();
    const next = np?.upcoming[0];
    const before = live.before;
    for (const want of [next && coverFor({ ...next, source: np!.source }), before && coverFor(before)]) {
      if (!want || warm.has(want) || warming.has(want)) continue;
      warming.add(want);
      const img = new Image();
      img.src = want;
      img.decode().then(() => keepWarm(want, img), () => coverFailed(want)).finally(() => warming.delete(want));
    }
  });
});

// The cover shown: a new one takes the old one's place only once it is loaded and decoded, so a track change never
// shows a frame without a cover, and the same picture at another track's address shows no change at all. A cover
// that fails is skipped (`coverSrc` offers the next candidate). One that has not come in COVER_GIVE_UP_MS gives way to
// the stand-in (the owner, 2026-10-26: no Retry), and still takes its place if it comes after all.
const COVER_GIVE_UP_MS = 10_000;
const shown = $state<{ src: string | null; coming: string | null; late: boolean }>({ src: null, coming: null, late: false });
$effect.root(() => {
  $effect(() => {
    const want = coverSrc();
    if (!want) {
      shown.src = null;
      shown.coming = null;
      shown.late = false;
      return;
    }
    if (want === untrack(() => shown.src)) return void (shown.coming = null);
    const ready = warm.get(want);
    if (ready) {
      keepWarm(want, ready);
      shown.src = want;
      shown.coming = null;
      shown.late = false;
      return;
    }
    let current = true;
    shown.coming = want;
    const img = new Image();
    img.src = want;
    img.decode().then(
      () => current && (keepWarm(want, img), (shown.src = want), (shown.coming = null), (shown.late = false)),
      () => current && ((shown.coming = null), coverFailed(want)),
    );
    const late = setTimeout(() => current && shown.coming === want && ((shown.src = null), (shown.coming = null), (shown.late = true)), COVER_GIVE_UP_MS);
    return () => {
      current = false;
      clearTimeout(late);
    };
  });
});
export const shownCover = () => shown.src;
/** Another cover is on its way (the old one still shown meanwhile). */
export const coverComing = () => shown.coming !== null;
/** The cover did not come in time: the stand-in shows meanwhile. */
export const coverLate = () => shown.late;

// The track as the players show it (the owner, 2026-10-26: a track change is one commit): its words with its cover, so
// while a new cover comes the one before stays named; once it is in (or after TEXT_WAIT_MS, when the cover darkens and
// its ring comes, lib/NowPlaying) the new track shows whole. The track itself plays at once (playing()).
const TEXT_WAIT_MS = 600;
let named = $state.raw<NowPlaying | null>(null);
/** The cover the playing track is waiting for: its address differs from the one shown, and it has not given way. */
const coverPending = () => {
  const want = coverSrc();
  return !!want && want !== shown.src && !shown.late;
};
$effect.root(() => {
  $effect(() => {
    const np = playing();
    const pending = coverPending();
    const was = untrack(() => named);
    if (!np || !was || was.id === np.id || !pending) return void (named = np);
    const t = setTimeout(() => (named = untrack(playing)), TEXT_WAIT_MS);
    return () => clearTimeout(t);
  });
});
/** The track whose progress the players show (the one playing, not the one still named while its cover comes): what a
 *  waveform's colour belongs to (lib/Waveform `track`; the owner, 2026-10-28: keyed on the named one, it flipped). */
export const playingKey = (np: NowPlaying | null) => (np ? `${np.source}/${np.id}` : null);
/** What plays, as the players show it: the track before while the new one's cover comes (at most TEXT_WAIT_MS). Its
 *  progress is the playing track's own once it shows. */
export const shownPlaying = (): NowPlaying | null => {
  const np = playing();
  return named && np && named.id !== np.id ? named : np;
};

// The one clock (lib/player/clock): between reports the position runs on from an anchor, re-anchored where a report
// disagrees much and eased toward one that disagrees a little. The player's bar reads it at its ticks, what moves with
// the music at each frame (lib/trackPlace).
export const clock = $state<{ anchor: Anchor | null; tick: number }>({ anchor: null, tick: performance.now() });

$effect.root(() => {
  $effect(() => {
    const np = playing();
    const p = np?.progress;
    if (!np || !p) {
      clock.anchor = null;
      return;
    }
    const a = untrack(() => clock.anchor);
    const next = heardAt(a, { id: np.id, pos: p.position_secs, paused: !!p.paused, rewinding: !!p.rewinding }, performance.now());
    if (next !== a) clock.anchor = next;
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
function running(): number {
  const own = port().position();
  if (own !== null) return own;
  return placeOf(clock.anchor, clock.tick, duration());
}
// Through a Start and stop ramp the position stands still (the owner, 2026-10-26: it stops hard as a stop begins, and
// starts hard once a start is at full speed): where it was as the ramp began, until it ends. After a stop it rests
// there (the owner, 2026-10-26: "where the waveform stops there is where it should always remain until the user presses
// play again"): not where the wind-down's tail took the player, nor what it reports meanwhile, until it plays again
// (its wind-up held too), a seek moves it or another track plays.
const frozen = $state({ on: false, pos: 0 });
/** The rest after a stop: on which track, at how many seeks, and whether the player has said paused yet. */
let rest: { id: string | null; seeks: number; paused: boolean } | null = null;
/** How long a stop's rest waits for the player's word that it paused (a press it never acted on lets go then). */
const REST_WAIT_MS = 2000;
function letGo() {
  rest = null;
  frozen.on = false;
}
$effect.root(() => {
  $effect(() => {
    // Start and stop's effects only (the owner, 2026-10-27): the tape's own stops and winds never hold the position.
    const dir = port().ramps && ramping.effect ? ramping.dir : null;
    void ramping.at;
    const id = playing()?.id ?? null;
    const still = paused();
    const seeks = sought.n;
    const winding = rewindNow.on || rewindNow.asked;
    untrack(() => {
      // A wind asked or under way: where it is, from the first moment, whatever rest there was.
      if (winding) return letGo();
      if (dir !== null) {
        if (!frozen.on) frozen.pos = running();
        frozen.on = true;
        // Once per stop: a seek during it (counted after) lets go when it ends.
        if (dir === 'out' && !rest) rest = { id, seeks, paused: false };
        return;
      }
      if (!rest || rest.id !== id || rest.seeks !== seeks) return letGo();
      if (still) rest.paused = true;
      // Playing again from the rest (a wind-up ended, or none was needed): it runs from here.
      else if (rest.paused) return letGo();
      else {
        const r = rest;
        setTimeout(() => rest === r && !r.paused && letGo(), REST_WAIT_MS);
      }
    });
  });
});
// Never a small step back (the owner, 2026-10-26: "the jumping back should never happen"): a player's report a little
// behind the page's own reckoning, or a pause settling where the press was, holds the position until it is passed.
// Seeks, restarts and other tracks (more than STEP_BACK back, a seek asked here, or another track) show at once.
const STEP_BACK = 1.5;
let lastShown: { id: string | null; pos: number; seeks: number } = { id: null, pos: 0, seeks: 0 };
export function position(): number {
  const p = frozen.on ? frozen.pos : running();
  const id = playing()?.id ?? null;
  const seeks = sought.n;
  // A tape's rewind on this device goes back on purpose: shown as it goes.
  const back = port().rewinding();
  if (!back && id === lastShown.id && seeks === lastShown.seeks && p < lastShown.pos && lastShown.pos - p < STEP_BACK) return lastShown.pos;
  lastShown = { id, pos: p, seeks };
  return p;
}
export const paused = (): boolean => playing()?.progress?.paused ?? false;
/** Still, as the waveform's colour shows it: paused, or asked to play and not sounding yet (this page's player). The
 *  truth, never the intention: only a Start and stop ramp (lib/SeekWave) runs ahead of it. */
export const waveStill = (progress: { paused?: boolean; starting?: boolean } | null | undefined = playing()?.progress): boolean =>
  (progress?.paused ?? false) || progress?.starting === true;
/** A Start and stop ramp under way on this device's player. The page's own turns round at a press (0075's first step,
 *  the owner 2026-10-27); the app's play button stays inert until its ramp ends, until the app turns too (0.9.13). */
export const rampBusy = () => port().ramps && ramping.dir !== null;
/** The play button inert: only through the app's own Start and stop effect, in an app that does not turn it round (the
 *  page's turns round at a press, an app's from 0.9.15 too; the tape's own stops never show). */
export const rampInert = () => rampBusy() && inApp && ramping.effect && !appInfo.turnsRound;
/** Paused as the play button shows it: from the press on, the way the ramp goes. */
// A Start and stop effect only (the owner, 2026-10-27): the tape's own stops (a turn of the knob, a hold's) never show on
// the play button.
export const shownPaused = (): boolean => (rampBusy() && ramping.effect ? ramping.dir === 'out' : paused());
/** The play button: play or pause; during the page's own ramp, that ramp turned round. */
export function playPause() {
  if (rampInert()) return;
  const ss = startStopNow();
  // The app's ramp turned round at the press, from the fill shown (the page's own turns itself: lib/localPlayer).
  const ramps = port().ramps;
  if (ramps && ss.mode !== 'off' && rampBusy() && inApp && ramping.effect) rampTurned(ss.stopMs, ss.ms);
  else if (ramps && ss.mode !== 'off' && !rampBusy()) {
    if (!paused()) rampPressed('out', ss.stopMs);
    else {
      const ms = windUpMs(ss.ms, ss.mode, position(), inApp ? 0.25 : undefined);
      if (ms > 0) rampPressed('in', ms, ss.ms);
    }
  }
  send('play-pause');
}

export function clockText(secs: number) {
  const s = Math.floor(secs);
  const mm = String(Math.floor((s % 3600) / 60));
  const ss = String(s % 60).padStart(2, '0');
  return s >= 3600 ? `${Math.floor(s / 3600)}:${mm.padStart(2, '0')}:${ss}` : `${mm}:${ss}`;
}

// Commands. What they lead to is shown at once and the server confirms it; only a failure says something, in a toast.
// Previous restarts the track past RESTART_AFTER, as every player does (it was 3 s here: from 3 to 5 s in, the remote
// showed a restart where the player went to the track before).

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
  } else if (command === 'previous' && at > RESTART_AFTER) {
    expect({ ...was, progress: was.progress && { ...was.progress, position_secs: 0 } }, (s) =>
      !s ? 'wait' : s.id !== was.id ? 'no' : (s.progress?.position_secs ?? 0) < Math.max(RESTART_AFTER, at - 2) ? 'yes' : 'wait',
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

export const send = (command: Transport) => {
  // Following someone (0.10): their queue is theirs; a track is asked of them instead (lib/following).
  const f = followingNow();
  if (f && (command === 'next' || command === 'previous')) return void toast.show({ text: `Following ${f.user}: ask for a track instead.` });
  return port().send(command, show);
};

/** Holding Previous (`back`) or Next (`forward`): winds the tape (decision 0074), here (or the app's or the driven
 *  player, lib/localPlayer), or on one of Cratarr's players through the server, which does the rest. */
export const wind = (dir: 'back' | 'forward', phase: 'start' | 'stop') => port().wind(dir, phase);

/** A touch on Previous or Next that may become a hold: this player's wind asked already; dropped if it was a tap. */
export const windTouch = (dir: 'back' | 'forward', on: boolean) => port().windTouch(dir, on);

/** Whether anything can follow what plays (Next dimmed where nothing can; the owner, 2026-10-27): a next track, or
 *  Autoplay, Radio or repeat to make one; elsewhere, what the player says is to come. */
export const hasNext = (): boolean => !followingNow() && port().hasNext();

/** Plays upcoming track `i` (the queue view's tap): within the local stream, or the server's player jumps. */
export const playUpcoming = async (i: number) => port().playUpcoming(i);

/** Seeks the local player (the remote one cannot be dragged yet, decision 0019). */
export const seekTo = (secs: number) => port().seekTo(secs);

/** Moves what plays `secs` on (back, below 0): the local player at once, within its track; one of Cratarr's players
 *  through the server, which moves it from where it is (and keeps it within its track). Others (a Jellyfin client's
 *  report names a player Play On does not know) are not moved. */
export const seekBy = async (secs: number) => port().seekBy(secs, position, duration);

// Favourite: shown as the server reports it; a tap shows the new state at once and asks for it.
const fav = $state<{ wanted: { id: string; on: boolean } | null }>({ wanted: null });
export function favorite(): boolean {
  const np = playing();
  return np && fav.wanted?.id === np.id ? fav.wanted.on : (np?.favorite ?? false);
}
// A favourite changed anywhere (Home Assistant's "I love this track", another device): the playing track's heart shows
// it at once, ahead of the state that follows (app/events.rs `favourite`).
on<{ source: string; kind: string; items: string[]; on: boolean }>('favourite', (c) => {
  const np = playing();
  if (c.kind === 'song' && np && np.source === c.source && c.items.includes(np.id)) fav.wanted = { id: np.id, on: c.on };
});
// The lists' rows show the playing track's heart as the player does (a change from the player, its menu or elsewhere).
// Only when it changes: a row's own flip is not undone by a player that has not heard of it yet.
let mirrored = '';
$effect.root(() => {
  $effect(() => {
    const np = playing();
    if (!np || np.favorite === null) return;
    const on = favorite();
    const now = `${np.source}/${np.id}/${on}`;
    if (now === mirrored) return;
    mirrored = now;
    setRowFavorite(np.source, np.id, on);
  });
});
export async function toggleFavorite() {
  const np = playing();
  if (!np || np.favorite === null || pending()) return;
  const want = { id: np.id, on: !favorite() };
  fav.wanted = want;
  try {
    await port().setFavorite(np, want.on);
  } catch (e) {
    toast.fail(e, "The favourite didn't change");
  } finally {
    // By value: the state proxy is never `===` the plain object it was given.
    if (fav.wanted?.id === want.id && fav.wanted.on === want.on) fav.wanted = null;
  }
}

// The colour taken from the cover (decision 0018, as Finamp's player): its most vivid colour, made readable against
// the player's background (the cover under a dark veil, or a light one in the light theme). null: a grey cover, or
// not known yet; the theme's accent applies then. `of`: the cover it was taken from (a new cover's comes a moment after
// the cover shows: until then the accent is still the one before's).
export const art = $state<{ accent: string | null; of: string | null }>({ accent: null, of: null });
let artFor: string | null = null;

$effect.root(() => {
  $effect(() => {
    const src = shownCover();
    if (src === artFor) return;
    artFor = src;
    if (!src) {
      art.accent = null;
      art.of = null;
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
      art.of = src;
    };
    img.onerror = () => artFor === src && ((art.accent = null), (art.of = src));
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
