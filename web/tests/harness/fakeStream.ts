// A browser's audio element and MediaSource stood in for, with a server's /api/stream, so that lib/localPlayer's real
// stream runs in a test (tests/isolated: a process of its own, as this fills in the page's globals). Its rules are the
// media specs': a SourceBuffer takes one change at a time, an append lands at its time and over what lay there, a removal
// reopens an ended stream, and an element reaching the end of what it holds waits unless the stream has ended. The
// buffer remembers whose sound each stretch is, so a test asks what is heard, not what the player believes. Time moves
// only in `runFor`, which also runs the player's clock.
import './pageGlobals.ts';

/** One byte of a track's stream, in seconds of it; and how much of it comes at a time. */
const BYTE_S = 0.1;
const CHUNK_S = 10;
/** The player's clock runs every 250 ms. */
const STEP_S = 0.25;
/** Stretches closer than this are one range, as a browser's are. */
const JOIN_S = 0.001;

/** Bumped by anything the fakes do: `settle` waits for it to stand still. */
let activity = 0;

type Piece = { start: number; end: number; track: string; media: number };

/** Whose sound each chunk is, and from where in its track. */
const chunks = new WeakMap<ArrayBufferLike, { track: string; from: number }>();

class Ranges {
  list: [number, number][];
  constructor(list: [number, number][]) {
    this.list = list;
  }
  get length() {
    return this.list.length;
  }
  start(i: number) {
    return this.list[i][0];
  }
  end(i: number) {
    return this.list[i][1];
  }
}

class FakeBuffer extends EventTarget {
  updating = false;
  mode = 'segments';
  timestampOffset = 0;
  appendWindowStart = 0;
  appendWindowEnd = Infinity;
  pieces: Piece[] = [];
  source: FakeSource;
  constructor(source: FakeSource) {
    super();
    this.source = source;
  }
  get buffered() {
    const sorted = [...this.pieces].sort((a, b) => a.start - b.start);
    const out: [number, number][] = [];
    for (const p of sorted) {
      const last = out[out.length - 1];
      if (last && p.start <= last[1] + JOIN_S) last[1] = Math.max(last[1], p.end);
      else out.push([p.start, p.end]);
    }
    return new Ranges(out);
  }
  private busy() {
    activity++;
    this.updating = true;
    setImmediate(() => {
      activity++;
      this.updating = false;
      this.dispatchEvent(new Event('updateend'));
    });
  }
  private clear(from: number, to: number) {
    this.pieces = this.pieces.flatMap((p) => {
      if (p.end <= from || p.start >= to) return [p];
      const kept: Piece[] = [];
      if (p.start < from) kept.push({ ...p, end: from });
      if (p.end > to) kept.push({ ...p, start: to, media: p.media + (to - p.start) });
      return kept;
    });
  }
  appendBuffer(bytes: Uint8Array) {
    if (this.updating) throw new DOMException('still updating', 'InvalidStateError');
    const from = chunks.get(bytes.buffer);
    if (!from) throw new Error('bytes that came from no stream');
    const media = from.from + bytes.byteOffset * BYTE_S;
    const at = media + this.timestampOffset;
    const start = Math.max(at, this.appendWindowStart);
    const end = Math.min(at + bytes.length * BYTE_S, this.appendWindowEnd);
    if (end > start) {
      this.clear(start, end);
      this.pieces.push({ start, end, track: from.track, media: media + (start - at) });
    }
    this.busy();
  }
  remove(from: number, to: number) {
    if (this.updating) throw new DOMException('still updating', 'InvalidStateError');
    if (from > this.source.duration) throw new TypeError('removal past the stream');
    this.clear(from, to);
    if (this.source.readyState === 'ended') this.source.readyState = 'open';
    this.busy();
  }
  abort() {
    if (this.updating) throw new DOMException('still updating', 'InvalidStateError');
  }
  changeType() {}
}

class FakeSource extends EventTarget {
  static isTypeSupported = (type: string) => type.includes('flac') || (server.opus && type.includes('opus'));
  readyState = 'closed';
  buffer: FakeBuffer | null = null;
  get duration() {
    const b = this.buffer?.buffered;
    return b?.length ? b.end(b.length - 1) : 0;
  }
  addSourceBuffer() {
    if (this.readyState !== 'open') throw new DOMException('not open', 'InvalidStateError');
    this.buffer = new FakeBuffer(this);
    return this.buffer;
  }
  endOfStream() {
    if (this.readyState !== 'open' || this.buffer?.updating) throw new DOMException('not now', 'InvalidStateError');
    activity++;
    this.readyState = 'ended';
  }
}

const sources = new Map<string, FakeSource>();
let urls = 0;
URL.createObjectURL = ((m: FakeSource) => {
  const url = `blob:stream/${++urls}`;
  sources.set(url, m);
  return url;
}) as typeof URL.createObjectURL;
URL.revokeObjectURL = () => {};

class FakeAudio extends EventTarget {
  paused = true;
  playbackRate = 1;
  volume = 1;
  preservesPitch = true;
  error = null;
  ended = false;
  private time = 0;
  private source: FakeSource | null = null;
  private waiting = false;
  /** Sound flowing since its last start: `playing` said. */
  private sounding = false;
  get currentTime() {
    return this.time;
  }
  set currentTime(t: number) {
    this.time = t;
    this.ended = false;
    this.waiting = false;
    activity++;
  }
  get buffered() {
    return this.source?.buffer?.buffered ?? new Ranges([]);
  }
  set src(url: string) {
    if (this.source) this.source.readyState = 'closed';
    const source = sources.get(url)!;
    this.source = source;
    this.time = 0;
    this.ended = false;
    this.waiting = false;
    // The load algorithm: paused again, with no event.
    this.paused = true;
    setImmediate(() => {
      activity++;
      if (this.source !== source) return;
      source.readyState = 'open';
      source.dispatchEvent(new Event('sourceopen'));
    });
  }
  removeAttribute() {
    if (this.source) this.source.readyState = 'closed';
    this.source = null;
  }
  play() {
    if (this.ended) this.currentTime = 0;
    if (this.paused) {
      this.paused = false;
      this.sounding = false;
      this.dispatchEvent(new Event('play'));
    }
    return Promise.resolve();
  }
  pause() {
    if (this.paused) return;
    this.paused = true;
    this.sounding = false;
    this.dispatchEvent(new Event('pause'));
  }
  /** Whose sound is at the playing position, and how far into its track; null where nothing is held. */
  heard(): { track: string; at: number } | null {
    const p = this.source?.buffer?.pieces.find((x) => this.time >= x.start - JOIN_S && this.time < x.end - JOIN_S);
    return p ? { track: p.track, at: p.media + (this.time - p.start) } : null;
  }
  /** The end of the range the playing position is in; null where it is in none. */
  private rangeEnd() {
    const b = this.buffered;
    for (let i = 0; i < b.length; i++) if (this.time >= b.start(i) - JOIN_S && this.time <= b.end(i) + JOIN_S) return b.end(i);
    return null;
  }
  /** `s` seconds of playing: on through what is held, to its end. */
  advance(s: number) {
    if (this.paused) return;
    const end = this.rangeEnd() ?? this.time;
    if (end > this.time + JOIN_S && !this.sounding) {
      this.sounding = true;
      this.waiting = false;
      this.dispatchEvent(new Event('playing'));
    }
    this.time = Math.min(this.time + s * this.playbackRate, end);
    this.dispatchEvent(new Event('timeupdate'));
    if (this.time < end - JOIN_S) return;
    if (this.source?.readyState === 'ended') {
      this.ended = true;
      this.paused = true;
      this.dispatchEvent(new Event('pause'));
      this.dispatchEvent(new Event('ended'));
    } else if (!this.waiting) {
      this.waiting = true;
      this.sounding = false;
      this.dispatchEvent(new Event('waiting'));
    }
  }
}

/** What a request for a track gives: `gives` seconds from where it starts, then it `breaks` (a read error), `dies` (the same,
 *  and Cratarr with it: `server.away`), `hangs` (a server that says no more, the connection open) or `ends`. No plan: all
 *  of it. */
export type Plan = { gives: number; then: 'breaks' | 'dies' | 'hangs' | 'ends' };

export const server = {
  /** Each track's length (s). */
  lengths: new Map<string, number>(),
  /** The plans for a track's next requests, in turn. */
  plans: new Map<string, Plan[]>(),
  /** Every request for a track: which, and from where (s). */
  asked: [] as { track: string; from: number }[],
  /** The same requests' addresses, as the player wrote them. */
  addresses: [] as string[],
  /** What Autoplay's next ask is answered with. */
  picks: [] as unknown[],
  /** Not reached: every request fails to start, but for a track kept on the device. */
  away: false,
  /** What the service worker keeps: these come whatever else does. */
  kept: new Set<string>(),
  /** Every stream's answer in place of its sound (a proxy's); null: its sound. */
  answer: null as { status: number; body?: string } | null,
  /** A track's own, in the words Cratarr gives. */
  refused: new Map<string, string>(),
  /** The browser takes Opus too, so a track has a form to fall back to. */
  opus: false,
};

function stream(track: string, from: number, signal: AbortSignal | undefined) {
  const length = server.lengths.get(track) ?? 0;
  const plan = server.plans.get(track)?.shift();
  const stop = Math.min(length, plan ? from + plan.gives : length);
  let at = from;
  return new ReadableStream<Uint8Array>(
    {
      start(c) {
        signal?.addEventListener('abort', () => c.error(new DOMException('Aborted', 'AbortError')));
      },
      pull(c) {
        activity++;
        if (at >= stop) {
          if (!plan || at >= length || plan.then === 'ends') return c.close();
          if (plan.then === 'dies') server.away = true;
          if (plan.then === 'breaks' || plan.then === 'dies') return c.error(new TypeError('network error'));
          return new Promise<void>(() => {});
        }
        const secs = Math.min(CHUNK_S, stop - at);
        const bytes = new Uint8Array(Math.round(secs / BYTE_S));
        chunks.set(bytes.buffer, { track, from: at });
        at += secs;
        c.enqueue(bytes);
      },
    },
    { highWaterMark: 0 },
  );
}

globalThis.fetch = (async (input: string, init?: RequestInit) => {
  activity++;
  const url = new URL(String(input), 'http://cratarr.test');
  const asked = url.pathname.match(/^\/api\/stream\/[^/]+\/([^/]+)$/);
  if (!asked) {
    if (server.away) throw new TypeError('Failed to fetch');
    const picks = url.pathname.includes('radio') ? server.picks.splice(0) : [];
    return new Response(JSON.stringify({ tracks: picks }), { headers: { 'content-type': 'application/json' } });
  }
  const track = decodeURIComponent(asked[1]);
  const from = Number(url.searchParams.get('from_ms') ?? 0) / 1000;
  server.asked.push({ track, from });
  server.addresses.push(String(input));
  if (!server.kept.has(track)) {
    if (server.away) throw new TypeError('Failed to fetch');
    if (server.answer) return new Response(server.answer.body ?? '', { status: server.answer.status });
    const refused = server.refused.get(track);
    if (refused) return new Response(JSON.stringify({ error: refused }), { status: 404 });
  }
  const length = server.lengths.get(track) ?? 0;
  return new Response(stream(track, from, init?.signal ?? undefined), {
    headers: { 'content-type': 'audio/mp4; codecs="flac"', 'x-sample-rate': '1000', 'x-total-samples': String(length * 1000), 'x-duration-ms': String(length * 1000) },
  });
}) as typeof fetch;

const g = globalThis as Record<string, unknown>;
g.Audio = FakeAudio;
g.MediaSource = FakeSource;

// The player's clock is `runFor`'s, as nothing else moves time here; every other interval is left unstarted.
let tick = () => {};
g.setInterval = (run: () => void) => {
  if (run.name === 'tick') tick = run;
  return 0;
};

const player = await import('../../src/lib/localPlayer.svelte.ts');
export { player };
export const audio = player.engineAudio() as unknown as FakeAudio;

// The player's waits go by the clock on the wall, which here is `runFor`'s too.
const wall = Date.now;
let passed = 0;
Date.now = () => wall() + passed;

/** Everything under way done: the fetches read, the appends in, the promises run out. */
export async function settle() {
  let seen = -1;
  for (let quiet = 0; quiet < 4; ) {
    await new Promise((go) => setImmediate(go));
    quiet = seen === activity ? quiet + 1 : 0;
    seen = activity;
  }
}

/** `seconds` of time: the element plays on, the player's clock runs, and what that sets going is done. */
export async function runFor(seconds: number) {
  for (let left = seconds; left > 1e-9; left -= STEP_S) {
    audio.advance(Math.min(STEP_S, left));
    passed += Math.min(STEP_S, left) * 1000;
    tick();
    await settle();
  }
}

/** A track of the server's, `secs` long. */
export function track(id: string, secs: number) {
  server.lengths.set(id, secs);
  return { source: 'cratarr', id, name: id, artist: null, album: null, album_id: null, duration_secs: secs };
}

/** A new test: nothing playing, nothing planned. */
export async function fromRest() {
  player.stop();
  Object.assign(player.local, { repeat: 'off', autoplay: null, radio: false, error: null });
  Object.assign(server, { away: false, answer: null, opus: false, asked: [], addresses: [], picks: [] });
  server.plans.clear();
  server.kept.clear();
  server.refused.clear();
  await settle();
}
