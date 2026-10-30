// The one matcher, the page's side (0.12): the server's own (the rules crate's matcher, decision 0072 as 0.12 has it),
// built as WebAssembly (matcher.wasm, its own file so the page's first load does not pay for it). Separators do not
// count ("hiphop" is Hip Hop is Hip-Hop), a name's aliases count ("dnb" is Drum and Bass), and a name meets a query as
// exact, alias, prefix or fuzzy (trigrams), in that order. The vectors the server is held to hold this too
// (tests/matcher.test.ts). Pure: lib/matcherLoad fetches it, lib/search asks it.

/** How a name met the query, strongest first. */
export type How = 'exact' | 'alias' | 'prefix' | 'fuzzy';
/** A name the query found: its place in the list, its score (0 to 1, within its `how`), how. */
export type Hit = { i: number; score: number; how: How };
/** A name to be found, with the other names it goes by. */
export type MatchName = { text: string; aliases?: readonly string[] };

/** A text as the matcher reads it (a5's contract, 2026-10-30): compatibility-decomposed, its marks dropped, lower case,
 *  as the server folds in the crate. The WebAssembly has no Unicode tables of its own for this; every text it is
 *  given, names and query alike, is folded here first. */
export const fold = (text: string): string => text.normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase();

type Exports = {
  memory: WebAssembly.Memory;
  matcher_alloc(len: number): number;
  matcher_free(ptr: number, len: number): void;
  matcher_index(ptr: number, len: number): number;
  matcher_query(handle: number, ptr: number, len: number): number;
  matcher_drop(handle: number): void;
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** A query written into the module once, asked of many indexes (a list is asked at every key). */
export type Asked = { readonly ptr: number; readonly len: number };

/** The module's five calls, with JSON in and out over its memory. */
export class Matcher {
  private readonly x: Exports;
  constructor(x: Exports) {
    this.x = x;
  }

  private write(text: string): Asked {
    const bytes = encoder.encode(text);
    const ptr = this.x.matcher_alloc(bytes.length);
    // Read the memory after the alloc: it may have grown (and its buffer been replaced).
    new Uint8Array(this.x.memory.buffer, ptr, bytes.length).set(bytes);
    return { ptr, len: bytes.length };
  }

  /** An index of these names (each folded), its handle; 0 when the module refused it. */
  index(names: readonly MatchName[]): number {
    const json = JSON.stringify({ names: names.map((n) => (n.aliases?.length ? { text: fold(n.text), aliases: n.aliases.map(fold) } : { text: fold(n.text) })) });
    const at = this.write(json);
    try {
      return this.x.matcher_index(at.ptr, at.len);
    } finally {
      this.x.matcher_free(at.ptr, at.len);
    }
  }

  /** A query, folded and written once (`release` it when done). */
  ask(q: string, limit: number): Asked {
    return this.write(JSON.stringify({ q: fold(q), limit }));
  }
  release(at: Asked) {
    this.x.matcher_free(at.ptr, at.len);
  }

  /** The hits of a written query in an index, best first. */
  answer(handle: number, at: Asked): Hit[] {
    const out = this.x.matcher_query(handle, at.ptr, at.len);
    if (!out) return [];
    const n = new DataView(this.x.memory.buffer).getUint32(out, true);
    const body = decoder.decode(new Uint8Array(this.x.memory.buffer, out + 4, n));
    this.x.matcher_free(out, 4 + n);
    return JSON.parse(body) as Hit[];
  }

  /** The hits of `q` in an index, best first, at most `limit`. */
  query(handle: number, q: string, limit: number): Hit[] {
    const at = this.ask(q, limit);
    try {
      return this.answer(handle, at);
    } finally {
      this.release(at);
    }
  }

  drop(handle: number) {
    this.x.matcher_drop(handle);
  }
}

/** The matcher from its module's bytes, or from its fetch (streamed where the answer says application/wasm). */
export async function instantiateMatcher(source: BufferSource | Response): Promise<Matcher> {
  let instance: WebAssembly.Instance;
  if (source instanceof Response) {
    const copy = source.clone();
    try {
      instance = (await WebAssembly.instantiateStreaming(source, {})).instance;
    } catch {
      // Served without application/wasm (a proxy, a cache): its bytes instead.
      instance = (await WebAssembly.instantiate(await copy.arrayBuffer(), {})).instance;
    }
  } else {
    instance = (await WebAssembly.instantiate(source, {})).instance;
  }
  return new Matcher(instance.exports as unknown as Exports);
}

/** Items' texts matched one at a time, as the pages ask (`matches(query, ...texts)` in their filters): each item's texts
 *  read once into a small index of their own (all of them as one name, each also an alias), asked again at every key.
 *  The query is written once while it stays the same; answers are kept per query and texts. At most `most` indexes
 *  live in the module; past that they all go and are made again as asked. */
export class TextMatcher {
  private readonly handles = new Map<string, number>();
  private readonly answers = new Map<string, Hit | null>();
  private asked: { q: string; at: Asked } | null = null;
  readonly matcher: Matcher;
  private readonly most: number;

  constructor(matcher: Matcher, most = 6000) {
    this.matcher = matcher;
    this.most = most;
  }

  /** How `q` meets these texts (empty ones left out), or null. */
  hit(q: string, texts: readonly string[]): Hit | null {
    const kept = texts.filter((t) => t.trim());
    if (!q.trim() || !kept.length) return null;
    const key = kept.join('\u0001');
    const answerKey = `${q}\u0002${key}`;
    const known = this.answers.get(answerKey);
    if (known !== undefined) return known;
    let handle = this.handles.get(key);
    if (handle === undefined) {
      if (this.handles.size >= this.most) this.clear();
      handle = this.matcher.index([{ text: kept.join(' '), aliases: kept.length > 1 ? kept : undefined }]);
      this.handles.set(key, handle);
    }
    if (this.asked?.q !== q) {
      if (this.asked) this.matcher.release(this.asked.at);
      this.asked = { q, at: this.matcher.ask(q, 1) };
    }
    const hit = handle ? (this.matcher.answer(handle, this.asked.at)[0] ?? null) : null;
    if (this.answers.size >= this.most * 4) this.answers.clear();
    this.answers.set(answerKey, hit);
    return hit;
  }

  /** Lets every index go (the module keeps its memory for the next ones). */
  clear() {
    for (const h of this.handles.values()) if (h) this.matcher.drop(h);
    this.handles.clear();
    this.answers.clear();
  }
}
