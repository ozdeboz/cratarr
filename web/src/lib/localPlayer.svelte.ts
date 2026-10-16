// The local player (decision 0022): Cratarr plays in the browser, gapless, through one continuous MSE stream. Each
// track comes from the server (/api/stream: FLAC repackaged as fragmented MP4, MP3 as it is) and is appended to the
// same stream right after the one before: MP3 trimmed by its LAME header's encoder delay at the start and to its exact
// length at the end (measured to the sample with tools/gapless-lab), FLAC placed at its own sample positions. The next
// track is appended while the current one plays; what has been played is dropped from the buffer again. A track is
// read only as far as the playback needs (LEAD), so the browser's small audio buffer never has to hold one whole.
//
// The stream's time is mapped back to "which track, how far in" through the segments: one per appended track, from
// where its sample 0 sits in the stream. Skipping to an appended track moves within the stream (no gap, no reload);
// anything else (a jump back, a seek outside what is loaded) starts a fresh stream at that point.
//
// Inside the Android app the app plays instead, natively (decision 0031, amended), and the page can also drive another
// of Cratarr's players (Play On, `drive`): then every call below is handed to that player (app.svelte.ts, or the
// server's /api/players), what it reports is shown in `local`, and the stream here is not used.

import { inApp, onApp, toApp } from './app.svelte';
import { playerCommand, radioNext } from './api';

export type LocalTrack = {
  /** Its entry in a player's queue (decision 0033), once Cratarr or the app has given it one. */
  entry?: string;
  /** `next`: added by hand (Play next, Add to queue), not part of the album or playlist that plays (decision 0033's
   *  two lanes). `radio`: chosen by the radio, after everything else (decision 0036). */
  lane?: 'next' | 'radio';
  /** Its place in the album or playlist as it was played, before any shuffle: turning shuffle off orders by it. */
  n?: number;
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  /** False for a track no playlist line can name (outside every library Cratarr sees). */
  addable?: boolean;
};

export type Repeat = 'off' | 'one' | 'all';
/** A sleep timer: pause at `atMs` (epoch ms), or when the playing track ends. */
export type Sleep = { atMs: number | null; endOfTrack: boolean };
/** How the radio chooses (decision 0036). */
export type RadioMode = 'similar' | 'continuous' | 'album_mix' | 'era' | 'artist' | 'reshuffle' | 'random';
/** The radio while it is on: its mode, the tracks it stays close to when started from an item (else the queue's own
 *  album or playlist), and the crate it plays inside ("Play as radio": only the crate's tracks), when it does. */
export type Radio = { mode: RadioMode; seed: string[]; within?: string | null };
export const RADIO_MODES: readonly RadioMode[] = ['similar', 'continuous', 'album_mix', 'era', 'artist', 'reshuffle', 'random'];

/** One appended track: its queue index, where its sample 0 sits in the stream (s), its length (s; null while
 *  unknown), and the length the server read from the file (s), shown while the exact one is unknown. */
type Segment = { index: number; start: number; length: number | null; told: number | null };

export const local = $state({
  queue: [] as LocalTrack[],
  /** The playing track's index in `queue`; -1 when nothing is loaded. */
  index: -1,
  paused: true,
  /** Seconds into the playing track. */
  position: 0,
  /** The playing track's length (s), when known. */
  duration: null as number | null,
  /** The last thing that failed, in plain words. */
  error: null as string | null,
  /** What comes of the album or playlist plays shuffled (the player's shuffle switch). */
  shuffled: false,
  /** Repeat (decision 0033, amended): off, one (the playing track), all (the album or playlist). */
  repeat: 'off' as Repeat,
  /** The sleep timer of the player that plays elsewhere (the app, Cratarr's player), as it reports it; null when none
   *  is set or it fired. */
  sleep: null as Sleep | null,
  /** The radio (decision 0036), null while off: the music goes on after the queue, chosen from the library. */
  radio: null as Radio | null,
  /** What the queue was started from ("Chill", an album), for the queue sheet's "Playing from"; null when unknown. */
  from: null as string | null,
});

const MP4 = 'audio/mp4; codecs="flac"';
const MPEG = 'audio/mpeg';
/** Keep this much ahead in the stream before fetching the next track (s). */
const AHEAD = 30;
/** Within a track, read no further than this ahead of the playing position (s): the rest of the fetch waits for the
 *  playback, so a long track never has to fit the browser's buffer whole (Chrome takes 12 MiB of audio, about two
 *  minutes of FLAC; on a phone 5, 2 or even 1 MiB). */
const LEAD = 60;
/** Bytes appended at a time (a phone's whole audio buffer can be 1 MiB). */
const SLICE = 256 * 1024;
/** Drop played audio older than this behind the playing position (s). */
const BEHIND = 30;
/** Previous restarts the track after this many seconds (Finamp: 5). */
const RESTART_AFTER = 5;

// The stream this browser offers: MediaSource, or on iPhone and iPad (Safari 17.1 on) only ManagedMediaSource, which
// works the same but lets the browser say when to fetch (startstreaming, endstreaming) and drop buffered audio itself
// (bufferedchange). Neither: the local player is not offered.
const Managed = typeof window === 'undefined' ? undefined : (window as unknown as { ManagedMediaSource?: typeof MediaSource }).ManagedMediaSource;
const Source = Managed ?? (typeof MediaSource === 'undefined' ? undefined : MediaSource);
/** Whether this browser can play here at all (FLAC or MP3 in its stream), or the app plays for it. */
export const canPlayHere = inApp || (!!Source && (Source.isTypeSupported(MP4) || Source.isTypeSupported(MPEG)));

const audio = typeof Audio === 'undefined' || !canPlayHere || inApp ? null : new Audio();
// Safari opens a managed stream only for an element that cannot be sent to AirPlay (the stream cannot go there).
if (audio && Managed) audio.disableRemotePlayback = true;
let ms: MediaSource | null = null;
let sb: SourceBuffer | null = null;
let mime: string | null = null;
let segments: Segment[] = [];
/** Stream time where the next appended track's sample 0 goes. */
let streamEnd = 0;
/** Bumped by every fresh stream; work for an older one stops. */
let generation = 0;
let fetching: AbortController | null = null;
let appending = false;
/** The queue index being fetched and appended (while `appending`). */
let appendingIndex = -1;
/** The last queue index appended to the stream. */
let lastAppended = -1;
/** The queue index `local.error` is about: it goes once a later track plays. */
let errorIndex = -1;

// --- MP3: the LAME/Xing header (the measured rule: only the encoder delay precedes the first real sample) ---

/** `at`: where the first frame starts, after the ID3 tag (the tag, covers and all, never goes into the stream). */
type Mp3Head = { rate: number; delay: number; samples: number | null; at: number };

/** Parses the first frame of an MP3 for its sample rate and LAME gapless information; null when more bytes are needed. */
function mp3Head(b: Uint8Array): Mp3Head | null | 'invalid' {
  let i = 0;
  if (b.length < 10) return null;
  if (b[0] === 0x49 && b[1] === 0x44 && b[2] === 0x33) i = 10 + ((b[6] << 21) | (b[7] << 14) | (b[8] << 7) | b[9]);
  const tagEnd = i; // the ID3 tag can be large (embedded covers of several MB): the limit below counts from its end
  while (i + 4 <= b.length && !(b[i] === 0xff && (b[i + 1] & 0xe0) === 0xe0)) i++;
  if (i + 200 > b.length) return b.length > tagEnd + (1 << 22) ? 'invalid' : null; // wait for the first frame and its header
  const version = (b[i + 1] >> 3) & 3; // 3 = MPEG1, 2 = MPEG2, 0 = MPEG2.5
  const rates = version === 3 ? [44100, 48000, 32000] : version === 2 ? [22050, 24000, 16000] : [11025, 12000, 8000];
  const rate = rates[(b[i + 2] >> 2) & 3] ?? 44100;
  const mono = (b[i + 3] & 0xc0) === 0xc0;
  const side = version === 3 ? (mono ? 17 : 32) : mono ? 9 : 17;
  const x = i + 4 + side;
  const tag = String.fromCharCode(b[x], b[x + 1], b[x + 2], b[x + 3]);
  if (tag !== 'Xing' && tag !== 'Info') return { rate, delay: 0, samples: null, at: i };
  const flags = (b[x + 4] << 24) | (b[x + 5] << 16) | (b[x + 6] << 8) | b[x + 7];
  let p = x + 8;
  let frames: number | null = null;
  if (flags & 1) {
    frames = ((b[p] << 24) | (b[p + 1] << 16) | (b[p + 2] << 8) | b[p + 3]) >>> 0;
    p += 4;
  }
  if (flags & 2) p += 4;
  if (flags & 4) p += 100;
  if (flags & 8) p += 4;
  const isLame = String.fromCharCode(b[p], b[p + 1], b[p + 2], b[p + 3]) === 'LAME' || String.fromCharCode(b[p], b[p + 1], b[p + 2], b[p + 3]) === 'Lavc';
  const d = p + 21;
  const delay = isLame ? (b[d] << 4) | (b[d + 1] >> 4) : 0;
  const padding = isLame ? ((b[d + 1] & 0x0f) << 8) | b[d + 2] : 0;
  const perFrame = version === 3 ? 1152 : 576;
  return { rate, delay, samples: frames === null ? null : frames * perFrame - delay - padding, at: i };
}

// --- the stream ---

async function waitUpdate() {
  if (sb?.updating) await new Promise((r) => sb!.addEventListener('updateend', r, { once: true }));
}

/** Seconds of audio buffered from the playing position on. */
function ahead() {
  if (!audio || !sb) return 0;
  const t = audio.currentTime;
  const b = sb.buffered;
  for (let i = 0; i < b.length; i++) if (b.start(i) <= t + 0.1 && b.end(i) > t) return b.end(i) - t;
  return 0;
}

/** Seconds of audio buffered in all. */
function held() {
  if (!sb) return 0;
  let s = 0;
  for (let i = 0; i < sb.buffered.length; i++) s += sb.buffered.end(i) - sb.buffered.start(i);
  return s;
}

/** Resolves once the playback moves on (the element's next timeupdate), or at once when `signal` aborts (a new
 *  stream, a cut, a stop). A paused player keeps it waiting: nothing more is needed until it plays. */
function playbackMoves(signal: AbortSignal) {
  return new Promise<void>((resolve) => {
    if (signal.aborted || !audio) return resolve();
    const done = new AbortController();
    const go = () => {
      done.abort();
      resolve();
    };
    audio.addEventListener('timeupdate', go, { signal: AbortSignal.any([signal, done.signal]) });
    signal.addEventListener('abort', go, { signal: done.signal });
  });
}

/** Appends bytes when there is reason and room for them, as developer.chrome.com/blog/quotaexceedederror advises:
 *  no further than LEAD ahead of the playing position, and when the browser's buffer is full (QuotaExceededError)
 *  once played audio is dropped or, with none left to drop, once the playback has used up some of what is ahead.
 *  Holding back holds the fetch back too (its reader is not read), and the server's stream waits with it. */
async function appendBytes(bytes: Uint8Array<ArrayBuffer>, gen: number, signal: AbortSignal) {
  // In slices: the browser wants room for a whole append before it takes any of it.
  for (let at = 0; at < bytes.length; at += SLICE) if (!(await appendSlice(bytes.subarray(at, at + SLICE), gen, signal))) return;
}

/** One slice of `appendBytes`; false when the stream moved on meanwhile. */
async function appendSlice(bytes: Uint8Array<ArrayBuffer>, gen: number, signal: AbortSignal) {
  // Cut meanwhile (the queue changed after this track): nothing more of it goes in.
  const gone = () => gen !== generation || !sb || signal.aborted;
  for (;;) {
    while (!gone() && ahead() > LEAD) {
      await evict();
      await playbackMoves(signal);
    }
    if (gone()) return false;
    await waitUpdate();
    if (gone()) return false;
    try {
      sb!.appendBuffer(bytes);
      await waitUpdate();
      return true;
    } catch (e) {
      if ((e as DOMException).name !== 'QuotaExceededError') throw e;
    }
    const before = held();
    await evict(5);
    if (held() < before) continue;
    // Nothing behind to drop and next to nothing ahead: the browser takes less than these bytes, waiting won't help.
    if (ahead() < 1) throw new Error('The browser has no room for more audio.');
    await playbackMoves(signal);
  }
}

/** Drops played audio more than `keep` seconds behind the playing position. */
async function evict(keep = BEHIND) {
  if (!sb || !audio || !sb.buffered.length) return;
  const until = audio.currentTime - keep;
  if (until <= sb.buffered.start(0) + 1) return;
  await waitUpdate();
  try {
    sb.remove(sb.buffered.start(0), until);
    await waitUpdate();
  } catch {
    /* the stream moved on; nothing to drop */
  }
}

/** Fetches queue track `index` and appends it to the stream, starting `from` seconds into it (a seek). */
async function appendTrack(index: number, from: number, gen: number) {
  const t = local.queue[index];
  if (!t) return;
  appending = true;
  appendingIndex = index;
  const ctl = (fetching = new AbortController());
  /** This track's segment, once placed in the stream. */
  let placedSegment = null as Segment | null; // set inside place(), which TypeScript does not follow
  try {
    const res = await fetch(`/api/stream/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}${from > 0 ? `?from_ms=${Math.floor(from * 1000)}` : ''}`, {
      signal: ctl.signal,
    });
    if (!res.ok) {
      let text = '';
      try {
        text = (await res.json()).error ?? '';
      } catch {
        /* no words */
      }
      throw new Error(`${t.name || 'A track'}: ${text || 'it could not be loaded.'}`);
    }
    const type = res.headers.get('content-type') ?? '';
    const kind = type.startsWith('audio/mp4') ? MP4 : MPEG;
    if (!Source?.isTypeSupported(kind)) throw new Error(`${t.name}: this browser cannot play ${kind === MP4 ? 'FLAC' : 'MP3'} here.`);
    const reader = res.body!.getReader();
    // The track's sample 0 goes to `start` in the stream (before the stream's 0 when starting mid-track).
    const start = streamEnd - from;
    let head = new Uint8Array(0);
    let placed = false;
    let length: number | null = null;
    const told = Number(res.headers.get('x-duration-ms')) / 1000 || null;
    const segment: Segment = { index, start, length: null, told };
    const place = async (rate: number, skip: number, samples: number | null) => {
      if (!sb) {
        sb = ms!.addSourceBuffer(kind);
        if (Managed) sb.addEventListener('bufferedchange', purged);
        mime = kind;
      } else if (mime !== kind) {
        await waitUpdate();
        sb.changeType(kind);
        mime = kind;
      }
      await waitUpdate();
      // The track before can end mid-frame (an MP3's last bytes are no whole frame, or its fetch was cut): the parser
      // then still waits for the rest, and neither the mode nor the offset may change ("PARSING_MEDIA_SEGMENT").
      // abort() drops that partial frame, which lies past that track's end (appendWindowEnd) anyway. A stream that was
      // ended (the queue's last track was in; more were added since) must be reopened first: removing nothing does it.
      if (ms?.readyState === 'ended') sb.remove(ms.duration, Infinity);
      // Until every removal is done (this one, a cut's, played audio dropped): nothing below may run while one does.
      while (sb.updating) await waitUpdate();
      if (ms?.readyState === 'open') sb.abort();
      // FLAC's fragments carry their own sample times (tfdt; after a seek they start at the frame holding the sample
      // asked for), so it runs in segments mode with the offset at the track's sample 0. MP3 has no times of its own:
      // sequence mode, the offset at its first decoded sample (the encoder delay before sample 0).
      const mode: AppendMode = kind === MP4 ? 'segments' : 'sequence';
      if (sb.mode !== mode) sb.mode = mode;
      length = samples === null ? null : samples / rate;
      sb.appendWindowEnd = Infinity;
      sb.appendWindowStart = Math.max(0, streamEnd);
      if (length !== null) sb.appendWindowEnd = start + length;
      sb.timestampOffset = kind === MP4 ? start : start - skip / rate;
      segment.length = length;
      segments.push(segment);
      placedSegment = segment;
      placed = true;
    };
    if (kind === MP4) {
      const rate = Number(res.headers.get('x-sample-rate')) || 44100;
      const total = Number(res.headers.get('x-total-samples')) || null;
      await place(rate, 0, total);
    }
    for (;;) {
      const { value, done } = await reader.read();
      if (gen !== generation) {
        reader.cancel().catch(() => {});
        return;
      }
      if (done) break;
      let chunk = value as Uint8Array<ArrayBuffer>;
      if (!placed) {
        const joined = new Uint8Array(head.length + chunk.length);
        joined.set(head);
        joined.set(chunk, head.length);
        const h = mp3Head(joined);
        if (h === null) {
          head = joined;
          continue;
        }
        if (h === 'invalid') throw new Error(`${t.name} is not a playable MP3.`);
        await place(h.rate, h.delay, h.samples);
        chunk = joined.subarray(h.at);
      }
      await appendBytes(chunk, gen, ctl.signal);
    }
    if (!placed && head.length) {
      const h = mp3Head(head);
      if (h && h !== 'invalid') {
        await place(h.rate, h.delay, h.samples);
        await appendBytes(head.subarray(h.at), gen, ctl.signal);
      }
    }
    // A new stream, or the queue changed after this track (cutFrom): it no longer counts as appended.
    if (gen !== generation || ctl.signal.aborted) return;
    // A length the header did not give: what the stream now holds.
    if (segment.length === null && sb && sb.buffered.length) segment.length = sb.buffered.end(sb.buffered.length - 1) - start;
    streamEnd = start + (segment.length ?? 0);
    lastAppended = index;
    if (index === local.queue.length - 1 && ms?.readyState === 'open') {
      await waitUpdate();
      ms.endOfStream();
    }
  } catch (e) {
    if ((e as DOMException).name === 'AbortError' || gen !== generation) return;
    local.error = e instanceof Error ? e.message : String(e);
    errorIndex = index;
    // Skip what cannot play, as a player does, unless it was the only thing.
    if (index < local.queue.length - 1) {
      lastAppended = index;
      // Placed already (it failed midway): that segment becomes the empty one, not a second segment for the track.
      if (placedSegment) placedSegment.length = 0;
      else segments.push({ index, start: streamEnd, length: 0, told: null });
    }
  } finally {
    // Not when a newer append is under way (this one was cut).
    if (gen === generation && fetching === ctl) appending = false;
  }
}

/** Starts a fresh stream at queue track `index`, `from` seconds in. */
async function fresh(index: number, from = 0, play = true) {
  if (!audio) return;
  const gen = ++generation;
  fetching?.abort();
  appending = false;
  segments = [];
  streamEnd = 0;
  lastAppended = index - 1;
  sb = null;
  mime = null;
  local.index = index;
  local.position = from;
  local.error = null;
  ms = new Source!();
  const url = URL.createObjectURL(ms);
  audio.src = url;
  if (play) audio.play().catch(() => {});
  await new Promise((r) => ms!.addEventListener('sourceopen', r, { once: true }));
  // The element holds the stream now; the URL is no longer needed (each seek or jump makes a new one).
  URL.revokeObjectURL(url);
  if (gen !== generation) return;
  await appendTrack(index, from, gen);
}

/** Whether the browser wants audio fetched now: always, except a managed stream between its endstreaming and
 *  startstreaming (the tick then waits; what is being fetched finishes). */
function streaming() {
  return !Managed || (ms as unknown as { streaming?: boolean } | null)?.streaming !== false;
}

/** A managed stream's browser dropped buffered audio (bufferedchange). Behind the playing position that is what the
 *  player does itself; ahead of it the track it was in is fetched again: the one playing from where it is (a fresh
 *  stream), a later one by cutting the stream there, after which the tick appends it again. */
function purged() {
  if (!audio || !sb) return;
  const t = audio.currentTime;
  let end = t; // where the audio that plays now runs out
  for (let i = 0; i < sb.buffered.length; i++) if (sb.buffered.start(i) <= t + 0.1) end = Math.max(end, sb.buffered.end(i));
  if (end >= streamEnd - 0.5) return; // all appended audio ahead is still there
  const seg = segments.find((s) => s.start + (s.length ?? 0) > end + 0.5);
  if (!seg) return;
  if (seg.index <= local.index) fresh(local.index, local.position, !audio.paused);
  else cutFrom(seg.index);
}

/** Which segment the stream is in now, and how far into its track. */
function where(): { seg: Segment; offset: number } | null {
  if (!audio) return null;
  const t = audio.currentTime;
  for (let i = segments.length - 1; i >= 0; i--) {
    const s = segments[i];
    if (t >= s.start - 0.001) return { seg: s, offset: t - s.start };
  }
  return segments[0] ? { seg: segments[0], offset: 0 } : null;
}

// The clock: the playing track and position, the next track fetched ahead, played audio dropped, the lock screen kept
// up to date. Four times a second is plenty for the display; the audio itself runs on its own.
let lastShown = -1;
function tick() {
  // Played elsewhere (another of Cratarr's players): this page's own, idle audio says nothing about it.
  if (!audio || local.index < 0 || handed()) return;
  local.paused = audio.paused;
  const w = where();
  if (w) {
    if (w.seg.index !== local.index) {
      // Repeat one: the stream went on to the next track by itself; back to the start of this one. (Next and previous
      // set the index themselves, so they still move on.)
      if (local.repeat === 'one' && w.seg.index === local.index + 1) return jump(local.index);
      local.index = w.seg.index;
      nextRound();
      fillRadio();
    }
    local.position = Math.max(0, w.offset);
    local.duration = w.seg.length ?? w.seg.told;
  }
  if (local.error && local.index > errorIndex && !audio.paused) local.error = null;
  if (local.index !== lastShown) {
    lastShown = local.index;
    showOnLockScreen();
  }
  const gen = generation;
  if (!appending && lastAppended >= local.index && lastAppended < local.queue.length - 1 && streamEnd - audio.currentTime < AHEAD && streaming()) {
    appendTrack(lastAppended + 1, 0, gen);
  }
  if (!appending) evict();
  if ('mediaSession' in navigator && local.duration && Number.isFinite(local.duration)) {
    try {
      navigator.mediaSession.setPositionState({ duration: local.duration, position: Math.min(local.position, local.duration), playbackRate: 1 });
    } catch {
      /* not supported */
    }
  }
}
if (audio) {
  setInterval(tick, 250);
  audio.addEventListener('play', tick);
  audio.addEventListener('pause', tick);
  audio.addEventListener('ended', () => {
    local.paused = true;
  });
}

function showOnLockScreen() {
  if (!('mediaSession' in navigator)) return;
  const t = local.queue[local.index];
  if (!t) return;
  navigator.mediaSession.metadata = new MediaMetadata({
    title: t.name,
    artist: t.artist ?? '',
    album: t.album ?? '',
    artwork: [{ src: `/api/cover/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}` }],
  });
}

/** The engine's audio element, for measuring (tools/gapless-lab, web/engine-lab.html); never for playback control. */
export const engineAudio = () => audio;

// --- played elsewhere: the app's own player, or another of Cratarr's players; what it plays, as it reports it ---

/** One of Cratarr's players the page drives (Play On), by name; null: this browser or the app plays. */
let driven: string | null = null;
/** Whether the queue plays elsewhere than in this page's stream. */
const handed = () => inApp || driven !== null;

/** From now on the queue plays on one of Cratarr's players (by name), or here again (`null`). What `local` holds is
 *  then that player's, as it reports it (`reported`). */
export function drive(name: string | null) {
  driven = name;
  // The next player's own, from its first report.
  local.sleep = null;
  reported = { positionMs: 0, at: performance.now() };
}

/** The last report: the position then (ms), and when it came; the position in between is counted on here. */
let reported = { positionMs: 0, at: 0 };
const sameQueue = (a: LocalTrack[], b: LocalTrack[]) =>
  a.length === b.length && a.every((t, i) => t.source === b[i].source && t.id === b[i].id && t.entry === b[i].entry);

/** The player that plays: what it plays now (`queue` only when it changed). */
export function reportedBy(m: {
  queue?: unknown;
  index?: unknown;
  positionMs?: unknown;
  durationMs?: unknown;
  paused?: unknown;
  error?: unknown;
  repeat?: unknown;
  sleep?: unknown;
  radio?: unknown;
}) {
  if (m.repeat === 'off' || m.repeat === 'one' || m.repeat === 'all') local.repeat = m.repeat;
  // Left out: not said (a report of the queue alone); null: none.
  if (m.sleep !== undefined) local.sleep = sleepOf(m.sleep);
  if (m.radio !== undefined) local.radio = radioOf(m.radio);
  if (Array.isArray(m.queue) && !sameQueue(m.queue as LocalTrack[], local.queue)) {
    // A queue the player changed (moved, added to) keeps the names this page already knows of its tracks.
    const known = new Map(local.queue.filter((t) => t.name).map((t) => [`${t.source}/${t.id}`, t]));
    local.queue = (m.queue as LocalTrack[]).map((t) => (t.name ? t : (known.get(`${t.source}/${t.id}`) ?? t)));
    // A queue the page did not order itself (a page opened while it played): shuffled when its order says so.
    local.shuffled = looksShuffled(local.queue, typeof m.index === 'number' ? m.index : -1);
  }
  local.index = typeof m.index === 'number' ? m.index : -1;
  local.paused = m.paused !== false;
  local.duration = typeof m.durationMs === 'number' && m.durationMs > 0 ? m.durationMs / 1000 : null;
  local.error = typeof m.error === 'string' ? m.error : null;
  reported = { positionMs: Number(m.positionMs) || 0, at: performance.now() };
  local.position = reported.positionMs / 1000;
}

function sleepOf(s: unknown): Sleep | null {
  if (!s || typeof s !== 'object') return null;
  const { atMs, endOfTrack } = s as Record<string, unknown>;
  const timer = { atMs: typeof atMs === 'number' ? atMs : null, endOfTrack: endOfTrack === true };
  return timer.atMs === null && !timer.endOfTrack ? null : timer;
}

/** A reported radio: `{on, mode, seed}` from the app, a mode or null from Cratarr's player view. */
function radioOf(r: unknown): Radio | null {
  if (typeof r === 'string') return RADIO_MODES.includes(r as RadioMode) ? { mode: r as RadioMode, seed: [] } : null;
  if (!r || typeof r !== 'object') return null;
  const { on, mode, seed, within } = r as Record<string, unknown>;
  if (on === false) return null;
  return {
    mode: RADIO_MODES.includes(mode as RadioMode) ? (mode as RadioMode) : 'similar',
    seed: Array.isArray(seed) ? seed.filter((s): s is string => typeof s === 'string') : [],
    within: typeof within === 'string' && within ? within : null,
  };
}

/** The sleep timer, for the player that plays elsewhere (the app, Cratarr's player), which keeps it: it is sure to be
 *  awake when it fires. `null` clears it. Its reports set `local.sleep`. */
export function handSleep(s: Sleep | null) {
  if (!handed()) return;
  local.sleep = s;
  hand('sleep', { atMs: s?.atMs ?? null, endOfTrack: s?.endOfTrack ?? false });
}

/** A call for the player that plays: the app's own, or the one driven (its command, as /api/players takes it). */
function hand(op: string, args: Record<string, unknown> = {}) {
  if (driven === null) return toApp({ type: 'player', op, ...args });
  const command: Record<string, unknown> =
    op === 'queue'
      ? { command: 'play', tracks: args.tracks, index: args.index, position_ms: args.positionMs }
      : op === 'seek'
        ? { command: 'seek', position_ms: args.positionMs }
        : op === 'play'
          ? { command: 'resume' }
          : op === 'sleep'
            ? { command: 'sleep', at_ms: args.atMs, ...(args.endOfTrack ? { end_of_track: true } : {}) }
            : { command: op, ...args };
  playerCommand(driven, command).catch((e) => (local.error = e instanceof Error ? e.message : String(e)));
}

if (inApp) onApp('player', (m) => driven === null && reportedBy(m as Parameters<typeof reportedBy>[0]));
if (typeof window !== 'undefined') {
  setInterval(() => {
    if (!handed() || local.paused || local.index < 0) return;
    const p = (reported.positionMs + performance.now() - reported.at) / 1000;
    local.position = local.duration ? Math.min(p, local.duration) : p;
  }, 250);
}

// --- what the player and the lock screen ask of it ---

/** The tracks added by hand still to come: the run of them right after the playing track (decision 0033). */
export function addedAhead(queue: LocalTrack[], index: number): LocalTrack[] {
  const run: LocalTrack[] = [];
  for (let i = index + 1; i < queue.length && queue[i].lane === 'next'; i++) run.push(queue[i]);
  return run;
}

/** The album's or playlist's own order kept on its tracks (`n`), for turning shuffle off later. */
const numbered = (tracks: LocalTrack[]) => tracks.map((t, i) => (t.lane || t.n !== undefined ? t : { ...t, n: i }));

/** In a random order (Fisher-Yates). */
function shuffledCopy<T>(items: T[]): T[] {
  const a = [...items];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

/** What comes of the album or playlist in its own order again, from the playing track on; those from before it last,
 *  so nothing is lost. Tracks without a place keep theirs, at the end. */
function inOrder(rest: LocalTrack[], from: number): LocalTrack[] {
  const placed = rest.filter((t) => t.n !== undefined).sort((a, b) => a.n! - b.n!);
  return [...placed.filter((t) => t.n! > from), ...placed.filter((t) => t.n! <= from), ...rest.filter((t) => t.n === undefined)];
}

/** The place in the album or playlist the queue has reached: the playing track's, else the last one's before it. */
function reached(queue: LocalTrack[], index: number): number {
  for (let i = index; i >= 0; i--) if (queue[i]?.n !== undefined) return queue[i].n!;
  return -1;
}

/** Whether what comes of the album or playlist is out of its own order: more than the one turn back that starting
 *  partway in gives. */
function looksShuffled(queue: LocalTrack[], index: number): boolean {
  const ns = queue.slice(index + 1).flatMap((t) => (t.lane || t.n === undefined ? [] : [t.n]));
  let back = 0;
  for (let i = 1; i < ns.length; i++) if (ns[i] < ns[i - 1]) back++;
  return back > 1;
}

/** Shuffle on (what comes of the album or playlist, mixed) or off (back in its own order, from the playing track on).
 *  What was added by hand stays ahead, in its order (decision 0033). */
export function setShuffle(on: boolean) {
  local.shuffled = on;
  if (local.index < 0) return;
  // A player of Cratarr's: Cratarr orders its queue (it holds it, decision 0033), not this page from what it last saw.
  if (driven !== null) return hand('shuffle', { on });
  const ahead = addedAhead(local.queue, local.index);
  const after = local.queue.slice(local.index + 1 + ahead.length);
  // The radio's tracks are not the album's or playlist's own: they stay last, in their order (decision 0036).
  const rest = after.filter((t) => t.lane !== 'radio');
  const radio = after.filter((t) => t.lane === 'radio');
  replaceUpcoming([...ahead, ...(on ? shuffledCopy(rest) : inOrder(rest, reached(local.queue, local.index))), ...radio]);
}

/** Repeat off, one or all: the player that plays applies it (here, the app, or Cratarr's player). Repeat all turns the
 *  radio off: both say what comes when the queue ends (decision 0036). */
export function setRepeat(mode: Repeat) {
  local.repeat = mode;
  if (mode === 'all' && local.radio) radioOff();
  if (handed()) return hand('repeat', { mode });
  nextRound();
}

/** The radio on (a mode; a seed of track ids when started from an item, else the queue's own) or off (its tracks to
 *  come go). The player that plays fills its lane: this page for its own stream, else the app or Cratarr's player. On
 *  turns repeat all off. */
export function setRadio(on: boolean, mode?: RadioMode, seed?: string[], within?: string | null) {
  if (!on) {
    radioOff();
    if (handed()) hand('radio', { on: false });
    return;
  }
  const inside = within === undefined ? (local.radio?.within ?? null) : within;
  local.radio = { mode: mode ?? local.radio?.mode ?? 'similar', seed: seed ?? local.radio?.seed ?? [], within: inside };
  if (local.repeat === 'all') local.repeat = 'off';
  // The seed only when given: the player keeps the one it has (a player's view does not show it here).
  if (handed()) return hand('radio', { on: true, mode: local.radio.mode, ...(seed ? { seed } : {}), ...(within !== undefined ? { within: within ?? '' } : {}) });
  // Another mode: what it chose before makes way for what this one chooses.
  const kept = local.queue.slice(local.index + 1).filter((t) => t.lane !== 'radio');
  if (local.index >= 0 && kept.length !== local.queue.length - local.index - 1) replaceUpcoming(kept);
  fillRadio();
}

/** The radio off here: its tracks still to come out (the player that plays is told by the caller). */
function radioOff() {
  local.radio = null;
  if (local.index < 0) return;
  const kept = local.queue.slice(local.index + 1).filter((t) => t.lane !== 'radio');
  if (kept.length === local.queue.length - local.index - 1) return;
  if (handed()) local.queue = [...local.queue.slice(0, local.index + 1), ...kept];
  else replaceUpcoming(kept);
}

/** Radio from an item ("Start radio", decision 0036): tracks like `seed` (its own left out) play, and the radio goes
 *  on with that seed. `within`: a crate's id, to play only the crate's tracks ("Play as radio"; its own then may play).
 *  `from`: its name, for the queue sheet. */
export async function startRadio(seed: string[], from: string | null, mode: RadioMode = local.radio?.mode ?? 'similar', within: string | null = null) {
  const { tracks } = await radioNext({ mode, seed, exclude: within ? [] : seed, last: null, count: RADIO_BATCH, within });
  if (!tracks.length) throw new Error('The radio found nothing to play.');
  playQueue(tracks as LocalTrack[], 0, from);
  setRadio(true, mode, seed, within);
}

/** How many radio tracks are asked for at a time, and how few may be left ahead before more are. */
const RADIO_BATCH = 5;
const RADIO_LOW = 3;
let filling = false;

/** This page's own stream with the radio on: more of the radio's tracks when fewer than three are left ahead. */
function fillRadio() {
  const radio = local.radio;
  if (!radio || handed() || filling || local.index < 0 || local.queue.length - local.index - 1 >= RADIO_LOW) return;
  const own = local.queue.filter((t) => !t.lane).map((t) => t.id);
  const ask = {
    mode: radio.mode,
    seed: radio.seed.length ? radio.seed : own,
    exclude: local.queue.map((t) => t.id),
    last: local.queue[local.index]?.id ?? null,
    count: RADIO_BATCH,
    within: radio.within ?? null,
  };
  filling = true;
  radioNext(ask)
    .then(({ tracks }) => {
      // Still on, still here: appended after everything, so the stream goes on into them.
      if (local.radio && !handed() && tracks.length) local.queue = [...local.queue, ...(tracks as LocalTrack[])];
    })
    .catch((e) => (local.error = e instanceof Error ? e.message : String(e)))
    .finally(() => (filling = false));
}

/** Under repeat all, while the last track plays here: the next round of the album or playlist after it (decision 0033,
 *  amended: those added by hand play once; shuffled anew when the queue plays shuffled, not starting with the track
 *  that just played). The stream then goes on into it without a gap. */
function nextRound() {
  if (local.repeat !== 'all' || local.index < 0 || local.index !== local.queue.length - 1) return;
  const byPlace = new Map<number, LocalTrack>();
  for (const t of local.queue) if (!t.lane && t.n !== undefined) byPlace.set(t.n, t);
  let round = byPlace.size
    ? [...byPlace.entries()].sort((a, b) => a[0] - b[0]).map(([, t]) => t)
    : local.queue.filter((t, i, all) => !t.lane && all.findIndex((x) => x.source === t.source && x.id === t.id) === i);
  if (!round.length) return;
  if (local.shuffled) {
    round = shuffledCopy(round);
    const last = local.queue[local.index];
    if (round.length > 1 && round[0].source === last.source && round[0].id === last.id) round.push(round.shift()!);
  }
  // New entries for the new round's tracks; their places stay.
  local.queue = [...local.queue, ...round.map(({ entry: _, ...t }) => t)];
}

/** Everything after the playing track, replaced; the playing track plays on. */
function replaceUpcoming(upcoming: LocalTrack[]) {
  local.queue = [...local.queue.slice(0, local.index + 1), ...upcoming];
  if (handed()) return hand('upcoming', { tracks: upcoming, shuffled: local.shuffled });
  cutFrom(local.index + 1);
}

/** Plays `tracks` from `index`, `position` seconds into it; `shuffle`: all of them mixed, from the first. Something new
 *  replaces the album or playlist that played, not what was added by hand: those come right after its first track, as
 *  before (decision 0033's two lanes). */
export function playQueue(tracks: LocalTrack[], index = 0, from: string | null = null, position = 0, shuffle = false) {
  tracks = numbered(tracks);
  if (shuffle) [tracks, index, position] = [shuffledCopy(tracks), 0, 0];
  // A queue moved here whole keeps its shuffle.
  local.shuffled = shuffle || looksShuffled(tracks, index);
  // Something new turns repeat one off; repeat all stays (decision 0033, amended).
  if (local.repeat === 'one') local.repeat = 'off';
  // The radio stays on, after the new album or playlist, which is its seed now; radio tracks only are a radio starting
  // from an item, which brings its own seed (decision 0036).
  if (local.radio && !tracks.every((t) => t.lane === 'radio')) local.radio = { ...local.radio, seed: [], within: null };
  // Those the new queue holds already (a queue moved here whole) are not added twice.
  const holds = (t: LocalTrack) => tracks.some((x) => x === t || (!!t.entry && x.entry === t.entry));
  const ahead = local.index < 0 ? [] : addedAhead(local.queue, local.index).filter((t) => !holds(t));
  if (ahead.length) tracks = [...tracks.slice(0, index + 1), ...ahead, ...tracks.slice(index + 1)];
  local.queue = tracks;
  local.from = from;
  if (handed()) {
    Object.assign(local, { index, position, paused: false, error: null });
    reported = { positionMs: position * 1000, at: performance.now() };
    return hand('queue', { tracks, index, positionMs: Math.round(position * 1000), play: true, shuffled: local.shuffled });
  }
  fresh(index, position);
  fillRadio();
}

/** Adds tracks by hand (decision 0033's two lanes): right after the playing one (`next`, so the newest come first), or
 *  after those added before (`last`), ahead of the rest of the album or playlist either way. */
export function enqueue(tracks: LocalTrack[], how: 'next' | 'last') {
  tracks = tracks.map((t) => ({ ...t, lane: 'next' as const }));
  if (local.index < 0) return playQueue(tracks);
  const at = local.index + 1 + (how === 'next' ? 0 : addedAhead(local.queue, local.index).length);
  const after = at === 0 ? '' : local.queue[at - 1].entry;
  local.queue = [...local.queue.slice(0, at), ...tracks, ...local.queue.slice(at)];
  if (handed()) return hand('add', { at, tracks, ...(after === undefined ? {} : { after }) });
  cutFrom(at);
}

/** Moves upcoming track `from` to place `to` (queue indexes, both after the playing track): the queue sheet's drag. */
export function move(from: number, to: number) {
  const n = local.queue.length;
  if (from === to || from <= local.index || to <= local.index || from >= n || to >= n) return;
  const q = [...local.queue];
  const [t] = q.splice(from, 1);
  q.splice(to, 0, t);
  local.queue = q;
  // By entry where the tracks have them (decision 0033): the player's queue may have moved on since.
  const after = to === 0 ? '' : q[to - 1].entry;
  if (handed()) return hand('move', { from, to, ...(t.entry && after !== undefined ? { entry: t.entry, after } : {}) });
  cutFrom(Math.min(from, to));
}

/** Takes upcoming track `index` (a queue index after the playing track) out of the queue: the queue sheet's swipe. */
export function remove(index: number) {
  if (index <= local.index || index >= local.queue.length) return;
  const entry = local.queue[index].entry;
  local.queue = local.queue.filter((_, i) => i !== index);
  if (handed()) return hand('remove', { index, ...(entry ? { entry } : {}) });
  cutFrom(index);
}

/** As `remove`, counted in what comes next. */
export const removeUpcoming = (i: number) => remove(local.index + 1 + i);

/** As `move`, counted in what comes next (0 = the track after the playing one), as the queue sheet lists it. */
export const moveUpcoming = (from: number, to: number) => move(local.index + 1 + from, local.index + 1 + to);

/** The queue changed from index `at` on: what the stream holds (or is fetching) from there is no longer what comes
 *  next. It is dropped, and the tick appends the new next track gaplessly after the last one kept. */
function cutFrom(at: number) {
  if (appending && appendingIndex >= at) {
    fetching?.abort();
    appending = false;
  }
  if (at > lastAppended) return;
  const keep = segments.filter((s) => s.index < at);
  // An ended stream counts too (the queue's last track was in): the removal below reopens it.
  if (!keep.length || !sb || !ms || ms.readyState === 'closed') return;
  const last = keep[keep.length - 1];
  const end = last.start + (last.length ?? 0);
  segments = keep;
  streamEnd = end;
  lastAppended = at - 1;
  (async () => {
    // An append that was under way finishes first; the aborted one adds nothing after it.
    while (sb?.updating) await waitUpdate();
    try {
      sb?.remove(end, Infinity);
    } catch {
      /* nothing appended there */
    }
  })();
}

export function togglePlay() {
  if (handed()) {
    local.paused = !local.paused;
    return hand('toggle');
  }
  if (!audio) return;
  if (audio.paused) audio.play().catch(() => {});
  else audio.pause();
}

/** To queue track `index`: within the stream when it is already appended (gapless), else a fresh stream. */
export function jump(index: number) {
  if (handed() && index >= 0 && index < local.queue.length) {
    const entry = local.queue[index].entry;
    Object.assign(local, { index, position: 0, paused: false });
    return hand('jump', { index, ...(entry ? { entry } : {}) });
  }
  if (!audio || index < 0 || index >= local.queue.length) return;
  const seg = segments.find((s) => s.index === index);
  if (seg && audio.buffered.length && seg.start >= audio.buffered.start(0)) {
    audio.currentTime = seg.start;
    local.index = index;
    if (audio.paused) audio.play().catch(() => {});
  } else {
    fresh(index, 0, true);
  }
}

export const next = () => jump(local.index + 1);

export function previous() {
  if (handed()) return hand('previous');
  // Repeat all: previous on the first track goes to the last.
  if (local.repeat === 'all' && local.index === 0 && local.position <= RESTART_AFTER && local.queue.length > 1) return jump(local.queue.length - 1);
  if (local.position > RESTART_AFTER) seek(0);
  else jump(Math.max(0, local.index - 1));
}

/** To `secs` into the playing track: within what is loaded, else a fresh stream from there. */
export function seek(secs: number) {
  if (handed()) {
    local.position = secs;
    reported = { positionMs: secs * 1000, at: performance.now() };
    return hand('seek', { positionMs: Math.round(secs * 1000) });
  }
  if (!audio) return;
  const w = where();
  const target = w ? w.seg.start + secs : secs;
  for (let i = 0; i < audio.buffered.length; i++) {
    if (target >= audio.buffered.start(i) && target <= audio.buffered.end(i)) {
      audio.currentTime = target;
      return;
    }
  }
  fresh(local.index, secs, !audio.paused);
}

/** Stops and forgets the queue (switching output to elsewhere). Returns where it was, for the handover. */
export function stop(): { queue: LocalTrack[]; index: number; position: number } {
  const was = { queue: local.queue, index: local.index, position: local.position };
  if (handed()) hand('stop');
  generation++;
  fetching?.abort();
  audio?.pause();
  if (audio) audio.removeAttribute('src');
  local.queue = [];
  local.index = -1;
  local.paused = true;
  return was;
}

if (typeof navigator !== 'undefined' && 'mediaSession' in navigator) {
  const ms_ = navigator.mediaSession;
  ms_.setActionHandler('play', () => audio?.play());
  ms_.setActionHandler('pause', () => audio?.pause());
  ms_.setActionHandler('nexttrack', () => next());
  ms_.setActionHandler('previoustrack', () => previous());
  try {
    ms_.setActionHandler('seekto', (d) => d.seekTime !== undefined && seek(d.seekTime));
  } catch {
    /* older browsers */
  }
}
