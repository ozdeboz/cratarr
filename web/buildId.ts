// The page's build (build.json's `build`, decision 0085 step 1), made from what the build is, not from what its files are
// named. Two builds of one commit named their chunks apart: main and the server adapters' chunks import each other, so
// each name hashes the other's (the desktop app's build and the image's, 2026-11-04), and a page an app carries then
// never counted as the server's own build. So each file's hash is taken out of its name and out of every file that
// names it; what is left, in name order, is hashed. Pure.
import { createHash } from 'node:crypto';

/** A file of the built page: its path, and what is in it. */
export type Built = { fileName: string; content: string | Uint8Array };

const HASHED = /-[A-Za-z0-9_-]{8}(\.[A-Za-z0-9]+)$/;

/** A built file's path without its hash (`assets/main-CBwqQcPE.js` → `assets/main.js`); only the hashed output under
 *  `assets/` has one. */
export const bare = (fileName: string) => (fileName.startsWith('assets/') ? fileName.replace(HASHED, '$1') : fileName);

/** The build's id: 12 characters of the hash of its files, their names' hashes taken out. */
export function buildId(files: Built[]): string {
  const names = [...new Set(files.map((f) => f.fileName).filter((n) => bare(n) !== n))].map((n) => n.slice('assets/'.length));
  // One pass over each file for every hashed name, the longest first (a name inside a longer one is not cut from it).
  const plainOf = new Map(names.map((n) => [n, bare(`assets/${n}`).slice('assets/'.length)]));
  const anyName = names.length ? new RegExp(names.sort((a, b) => b.length - a.length).map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|'), 'g') : null;
  const parts = files.map((f) => {
    const text = typeof f.content === 'string' ? f.content : null;
    const body = text === null ? Buffer.from(f.content as Uint8Array) : Buffer.from(anyName ? text.replace(anyName, (n) => plainOf.get(n) ?? n) : text);
    return { name: bare(f.fileName), body };
  });
  // In name order; two that come out under one name (two chunks both called index), by what is in them.
  parts.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : Buffer.compare(a.body, b.body)));
  const h = createHash('sha256');
  for (const p of parts) h.update(p.name).update('\0').update(p.body).update('\0');
  return h.digest('base64url').slice(0, 12);
}
