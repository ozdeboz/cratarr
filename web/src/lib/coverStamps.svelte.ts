// Every album cover's stamp in Cratarr's library (0.9.7; the owner, 2026-10-26: one stamp list, not a field on every
// album): the server's `GET /api/covers/stamps` ({ "source/album": stamp }), read once, then kept by the live channel
// (`cover`: one album's new stamp, or none where its cover went; `covers`: many at once, read the list again). A cover's
// address carries its stamp (lib/api albumCoverUrl), so a browser keeps a cover until it changes, and a changed one
// shows everywhere at once, on every device, without anything being read again by hand.
import { on, stream } from './events.svelte';

export const coverStamps = $state<Record<string, string>>({});

async function read() {
  try {
    const r = await fetch('/api/covers/stamps');
    if (!r.ok) return;
    const now = (await r.json()) as Record<string, string>;
    for (const k of Object.keys(coverStamps)) if (!(k in now)) delete coverStamps[k];
    for (const [k, v] of Object.entries(now)) if (coverStamps[k] !== v) coverStamps[k] = v;
  } catch {
    // A server before 0.9.7 (no such call) or out of reach: covers as before, by their plain address.
  }
}

on<{ source: string; album: string; v: string | null }>('cover', ({ source, album, v }) => {
  const k = `${source}/${album}`;
  if (v) coverStamps[k] = v;
  else delete coverStamps[k];
});
on('covers', () => void read());

// Read as the page opens, and again whenever the stream comes back: what changed while it was down went unheard.
$effect.root(() => {
  $effect(() => {
    if (stream.connected) void read();
  });
});
void read();
