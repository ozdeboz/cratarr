// A Bandcamp preview's tracks (decision 0071) as a queue carries them: their source, and their ids read back to the
// album's page on Bandcamp (server/src/bandcamp.rs track_id: "bc." and, base64url, "<number>|<page>"), so the
// player's menu can share that page and find the Discover suggestion it came from (the owner, 2026-10-25).

/** The source a preview's tracks carry. */
export const BANDCAMP = 'bandcamp';

/** A preview track's album page on Bandcamp, else null. */
export function previewPage(id: string): string | null {
  if (!id.startsWith('bc.')) return null;
  try {
    const raw = atob(id.slice(3).replace(/-/g, '+').replace(/_/g, '/'));
    const text = new TextDecoder().decode(Uint8Array.from(raw, (c) => c.charCodeAt(0)));
    const bar = text.indexOf('|');
    return bar < 0 ? null : text.slice(bar + 1);
  } catch {
    return null;
  }
}

/** A Bandcamp album's id as the panel opens it: its track 0's, as the server writes ids (bandcamp.rs track_id). */
export function albumIdOf(page: string): string {
  const bytes = new TextEncoder().encode(`0|${page}`);
  const b64 = btoa(String.fromCharCode(...bytes));
  return `bc.${b64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')}`;
}

/** Whether two Bandcamp addresses are the same page (scheme, query, fragment, a last slash and case aside). */
export function samePage(a: string, b: string): boolean {
  const bare = (u: string) => u.toLowerCase().replace(/^https?:\/\//, '').replace(/[?#].*$/, '').replace(/\/+$/, '');
  return bare(a) === bare(b);
}
