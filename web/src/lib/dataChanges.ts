// What the page shows of the server's data, kept true after a change (the owner, 2026-11-03: after a fix of track
// lengths, the level above still said what was before; "this class of problem needs to be eradicated"). One place
// turns the server's own word that something changed into kinds of data, and each store of them reads itself again
// when its kinds change; no action has to remember which views show what it changed.
//   files       a run on the library's files finished (`file_run` done: written, set aside or undone)
//   collection  records came, went or moved (`changed`: collection)
//   incoming    an Incoming library changed (`changed`: incoming)
//   playlists   playlists changed (`changed`: playlists)
//   words       a record's or a track's words, a kind's sources or its spellings (`changed`: words; decision 0097)
// A run's own view follows the run as it goes (lib/fileRuns); this is for everything else that shows its outcome.

export type DataKind = 'files' | 'collection' | 'incoming' | 'playlists' | 'words';
type On = <T>(name: string, handler: (data: T) => void) => () => void;

/** How long changes close together wait to be read as one. */
const TOGETHER_MS = 250;

/** The watching, on the server's events as `on` gives them (lib/events in the page; a stand-in in its test). */
export function changesOn(on: On) {
  type Watch = { kinds: readonly DataKind[]; read: () => void; timer: ReturnType<typeof setTimeout> | null };
  const watches = new Set<Watch>();
  function changed(kind: DataKind) {
    for (const w of watches) {
      if (!w.kinds.includes(kind) || w.timer) continue;
      w.timer = setTimeout(() => {
        w.timer = null;
        w.read();
      }, TOGETHER_MS);
    }
  }
  let listening = false;
  function listen() {
    if (listening) return;
    listening = true;
    on<{ state?: string }>('file_run', (r) => void (r.state === 'done' && changed('files')));
    on<{ area?: string }>('changed', ({ area }) => {
      if (area === 'collection' || area === 'incoming' || area === 'playlists' || area === 'words') changed(area);
    });
  }
  /** `read` again whenever data of these kinds changed on the server (a few changes together read once). Until the
   *  returned function is called (a view's own effect returns it); a store of the whole page's never calls it. */
  return function whenChanged(kinds: readonly DataKind[], read: () => void): () => void {
    listen();
    const w: Watch = { kinds, read, timer: null };
    watches.add(w);
    return () => {
      if (w.timer) clearTimeout(w.timer);
      watches.delete(w);
    };
  };
}
