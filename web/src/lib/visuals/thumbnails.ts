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

/** One step of a thumbnail's reduction: a quarter of each side (rounded up, a pixel at least), each pixel the brightest
 *  of the (up to) 4×4 it covers. */
export const quarter = (width: number, height: number): [number, number] => [Math.max(1, Math.ceil(width / 4)), Math.max(1, Math.ceil(height / 4))];

/** A thumbnail's size for a picture of `width` x `height`: two quarters, about a sixteenth of each side. */
export const thumbSize = (width: number, height: number): [number, number] => quarter(...quarter(width, height));

/** The first of the pixels (along a side) that pixel `i` of a reduced side covers: a block of `ceil(from / to)`, 4 at
 *  most, the last ones clamped to the side. */
export const blockStart = (i: number, from: number, to: number) => Math.floor((i * from) / to);

/** One step on the CPU, as the shader does it (for the tests): each channel the most of its block. */
export function reduceMax(px: Float32Array, width: number, height: number): { px: Float32Array; width: number; height: number } {
  const [w, h] = quarter(width, height);
  const out = new Float32Array(w * h * 4);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++)
      for (let j = 0; j < 4; j++)
        for (let i = 0; i < 4; i++) {
          const sx = Math.min(width - 1, blockStart(x, width, w) + i);
          const sy = Math.min(height - 1, blockStart(y, height, h) + j);
          for (let c = 0; c < 4; c++) out[(y * w + x) * 4 + c] = Math.max(out[(y * w + x) * 4 + c], px[(sy * width + sx) * 4 + c]);
        }
  return { px: out, width: w, height: h };
}
