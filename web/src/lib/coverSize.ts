// What a list asks for a cover (finding 86; the owner, 2026-11-07: lists and grids get sized covers, the player and a panel keep the
// whole one): about the size it is drawn at, in the device's own pixels, as Cratarr makes it once and keeps it. A 3000 by 3000 cover
// is 36 MB decoded on a tablet, and a grid shows a hundred. Pure: lib/Cover measures its box and asks here.

/** The sizes asked, in device pixels: few, so each is made once and kept, by Cratarr and by the browser. The first is the size of
 *  the small covers Cratarr makes for a device's index (D13), which it then hands out as they are. */
export const COVER_STEPS = [160, 320, 640, 1280] as const;

/** The addresses Cratarr makes sizes of: a record's cover, and a track's (its record's). */
const SIZABLE = /^\/api\/(?:albums\/[^/]+\/[^/]+\/cover|cover\/[^/]+\/[^/]+)$/;
const PAGE = 'http://page.invalid';

/** `src` as a URL on this page, where it is one Cratarr makes sizes of; else null. */
function ours(src: string): URL | null {
  try {
    const u = new URL(src, PAGE);
    return u.origin === PAGE && SIZABLE.test(u.pathname) ? u : null;
  } catch {
    return null;
  }
}

/** Whether a cover's address takes a size: a device's own, a server's in its mode, a picture chosen here do not. */
export const sizable = (src: string) => ours(src) !== null;

/** `src` at the least size that fills `px` device pixels; as it is where none does (the whole picture), or it takes no size. */
export function sizedCover(src: string, px: number): string {
  const step = COVER_STEPS.find((s) => s >= px);
  const u = ours(src);
  if (!step || !u) return src;
  u.searchParams.set('size', String(step));
  return u.pathname + u.search;
}
