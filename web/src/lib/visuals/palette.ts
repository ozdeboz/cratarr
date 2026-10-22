// The colours of a track's cover, for the visuals' Cover colours (decision 0046, amended): the cover drawn small, its
// pixels as OKLab, gathered into a few colours (k-means), each pixel weighted by how coloured it is, so a grey, white or
// black background does not take a band from the colours the cover is known by. A cover with hardly any colour gives
// its greys.

export type PaletteColour = {
  /** OKLab: the perceptual lightness (0..1) and the colour plane's two axes. */
  L: number;
  a: number;
  b: number;
  /** Its weight in the cover (0..1, all of them summing to one): its pixels, each counted by how coloured it is. */
  share: number;
};

/** A cover's colours, darkest first. */
export type Palette = { colours: PaletteColour[] };

/** The cover is drawn this many pixels a side: enough for its colours, little to gather. */
const SIZE = 48;
const ROUNDS = 12;
/** Under this chroma a pixel counts as grey: weighed `GREY_WEIGHT`, against 1 for one of `FULL` chroma or more; a grey
 *  darker than `DARK` or lighter than `LIGHT` (a cover's black or white ground, and black is nothing on the visuals'
 *  black) only `GROUND_WEIGHT`. */
const GREY = 0.02;
const FULL = 0.1;
const GREY_WEIGHT = 0.08;
const DARK = 0.25;
const LIGHT = 0.9;
const GROUND_WEIGHT = 0.005;
/** A colour of some tint, but under `LIFT`, is lifted to it, so the cover's tint shows on the black around it. */
const LIFT = 0.05;
/** A cover whose coloured pixels weigh less than this share of it is grey: its greys are its colours. */
const GREY_COVER = 0.02;

const linear = Array.from({ length: 256 }, (_, i) => {
  const c = i / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
});

/** OKLab from sRGB (0..255 each). */
export function oklab(r: number, g: number, b: number): [number, number, number] {
  const R = linear[r];
  const G = linear[g];
  const B = linear[b];
  const l = Math.cbrt(0.4122214708 * R + 0.5363325363 * G + 0.0514459929 * B);
  const m = Math.cbrt(0.2119034982 * R + 0.6806995451 * G + 0.1073969566 * B);
  const s = Math.cbrt(0.0883024619 * R + 0.2817188376 * G + 0.6299787005 * B);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/** The colours of an image's pixels (RGBA, 0..255 each; the transparent ones left out): `n` of them, darkest first, or
 *  null where none is opaque. */
export function paletteOf(rgba: ArrayLike<number>, n = 6): Palette | null {
  const count = Math.floor(rgba.length / 4);
  const L = new Float64Array(count);
  const A = new Float64Array(count);
  const B = new Float64Array(count);
  const w = new Float64Array(count);
  let used = 0;
  let coloured = 0;
  for (let i = 0; i < count; i++) {
    if (rgba[i * 4 + 3] < 128) continue;
    const [l, a, b] = oklab(rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]);
    const c = Math.hypot(a, b);
    L[used] = l;
    A[used] = a;
    B[used] = b;
    w[used] =
      c >= GREY ? GREY_WEIGHT + (1 - GREY_WEIGHT) * Math.min(1, (c - GREY) / (FULL - GREY)) : l < DARK || l > LIGHT ? GROUND_WEIGHT : GREY_WEIGHT;
    if (c >= GREY) coloured++;
    used++;
  }
  if (used === 0) return null;
  // A grey cover: every pixel as much as another.
  if (coloured < used * GREY_COVER) w.fill(1, 0, used);

  // The first centre the weighted mean, each next the pixel furthest from those there are (by weight): the same
  // colours each time for the same cover.
  const k = Math.max(1, Math.min(n, used));
  const centres: [number, number, number][] = [];
  const near = new Float64Array(used).fill(Infinity);
  const furthest = () => {
    let best = 0;
    let most = -1;
    for (let i = 0; i < used; i++) {
      const d = w[i] * near[i];
      if (d > most) {
        most = d;
        best = i;
      }
    }
    return best;
  };
  const closer = (c: [number, number, number]) => {
    for (let i = 0; i < used; i++) near[i] = Math.min(near[i], (L[i] - c[0]) ** 2 + (A[i] - c[1]) ** 2 + (B[i] - c[2]) ** 2);
  };
  {
    let sw = 0;
    const m: [number, number, number] = [0, 0, 0];
    for (let i = 0; i < used; i++) {
      sw += w[i];
      m[0] += w[i] * L[i];
      m[1] += w[i] * A[i];
      m[2] += w[i] * B[i];
    }
    centres.push([m[0] / sw, m[1] / sw, m[2] / sw]);
    closer(centres[0]);
  }
  while (centres.length < k) {
    const i = furthest();
    centres.push([L[i], A[i], B[i]]);
    closer(centres[centres.length - 1]);
  }

  const of = new Int32Array(used);
  const weight = new Float64Array(k);
  for (let round = 0; round < ROUNDS; round++) {
    for (let i = 0; i < used; i++) {
      let best = 0;
      let d = Infinity;
      for (let j = 0; j < k; j++) {
        const c = centres[j];
        const e = (L[i] - c[0]) ** 2 + (A[i] - c[1]) ** 2 + (B[i] - c[2]) ** 2;
        if (e < d) {
          d = e;
          best = j;
        }
      }
      of[i] = best;
      near[i] = d;
    }
    const sums = Array.from({ length: k }, () => [0, 0, 0]);
    weight.fill(0);
    for (let i = 0; i < used; i++) {
      const j = of[i];
      weight[j] += w[i];
      sums[j][0] += w[i] * L[i];
      sums[j][1] += w[i] * A[i];
      sums[j][2] += w[i] * B[i];
    }
    let moved = false;
    for (let j = 0; j < k; j++) {
      // A centre no pixel chose: to the pixel furthest from its own.
      const c: [number, number, number] =
        weight[j] > 0 ? [sums[j][0] / weight[j], sums[j][1] / weight[j], sums[j][2] / weight[j]] : (() => {
          const i = furthest();
          near[i] = 0;
          return [L[i], A[i], B[i]] as [number, number, number];
        })();
      if (c.some((v, e) => Math.abs(v - centres[j][e]) > 1e-5)) moved = true;
      centres[j] = c;
    }
    if (!moved) break;
  }

  const total = weight.reduce((s, x) => s + x, 0) || 1;
  const colours = centres.map(([l, a, b], j) => {
    const c = Math.hypot(a, b);
    const lift = c >= GREY && c < LIFT ? LIFT / c : 1;
    return { L: l, a: a * lift, b: b * lift, share: weight[j] / total };
  });
  colours.sort((x, y) => x.L - y.L);
  return { colours };
}

/** The covers' colours already worked out (or being), by address; the oldest let go past `KEPT`. */
const known = new Map<string, Promise<Palette | null>>();
const KEPT = 64;

/** The colours of the cover at `url` (the page's own, as `coverUrl` gives it): null where it has none, or it does not
 *  load. Each cover's are worked out once. */
export function coverPalette(url: string, n = 6): Promise<Palette | null> {
  const key = `${n}:${url}`;
  const had = known.get(key);
  if (had) return had;
  const p = drawn(url).then(
    (pixels) => (pixels ? paletteOf(pixels, n) : null),
    () => null,
  );
  known.set(key, p);
  if (known.size > KEPT) known.delete(known.keys().next().value!);
  return p;
}

/** The covers loaded lately, by address (the Music module's picture, beside their colours). */
const images = new Map<string, Promise<HTMLImageElement | null>>();

/** The cover at `url` as an image, decoded: null where it does not load. The last few are kept. */
export function coverImage(url: string): Promise<HTMLImageElement | null> {
  const had = images.get(url);
  if (had) return had;
  const image = new Image();
  image.decoding = 'async';
  image.src = url;
  const p = image.decode().then(
    () => image,
    () => null,
  );
  images.set(url, p);
  if (images.size > 4) images.delete(images.keys().next().value!);
  return p;
}

/** The image at `url` drawn `SIZE` pixels a side: its pixels, or null where it does not load. */
async function drawn(url: string): Promise<Uint8ClampedArray | null> {
  const image = await coverImage(url);
  if (!image) return null;
  const canvas = typeof OffscreenCanvas === 'function' ? new OffscreenCanvas(SIZE, SIZE) : Object.assign(document.createElement('canvas'), { width: SIZE, height: SIZE });
  const g = canvas.getContext('2d', { willReadFrequently: true }) as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
  if (!g) return null;
  g.imageSmoothingQuality = 'high';
  g.drawImage(image, 0, 0, SIZE, SIZE);
  return g.getImageData(0, 0, SIZE, SIZE).data;
}
