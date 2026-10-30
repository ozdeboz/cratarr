// The page's live channel (server-sent events; the server's app/events.rs): `state` (what plays and which playlists
// hold it), `jobs` (the conversion queue) and `changed` ({ area }: a list to read again), each sent when it changes.
// One stream for the whole app, opened by live.svelte.ts. While it is down the pollers carry on: they check
// `stream.connected`.
//
// A tab that is not being looked at closes its stream and opens it again when it is: over plain HTTP a browser opens
// at most six connections to a server, shared by all its tabs, and every open stream holds one for good (seen
// 2026-10-13: six background tabs, and the Settings page never loaded).

import { reach, refusedFor } from './reach.svelte';

type Handler = (data: unknown) => void;

export const stream = $state({ connected: false });

const handlers = new Map<string, Set<Handler>>();
let source: EventSource | null = null;
let url = '';
let retry: ReturnType<typeof setTimeout> | undefined;
/** The source followed, while the stream is wanted (between `open` and `close`). */
let wanted: { follow?: string } | null = null;

document.addEventListener('visibilitychange', () => {
  if (!wanted) return;
  if (document.hidden) {
    clearTimeout(retry);
    source?.close();
    source = null;
    stream.connected = false;
  } else {
    connect(wanted.follow);
  }
});

function listen(es: EventSource, name: string) {
  es.addEventListener(name, (e) => {
    let data: unknown;
    try {
      data = JSON.parse((e as MessageEvent<string>).data);
    } catch {
      return;
    }
    for (const h of handlers.get(name) ?? []) h(data);
  });
}

/** Call `handler` with each `name` event's data; returns the function that stops it. */
export function on<T = unknown>(name: string, handler: (data: T) => void): () => void {
  let set = handlers.get(name);
  if (!set) {
    set = new Set();
    handlers.set(name, set);
    if (source) listen(source, name);
  }
  const h = handler as Handler;
  set.add(h);
  return () => set.delete(h);
}

/** Open the stream for the source followed (`undefined`: whichever plays). Opening it again with another one
 *  replaces it. */
export function open(follow?: string) {
  wanted = { follow };
  if (!document.hidden) connect(follow);
}

function connect(follow?: string) {
  // Refused by the proxy a moment ago: again once that has passed (lib/reach), not before.
  if (refusedFor() > 0) {
    clearTimeout(retry);
    retry = setTimeout(() => wanted && connect(follow), refusedFor());
    return;
  }
  const next = `/api/events${follow ? `?source=${encodeURIComponent(follow)}` : ''}`;
  if (source && url === next && source.readyState !== EventSource.CLOSED) return;
  clearTimeout(retry);
  source?.close();
  url = next;
  const es = new EventSource(url);
  source = es;
  for (const name of handlers.keys()) listen(es, name);
  es.onopen = () => (stream.connected = true);
  es.onerror = () => {
    stream.connected = false;
    // The browser retries by itself, unless it gave up (the server answered with an error): then try again later, less
    // often while Cratarr is out of reach (lib/reach).
    if (es.readyState === EventSource.CLOSED && source === es) {
      clearTimeout(retry);
      retry = setTimeout(() => source === es && connect(follow), reach.away ? 30_000 : 10_000);
    }
  };
}

export function close() {
  wanted = null;
  clearTimeout(retry);
  source?.close();
  source = null;
  stream.connected = false;
}
