// The page's live channel (server-sent events; the server's app/events.rs): `state` (what plays and which playlists
// hold it), `jobs` (the conversion queue) and `changed` ({ area }: a list to read again), each sent when it changes.
// One stream for the whole app, opened by live.svelte.ts. While it is down the pollers carry on: they check
// `stream.connected`.

type Handler = (data: unknown) => void;

export const stream = $state({ connected: false });

const handlers = new Map<string, Set<Handler>>();
let source: EventSource | null = null;
let url = '';
let retry: ReturnType<typeof setTimeout> | undefined;

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
  const next = `/api/events${follow ? `?source=${encodeURIComponent(follow)}` : ''}`;
  if (source && url === next && source.readyState !== EventSource.CLOSED) return;
  close();
  url = next;
  const es = new EventSource(url);
  source = es;
  for (const name of handlers.keys()) listen(es, name);
  es.onopen = () => (stream.connected = true);
  es.onerror = () => {
    stream.connected = false;
    // The browser retries by itself, unless it gave up (the server answered with an error): then try again later.
    if (es.readyState === EventSource.CLOSED && source === es) {
      clearTimeout(retry);
      retry = setTimeout(() => source === es && open(follow), 10_000);
    }
  };
}

export function close() {
  clearTimeout(retry);
  source?.close();
  source = null;
  stream.connected = false;
}
