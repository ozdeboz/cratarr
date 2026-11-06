// An album's cover on this device (Symfonium's "Change local thumbnail", decided 2026-10-16): a picture chosen here,
// kept in this browser (IndexedDB) and shown wherever the album's cover is (lib/api's albumCoverUrl); and a cover read
// again from the server (Symfonium's "Remove cached images"). Neither reaches the server or another device; Home's
// picture alone does, as a copy on the account. Each account's apart (decision 0101: `cratarr-covers@name`; Home's
// header picture is the person's own), hidden while another is signed in.
import { perAccount, whenAccount } from './account';

const DB = 'cratarr-covers';
const STORE = 'covers';
/** The longest side a chosen picture is kept at. */
const MAX_PX = 1000;

/** `source/album` → the picture chosen here (an object URL). */
export const localCovers = $state<Record<string, string>>({});
/** `source/album` → how often its cover was read again here: a new address each time, so every image takes it. */
export const coverRounds = $state<Record<string, number>>({});
export const coverKey = (source: string, album: string) => `${source}/${album}`;

/** Whose pictures [localCovers] holds. */
let ofWho = perAccount(DB);

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open(ofWho, 1);
    r.onupgradeneeded = () => r.result.createObjectStore(STORE);
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

async function write(key: string, blob: Blob | null) {
  const db = await open();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, 'readwrite');
    if (blob) tx.objectStore(STORE).put(blob, key);
    else tx.objectStore(STORE).delete(key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
  db.close();
}

// The pictures chosen before, as the page opens, and another account's when it signs in.
async function load() {
  const who = ofWho;
  const db = await open();
  const tx = db.transaction(STORE, 'readonly');
  const cursor = tx.objectStore(STORE).openCursor();
  cursor.onsuccess = () => {
    const c = cursor.result;
    if (!c || who !== ofWho) return db.close();
    if (c.value instanceof Blob) localCovers[String(c.key)] = URL.createObjectURL(c.value);
    c.continue();
  };
}
if (typeof indexedDB !== 'undefined') load().catch(() => {});

// Another account signed in, or nobody: nothing of the one before shows from here on, at once, its pictures read in as
// they come. Before lib/accountPrefs hears of it, which takes what shows now for the account's and not for a change.
whenAccount.add(() => {
  if (perAccount(DB) === ofWho) return;
  ofWho = perAccount(DB);
  for (const k of Object.keys(localCovers)) {
    URL.revokeObjectURL(localCovers[k]);
    delete localCovers[k];
  }
  homePicture.held = keptHeld();
  if (typeof indexedDB !== 'undefined') load().catch(() => {});
});

/** A picture no larger than it needs to be (JPEG, its longest side at most `max`); as it is when it cannot be read. */
async function shrink(file: Blob, max = MAX_PX): Promise<Blob> {
  try {
    const img = await createImageBitmap(file);
    const scale = Math.min(1, max / Math.max(img.width, img.height));
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(img.width * scale);
    canvas.height = Math.round(img.height * scale);
    canvas.getContext('2d')!.drawImage(img, 0, 0, canvas.width, canvas.height);
    img.close();
    return await new Promise<Blob>((resolve) => canvas.toBlob((b) => resolve(b ?? file), 'image/jpeg', 0.9));
  } catch {
    return file;
  }
}

/** Keeps `file` on this device under `key` (its longest side at most `max`), shown from now on. */
export async function setLocalPicture(key: string, file: Blob, max = MAX_PX) {
  const who = ofWho;
  // Read whole first: a picked file is only a reference to it on the device, which IndexedDB may not keep.
  const small = await shrink(new Blob([await file.arrayBuffer()], { type: file.type }), max);
  // Another account signed in meanwhile: it is not that one's choice.
  if (who !== ofWho) return;
  await write(key, small);
  if (localCovers[key]) URL.revokeObjectURL(localCovers[key]);
  localCovers[key] = URL.createObjectURL(small);
}

/** Forgets the picture kept under `key`. */
export async function removeLocalPicture(key: string) {
  await write(key, null);
  if (localCovers[key]) URL.revokeObjectURL(localCovers[key]);
  delete localCovers[key];
}

/** Shows `file` as the album's cover on this device from now on. */
export const setLocalCover = (source: string, album: string, file: Blob) => setLocalPicture(coverKey(source, album), file);
/** The server's cover again. */
export const removeLocalCover = (source: string, album: string) => removeLocalPicture(coverKey(source, album));

/** Home's header picture chosen in place of the weather's sky (the owner, 2026-10-24); wide, so kept larger than a cover. */
export const HOME_HEADER = 'home/header';
const HOME_HEADER_PX = 2400;

// Home's picture on the account (decision 0101 point 14), as a copy: what a preference holds is far below the picture
// this device keeps, so the account's is the largest that fits, a data URL. The device that chose it shows its own;
// another shows the account's. Not among the prefs: those go over to the app whole at each change. Each account's own.
const HELD = 'cratarr-home-picture';
/** The most the account keeps of a preference, as JSON (the server's MOST_BYTES), less the quotes round a string. */
const HELD_MOST = 256 * 1024 - 2;
/** The longest sides tried for the copy, largest first. */
const HELD_SIDES = [1600, 1200, 900, 600, 400];

function keptHeld(): string | null {
  try {
    return localStorage.getItem(perAccount(HELD));
  } catch {
    return null;
  }
}

/** Home's picture as the account holds it: a data URL; null: none. */
export const homePicture = $state({ held: keptHeld() });

function hold(held: string | null) {
  homePicture.held = held;
  try {
    if (held === null) localStorage.removeItem(perAccount(HELD));
    else localStorage.setItem(perAccount(HELD), held);
  } catch {
    /* not kept: asked of the account again at the next start */
  }
}

/** The largest copy `copyAt` makes of `sides` (largest first) within `most` characters; null where none is. */
export async function fittedCopy(sides: number[], most: number, copyAt: (side: number) => Promise<string>): Promise<string | null> {
  for (const side of sides) {
    const copy = await copyAt(side);
    if (copy.length <= most) return copy;
  }
  return null;
}

const dataUrl = (b: Blob) =>
  new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error);
    r.readAsDataURL(b);
  });
const heldCopy = (file: Blob) => fittedCopy(HELD_SIDES, HELD_MOST, async (side) => dataUrl(await shrink(file, side)));

function blobOf(data: string): Blob {
  const [head, body] = data.split(',');
  return new Blob([Uint8Array.from(atob(body), (c) => c.charCodeAt(0))], { type: head.slice('data:'.length, head.indexOf(';')) });
}

/** `file` as Home's picture, here and on the account. */
export async function chooseHomePicture(file: Blob) {
  const who = ofWho;
  // Read whole first, as `setLocalPicture` does: it is read twice here.
  const whole = new Blob([await file.arrayBuffer()], { type: file.type });
  await setLocalPicture(HOME_HEADER, whole, HOME_HEADER_PX);
  const copy = await heldCopy(whole);
  if (who === ofWho) hold(copy);
}

/** No picture of one's own any more, here or on the account. */
export async function dropHomePicture() {
  hold(null);
  await removeLocalPicture(HOME_HEADER);
}

/** The account's picture, or none, made this device's: shown as one chosen here. */
export async function takeHomePicture(held: string | null) {
  const blob = held === null ? null : blobOf(held);
  hold(held);
  if (localCovers[HOME_HEADER]) URL.revokeObjectURL(localCovers[HOME_HEADER]);
  if (blob) localCovers[HOME_HEADER] = URL.createObjectURL(blob);
  else delete localCovers[HOME_HEADER];
  await write(HOME_HEADER, blob);
}

/** Reads the album's cover from the server again, past the browser's copy (`url`: its plain address). */
export async function refreshCover(source: string, album: string, url: string) {
  await fetch(url, { cache: 'reload' });
  const key = coverKey(source, album);
  coverRounds[key] = (coverRounds[key] ?? 0) + 1;
}
