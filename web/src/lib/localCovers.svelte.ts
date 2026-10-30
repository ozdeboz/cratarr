// An album's cover on this device (Symfonium's "Change local thumbnail", decided 2026-10-16): a picture chosen here,
// kept in this browser (IndexedDB) and shown wherever the album's cover is (lib/api's albumCoverUrl); and a cover read
// again from the server (Symfonium's "Remove cached images"). Neither reaches the server or another device.

const DB = 'cratarr-covers';
const STORE = 'covers';
/** The longest side a chosen picture is kept at. */
const MAX_PX = 1000;

/** `source/album` → the picture chosen here (an object URL). */
export const localCovers = $state<Record<string, string>>({});
/** `source/album` → how often its cover was read again here: a new address each time, so every image takes it. */
export const coverRounds = $state<Record<string, number>>({});
export const coverKey = (source: string, album: string) => `${source}/${album}`;

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open(DB, 1);
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

// The pictures chosen before, as the page opens.
async function load() {
  const db = await open();
  const tx = db.transaction(STORE, 'readonly');
  const cursor = tx.objectStore(STORE).openCursor();
  cursor.onsuccess = () => {
    const c = cursor.result;
    if (!c) return db.close();
    if (c.value instanceof Blob) localCovers[String(c.key)] = URL.createObjectURL(c.value);
    c.continue();
  };
}
if (typeof indexedDB !== 'undefined') load().catch(() => {});

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
  // Read whole first: a picked file is only a reference to it on the device, which IndexedDB may not keep.
  const small = await shrink(new Blob([await file.arrayBuffer()], { type: file.type }), max);
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

/** Home's header picture chosen on this device in place of the weather's sky (the owner, 2026-10-24); wide, so kept
 *  larger than a cover. */
export const HOME_HEADER = 'home/header';
export const HOME_HEADER_PX = 2400;

/** Reads the album's cover from the server again, past the browser's copy (`url`: its plain address). */
export async function refreshCover(source: string, album: string, url: string) {
  await fetch(url, { cache: 'reload' });
  const key = coverKey(source, album);
  coverRounds[key] = (coverRounds[key] ?? 0) + 1;
}
