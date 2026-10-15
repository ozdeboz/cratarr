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

export type LocalTrack = {
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  /** False for a track outside the Music library (Incoming): it cannot go in a playlist. */
  addable?: boolean;
};

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
/** Whether this browser can play here at all (FLAC or MP3 in its stream). */
export const canPlayHere = !!Source && (Source.isTypeSupported(MP4) || Source.isTypeSupported(MPEG));

const audio = typeof Audio === 'undefined' || !canPlayHere ? null : new Audio();
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
  if (!audio || local.index < 0) return;
  local.paused = audio.paused;
  const w = where();
  if (w) {
    if (w.seg.index !== local.index) local.index = w.seg.index;
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

// --- what the player and the lock screen ask of it ---

/** Plays `tracks` from `index`. */
export function playQueue(tracks: LocalTrack[], index = 0, from: string | null = null) {
  local.queue = tracks;
  local.from = from;
  fresh(index, 0);
}

/** Adds tracks after the playing one (`next`) or at the end (`last`). */
export function enqueue(tracks: LocalTrack[], how: 'next' | 'last') {
  if (local.index < 0) return playQueue(tracks);
  const at = how === 'next' ? local.index + 1 : local.queue.length;
  local.queue = [...local.queue.slice(0, at), ...tracks, ...local.queue.slice(at)];
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
  cutFrom(Math.min(from, to));
}

/** Takes upcoming track `index` (a queue index after the playing track) out of the queue: the queue sheet's swipe. */
export function remove(index: number) {
  if (index <= local.index || index >= local.queue.length) return;
  local.queue = local.queue.filter((_, i) => i !== index);
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
  if (!audio) return;
  if (audio.paused) audio.play().catch(() => {});
  else audio.pause();
}

/** To queue track `index`: within the stream when it is already appended (gapless), else a fresh stream. */
export function jump(index: number) {
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
  if (local.position > RESTART_AFTER) seek(0);
  else jump(Math.max(0, local.index - 1));
}

/** To `secs` into the playing track: within what is loaded, else a fresh stream from there. */
export function seek(secs: number) {
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
