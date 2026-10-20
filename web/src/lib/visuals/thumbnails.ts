// The graph's thumbnails (decision 0057): which modules' pictures a frame has of their own when it is done, for a
// small copy of one. Pure: no GL here (nodes/thumbs.ts draws them).

/** Each module's picture at the end of a frame, from the draws in their order (a module, the target it drew into):
 *  a target drawn into again holds the later module's, not the earlier's (the pool gives it out once read). A module
 *  at rest (`alias`: what it passes on) shows what it passes on. A module that drew none (fused into a pass) has none. */
export function outputsOf<T>(draws: readonly (readonly [string, T])[], alias: ReadonlyMap<string, { module: string }> = new Map()): Map<string, T> {
  const owner = new Map<T, string>();
  for (const [id, t] of draws) owner.set(t, id);
  const out = new Map<string, T>();
  for (const [id, t] of draws) if (owner.get(t) === id) out.set(id, t);
  for (const id of alias.keys()) {
    let at = id;
    for (let i = 0; alias.has(at) && i <= alias.size; i++) at = alias.get(at)!.module;
    const t = out.get(at);
    if (t !== undefined && !out.has(id)) out.set(id, t);
  }
  return out;
}

/** A thumbnail's size for a picture of `width` x `height`: a sixteenth of each side, a pixel at least. */
export const thumbSize = (width: number, height: number): [number, number] => [Math.max(1, Math.round(width / 16)), Math.max(1, Math.round(height / 16))];
