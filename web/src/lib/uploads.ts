// Uploads from the page (UP04; UP01–UP03 are the server's): files picked, each sent a piece at a time from where the server says it
// has them, so a cut (the WebView paused, the network gone) or a reload goes on from there; the whole pick checked against the room
// first; a line each, its progress the server's. The rules, pure, so the tests take them as they are; lib/UploadSheet runs them.

import { GB } from './room';

/** A piece's size: the route's body limit is one piece (UP02.13). */
export const PIECE = 8 * 1024 * 1024;

/** The next piece of a file to send, from the server's `received` (never the page's own count): bytes `start` to `end`, the last
 *  one short; none once the file is whole. A `received` past the file's size is no status of this file. */
export function nextPiece(file: { size: number }, received: number, pieceSize = PIECE): { start: number; end: number } | null {
  if (!Number.isInteger(received) || received < 0 || received > file.size) throw new RangeError(`received ${received} of ${file.size}`);
  if (received === file.size) return null;
  return { start: received, end: Math.min(file.size, received + pieceSize) };
}

/** A file as an upload knows it, and as it is picked. */
export type Named = { name: string; size: number };

/** After a reload the files' handles are gone: each of the upload's files matched again among those picked anew, by name and size
 *  (one of the name and another size is no match, and is not sent); null where none is. */
export function matchPicked<P extends Named>(uploadFiles: readonly Named[], picked: readonly P[]): (P | null)[] {
  const left = [...picked];
  return uploadFiles.map((u) => {
    const at = left.findIndex((p) => p.name === u.name && p.size === u.size);
    return at < 0 ? null : left.splice(at, 1)[0];
  });
}

/** A size in GB as the room says it (decimal, one place where it has one). */
const inGb = (bytes: number) => `${Math.round((bytes / GB) * 10) / 10} GB`;

/** The whole pick checked before anything is sent (Q7: "Refused up front"): its words, or null where it may go. `left`: what the
 *  account's room has left (null: no room, an administrator's); `short`: the server below its disk floor. */
export function pickRefusal(total: number, left: number | null, short: boolean): string | null {
  if (short) return 'The server is short of space';
  if (left !== null && total > left) return `Needs ${inGb(total)}, ${inGb(Math.max(0, left))} left`;
  return null;
}

/** The picked folder's name (a name, never a path: `webkitRelativePath`'s first part), or null for files picked loose. */
export function pickedFolder(files: readonly { webkitRelativePath?: string }[]): string | null {
  const first = files[0]?.webkitRelativePath?.split('/')[0];
  return first && files.every((f) => f.webkitRelativePath?.split('/')[0] === first) ? first : null;
}

/** A line's progress: the server's bytes of the file, never those sent. */
export const lineProgress = (f: { size: number; received: number }) => (f.size ? Math.min(1, f.received / f.size) : 1);

/** An unfinished upload's line after a reload: how many of its files are whole ("3 of 10 files"). */
export function wholeOf(files: readonly { size: number; received: number }[]): string {
  const whole = files.filter((f) => f.received >= f.size).length;
  return `${whole} of ${files.length} ${files.length === 1 ? 'file' : 'files'}`;
}

/** A piece's SHA-256 as the request carries it (RFC 9530's `Content-Digest`), the piece alone read: never a whole file. */
export async function pieceDigest(piece: Blob): Promise<string> {
  const sum = new Uint8Array(await crypto.subtle.digest('SHA-256', await piece.arrayBuffer()));
  let bin = '';
  for (const b of sum) bin += String.fromCharCode(b);
  return `sha-256=:${btoa(bin)}:`;
}

/** Sends `file` to an upload's file from `received` on, a piece at a time (`File.slice`), each with its digest; after each,
 *  `received` is what the server answers, so a cut is gone on from the server's count. */
export async function sendFrom(
  file: Blob,
  received: number,
  put: (offset: number, piece: Blob, digest: string) => Promise<number>,
  pieceSize = PIECE,
): Promise<number> {
  let at = received;
  for (let next = nextPiece(file, at, pieceSize); next; next = nextPiece(file, at, pieceSize)) {
    const piece = file.slice(next.start, next.end);
    at = await put(next.start, piece, await pieceDigest(piece));
  }
  return at;
}
