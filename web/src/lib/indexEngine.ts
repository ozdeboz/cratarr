// The rules crate's index in WebAssembly, as the page holds it (index.wasm; server/rules/src/wasm.rs says what each call does): what a
// device keeps is bytes, and this is what reads them. Bytes and JSON go over the module's memory, as the matcher's do. An instance
// holds one index and one set of sounds at most, and its memory never shrinks, so whoever is done lets the whole instance go. What
// it says of when to fetch is the same rule the app's worker asks (`index::sync`). Pure: lib/deviceIndex fetches the file.
import type { ArtistRows, FoundRows, ListRows, RecordRows } from './deviceReads';

/** Why bytes were not taken. Whatever was held stays as it was. */
export type Refused = { why: 'version'; found: number; wants: number } | { why: 'damaged' | 'rev' | 'unknown' };

/** What a fetch costs the person, as the device can tell. */
export type Network = 'free' | 'metered' | 'off';
/** What the person said of a whole fetch on mobile data. */
export type Mobile = 'unasked' | 'yes' | 'no';
/** How one part of what a device keeps stands (`index::sync::Part`): `tried` null before a fetch, `started` when this start began. */
export type Part = { whole: boolean; stopped: boolean; lost: number; failed: number; tried: number | null; started: number };
/** What to do now. */
export type Step = { do: 'stopped' | 'rest' | 'ask' | 'whole' | 'changes' } | { do: 'wait'; until: number };
/** What a search asks (`index::find::Ask`). `kinds`: the module's, none for all of them. */
export type FindAsk = { q: string; kinds: string[]; limit: number; offset: number; without: string[] };
/** A part of what a device keeps, as the module numbers it. */
const KIND = { index: 0, sounds: 1, covers: 2, own: 3 } as const;
export type Kind = keyof typeof KIND;
/** How a fetch ended, or a new start: `start-over-another` where another version than this one's kept what is there. */
export type Outcome = 'took' | 'lost' | 'failed' | 'start-again' | 'version' | 'start' | 'start-over-another';

type Exports = {
  memory: WebAssembly.Memory;
  index_alloc(len: number): number;
  index_free(ptr: number, len: number): void;
  index_load(ptr: number, len: number): number;
  index_apply(ptr: number, len: number): number;
  index_save(): number;
  index_rev(): number;
  index_tracks(): number;
  index_list(): number;
  index_record(id: number): number;
  index_find(ptr: number, len: number): number;
  index_artist(ptr: number, len: number): number;
  sounds_load(ptr: number, len: number): number;
  sounds_apply(ptr: number, len: number): number;
  sounds_save(): number;
  sounds_rev(): number;
  marks_load(ptr: number, len: number): number;
  marks_at(): number;
  device_open(): number;
  device_ask(ptr: number, len: number): number;
  index_version(): number;
  sounds_version(): number;
  marks_version(): number;
  refused_found(): number;
  refused_wants(): number;
  sync_answer(status: number, whole: number): number;
  sync_take_whole(kind: number, ptr: number, len: number): number;
  sync_take_changes(kind: number, kept: number, keptLen: number, feed: number, feedLen: number): number;
  covers_version(): number;
  covers_rev_taken(): number;
  sync_step(whole: number, stopped: number, lost: number, failed: number, tried: number, started: number, network: number, mobile: number, told: number, now: number): number;
  sync_step_stale(whole: number, stopped: number, lost: number, failed: number, tried: number, started: number, network: number, mobile: number, took: number, now: number): number;
  sync_after(whole: number, stopped: number, lost: number, failed: number, outcome: number): number;
  sync_paused(lost: number): number;
  sync_first_after(): number;
  sync_anew_after(): number;
};

const NETWORK: Network[] = ['free', 'metered', 'off'];
const MOBILE: Mobile[] = ['unasked', 'yes', 'no'];
const OUTCOME: Outcome[] = ['took', 'lost', 'failed', 'start-again', 'version', 'start', 'start-over-another'];
const STEP: Record<number, 'stopped' | 'rest' | 'ask' | 'whole' | 'changes'> = { [-1]: 'stopped', [-2]: 'rest', [-3]: 'ask', [-4]: 'whole', [-5]: 'changes' };
const decoder = new TextDecoder();
const encoder = new TextEncoder();

export class IndexEngine {
  private readonly x: Exports;
  constructor(x: Exports) {
    this.x = x;
  }

  /** `call` over `bytes` written into the module. */
  private over(bytes: Uint8Array, call: (ptr: number, len: number) => number): number {
    const ptr = this.x.index_alloc(bytes.length);
    // Read the memory after the alloc: it may have grown, and its buffer been replaced.
    new Uint8Array(this.x.memory.buffer, ptr, bytes.length).set(bytes);
    const said = call(ptr, bytes.length);
    this.x.index_free(ptr, bytes.length);
    return said;
  }

  /** The bytes of an answer, freed from the module: a length, then that many. Null for none. */
  private taken(ptr: number): Uint8Array | null {
    if (!ptr) return null;
    const length = new DataView(this.x.memory.buffer).getUint32(ptr, true);
    const bytes = new Uint8Array(this.x.memory.buffer, ptr + 4, length).slice();
    this.x.index_free(ptr, 4 + length);
    return bytes;
  }

  private refused(said: number): Refused | null {
    switch (said) {
      case 0:
        return null;
      case 1:
        return { why: 'version', found: this.x.refused_found(), wants: this.x.refused_wants() };
      case 3:
        return { why: 'rev' };
      case 4:
        return { why: 'unknown' };
      default:
        return { why: 'damaged' };
    }
  }

  private json<T>(ptr: number): T | null {
    const bytes = this.taken(ptr);
    return bytes && (JSON.parse(decoder.decode(bytes)) as T);
  }

  loadIndex = (bytes: Uint8Array) => this.refused(this.over(bytes, this.x.index_load));
  applyIndex = (bytes: Uint8Array) => this.refused(this.over(bytes, this.x.index_apply));
  saveIndex = () => this.taken(this.x.index_save());
  indexRev = () => this.x.index_rev();
  tracks = () => this.x.index_tracks();
  /** Every library and record held. */
  list = () => this.json<ListRows>(this.x.index_list());
  /** A record and its tracks, by the number in its id. */
  record = (id: number) => this.json<RecordRows>(this.x.index_record(id));
  /** What the words find, read inside the module: a library's tracks never come over. Null where no index is held. */
  find = (ask: FindAsk) => this.json<FoundRows>(this.over(encoder.encode(JSON.stringify(ask)), this.x.index_find));
  /** An artist as their panel shows them; null where nobody goes by the name. */
  artist = (name: string, without: string[]) => this.json<ArtistRows>(this.over(encoder.encode(JSON.stringify({ q: name, without })), this.x.index_artist));

  loadSounds = (bytes: Uint8Array) => this.refused(this.over(bytes, this.x.sounds_load));
  applySounds = (bytes: Uint8Array) => this.refused(this.over(bytes, this.x.sounds_apply));
  saveSounds = () => this.taken(this.x.sounds_save());
  soundsRev = () => this.x.sounds_rev();

  /** An account's own, for what a device answers for itself. */
  loadOwn = (bytes: Uint8Array) => this.refused(this.over(bytes, this.x.marks_load));
  /** When the account's own held were worked out (epoch s). */
  ownAt = () => this.x.marks_at();

  /** What a device answers for itself made ready over what is held: whether there was an index to do it on. */
  openDevice = () => this.x.device_open() === 0;

  /** What is asked of it, by `index::device`'s words; null where it has no answer. */
  ask<T>(asked: unknown): T | null {
    const bytes = encoder.encode(JSON.stringify(asked));
    const ptr = this.x.index_alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer, ptr, bytes.length).set(bytes);
    const answer = this.x.device_ask(ptr, bytes.length);
    this.x.index_free(ptr, bytes.length);
    return this.json<T>(answer);
  }

  /** How an answer ended a fetch, or `bytes` where it brought some to take. `status` null: no answer came. */
  answer(status: number | null, whole: boolean): Outcome | 'bytes' {
    const said = this.x.sync_answer(status ?? -1, +whole);
    return said === 5 ? 'bytes' : OUTCOME[said];
  }

  /** Bytes just fetched whole, held if they can be: how that ended. */
  takeWhole(kind: Kind, bytes: Uint8Array): Outcome {
    return OUTCOME[this.over(bytes, (ptr, len) => this.x.sync_take_whole(KIND[kind], ptr, len))];
  }

  /** A feed applied to what is kept, the result held if it can be: how that ended. */
  takeChanges(kind: Kind, kept: Uint8Array, feed: Uint8Array): Outcome {
    return OUTCOME[this.over(kept, (k, kLen) => this.over(feed, (f, fLen) => this.x.sync_take_changes(KIND[kind], k, kLen, f, fLen)))];
  }

  /** Covers' head taken: how that ended. Empty, it says they were cut short. */
  takeCovers(head: Uint8Array, whole: boolean): Outcome {
    return whole ? this.takeWhole('covers', head) : this.takeChanges('covers', new Uint8Array(0), head);
  }

  /** Where the covers last taken stand. */
  coversRev = () => this.x.covers_rev_taken();

  /** What to do now with a part. `told`: its feed may be asked. */
  step(p: Part, network: Network, mobile: Mobile, told: boolean, now: number): Step {
    const said = this.x.sync_step(+p.whole, +p.stopped, p.lost, p.failed, p.tried ?? -1, p.started, NETWORK.indexOf(network), MOBILE.indexOf(mobile), +told, now);
    return said >= 0 ? { do: 'wait', until: said } : { do: STEP[said] };
  }

  /** What to do now with what has no feed and goes stale. `took`: when it was last taken. */
  stepStale(p: Part, network: Network, mobile: Mobile, took: number | null, now: number): Step {
    const said = this.x.sync_step_stale(+p.whole, +p.stopped, p.lost, p.failed, p.tried ?? -1, p.started, NETWORK.indexOf(network), MOBILE.indexOf(mobile), took ?? -1, now);
    return said >= 0 ? { do: 'wait', until: said } : { do: STEP[said] };
  }

  /** How a part stands after a fetch that ended at `now`, or a start at `now`. */
  after(p: Part, outcome: Outcome, now: number): Part {
    const bits = this.x.sync_after(+p.whole, +p.stopped, p.lost, p.failed, OUTCOME.indexOf(outcome));
    const start = outcome === 'start' || outcome === 'start-over-another';
    return { whole: !!(bits & 1), stopped: !!(bits & 2), lost: (bits >> 8) & 255, failed: (bits >> 16) & 255, tried: start ? null : now, started: start ? now : p.started };
  }

  /** Whether a part should say it is paused. */
  paused = (p: Part) => this.x.sync_paused(p.lost) === 1;

  /** The first seconds of a start (ms), and how long hidden counts as one: the page goes by its own, which `device-sync` holds to these. */
  firstAfter = () => this.x.sync_first_after();
  anewAfter = () => this.x.sync_anew_after();

  /** The versions this module reads. */
  indexVersion = () => this.x.index_version();
  soundsVersion = () => this.x.sounds_version();
  coversVersion = () => this.x.covers_version();
  ownVersion = () => this.x.marks_version();
}

/** The module's instance: one to let go when done. */
export async function instantiateIndex(module: WebAssembly.Module): Promise<IndexEngine> {
  return new IndexEngine((await WebAssembly.instantiate(module)).exports as unknown as Exports);
}
