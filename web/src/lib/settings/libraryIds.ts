// Cratarr's own libraries by their ids (CU17, CU18; Q11 (a): `cratarr/<folder id>` everywhere): a library is its id, kept for life,
// and its folder, which may change. Settings keep the folders chosen by their paths (decision 0044), and the server says which
// library each folder is. Pure, so the tests take it as it is.

/** A server's library as its view lists it: its id, and the folders it reads (Cratarr's own: its one folder). */
export type Listed = { id: string; folders: readonly string[]; paths: readonly string[] };

/** The library of Cratarr's own folder `path`: the server's id for it, once it has read it; until then (a folder just chosen) the
 *  path, which the server takes as a new library's. */
export function libraryOf(listed: readonly Listed[], path: string): string {
  return listed.find((l) => l.folders.includes(path) || l.paths.includes(path))?.id ?? path;
}

/** The chosen folders with `from` changed to `to` in its place ("Change folder", CU18): the same library, its id kept. Null where it
 *  cannot be: `from` not chosen, or `to` chosen already (it is a library of its own). */
export function folderMoved(folders: readonly string[], from: string, to: string): string[] | null {
  if (!folders.includes(from) || from === to || folders.includes(to)) return null;
  return folders.map((p) => (p === from ? to : p));
}
