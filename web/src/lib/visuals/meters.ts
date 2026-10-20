// What the visuals do, measured (decision 0046, amended): how much they flash, move, change shape and colour, pump
// and start new looks, each as a number the sidechaining holds within its budget and the calibration compares with the
// owner's ratings. One definition for both: the runtime fills `FrameStats` each frame; the precheck of a recipe fills
// them from a simulation (no picture: its flashes from the glow it would show, one tile for the screen).
//
// Flashes are WCAG 2.2's (success criterion 2.3.1, "Three Flashes or Below Threshold"): a general flash is a pair of
// opposing changes in relative luminance of 0.1 or more where the darker is below 0.8; a red flash a pair of opposing
// transitions into or out of a saturated red (R / (R + G + B) at least 0.8) that move its chromaticity more than 0.2
// in CIE 1976 u'v'; no more than three of either in any second. A tile flashes at its own rate. Over how much of the
// screen is the owner's: "Full screen flashes at most 3 times per second, but if there is some minor flashing deep
// inside a cloud, it shouldn't be limited", and of the choices, a quarter of the whole screen (2026-10-18): the rate a
// quarter of all the tiles reach is `flash`, which the sidechaining holds to three. WCAG's own area (a quarter of any 10°
// field of view, which it estimates as a third of the screen across and down) is `flashStrict`, measured and reported,
// not held: on rhythmic music the visuals' beat-lit parts pass three by it (4 to 8 a second, 2026-10-18).

/** One reading of the picture by tiles (flashmeter.ts): each tile's relative luminance, chromaticity (CIE 1976 u',
 *  v') and share of red (R / (R + G + B), linear light), `cols` x `rows` from the bottom left, at `t` seconds. */
export type Tiles = { t: number; cols: number; rows: number; luminance: Float32Array; u: Float32Array; v: Float32Array; red: Float32Array };

/** What the frame did, as the visuals' loop knows it (or the precheck's simulation). */
export type FrameStats = {
  /** Seconds, rising. */
  t: number;
  /** Each part's turning now (radians a second, signed). */
  turnRates: number[];
  /** Each part's evolving shape, hue (turns) and spread, as drawn. */
  shape: number[];
  hue: number[];
  spread: number[];
  /** The picture's size as drawn (its scale times a beat's swell). */
  scale: number;
  /** Its colourfulness as drawn (0..1.25). */
  saturation: number;
  /** A new look started this frame, and why (a track's own does not count against the budget). */
  look: 'track' | 'dice' | 'next' | null;
  /** A new symmetry this frame (a moment). */
  symmetry: boolean;
  /** Each part's weight in the drawing (Presence, 65: its share of the points against the others'); none: all alike. */
  weights?: number[];
  /** What the effects add (effects.ts `reportOf`), each a second, beside the flame's own. */
  extra?: Partial<Record<'motion' | 'shapeChurn' | 'colourChurn' | 'zoomPumping', number>>;
  /** The picture's tiles, where a reading arrived this frame (one frame may bring several, or none). */
  tiles: Tiles[];
};

/** The meters now. */
export type MeterValues = {
  /** General flashes a second over a quarter of the screen (the owner's area; held to 3). */
  flash: number;
  /** Red flashes a second over a quarter of the screen (held to 3). */
  redFlash: number;
  /** Either over three a second (over a quarter of the screen). */
  flashFails: boolean;
  /** General and red flashes a second by WCAG's own area (a quarter of a third-by-third window): reported, not held. */
  flashStrict: number;
  redFlashStrict: number;
  /** How fast the parts turn: their mean |turning| (radians a second). */
  motion: number;
  /** How fast the parts change shape and spread: their mean |change| a second. */
  shapeChurn: number;
  /** How fast the colours change: the parts' mean |hue change| (turns a second) and the saturation's. */
  colourChurn: number;
  /** How much the picture pumps: |change in log size| a second. */
  zoomPumping: number;
  /** New looks (the dice's and the moments', not a track's own) in the last minute. */
  looksPerMinute: number;
};

export const FLASH_STEP = 0.1;
export const FLASH_DARK = 0.8;
export const RED_SHARE = 0.8;
export const RED_SHIFT = 0.2;
/** WCAG's most flashes a second. */
export const MOST_FLASHES = 3;
/** The meters other than the flashes follow over this long (seconds). */
const FOLLOW_S = 1;
/** D65's chromaticity: where black (no colour to speak of) is put. */
const WHITE_U = 0.1978;
const WHITE_V = 0.4683;

/** CIE 1976 u'v' of linear sRGB (0..1 each); black as white. */
export function chromaticity(r: number, g: number, b: number): [number, number] {
  const x = 0.4124 * r + 0.3576 * g + 0.1805 * b;
  const y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  const z = 0.0193 * r + 0.1192 * g + 0.9505 * b;
  const d = x + 15 * y + 3 * z;
  return d > 1e-6 ? [(4 * x) / d, (9 * y) / d] : [WHITE_U, WHITE_V];
}

/** The transitions of one tile, general and red, and their times. */
class TileTrack {
  private ext = NaN;
  private dir = 0;
  private wasRed: boolean | null = null;
  private lastU = WHITE_U;
  private lastV = WHITE_V;
  private redU = WHITE_U;
  private redV = WHITE_V;
  general: number[] = [];
  red: number[] = [];

  add(t: number, l: number, u: number, v: number, share: number) {
    // General: a turn of at least FLASH_STEP from the last extreme, the darker side under FLASH_DARK.
    const turns = Math.abs(l - this.ext) >= FLASH_STEP && Math.min(l, this.ext) < FLASH_DARK;
    if (Number.isNaN(this.ext)) this.ext = l;
    else if (this.dir !== 0 && (this.dir > 0 ? l >= this.ext : l <= this.ext)) this.ext = l;
    else if (turns) {
      this.dir = l > this.ext ? 1 : -1;
      this.ext = l;
      this.general.push(t);
    }
    // Red: into or out of a saturated red, its chromaticity moved more than RED_SHIFT.
    const isRed = share >= RED_SHARE && l > 0.001;
    if (this.wasRed !== null && isRed !== this.wasRed) {
      const [fu, fv] = isRed ? [this.lastU, this.lastV] : [this.redU, this.redV];
      if (Math.hypot(u - fu, v - fv) > RED_SHIFT) this.red.push(t);
    }
    if (isRed) [this.redU, this.redV] = [u, v];
    else [this.lastU, this.lastV] = [u, v];
    this.wasRed = isRed;
  }

  /** Transitions within the last second before `t` (older ones let go). */
  count(t: number): [number, number] {
    const keep = (a: number[]) => {
      while (a.length && a[0] <= t - 1) a.shift();
      return a.length;
    };
    return [keep(this.general), keep(this.red)];
  }
}

/** Every tile's flashes, and the screen's rate as WCAG measures it (a quarter of a third-by-third window). */
export class Flashes {
  private tracks: TileTrack[] = [];
  private cols = 0;
  private rows = 0;

  add(tiles: Tiles) {
    if (tiles.cols !== this.cols || tiles.rows !== this.rows) {
      this.cols = tiles.cols;
      this.rows = tiles.rows;
      this.tracks = Array.from({ length: tiles.cols * tiles.rows }, () => new TileTrack());
    }
    this.tracks.forEach((track, i) => track.add(tiles.t, tiles.luminance[i], tiles.u[i], tiles.v[i], tiles.red[i]));
  }

  /** General and red flashes a second at `t`: over a quarter of the screen (the owner's area), and by WCAG's window. */
  rates(t: number): { general: number; red: number; strictGeneral: number; strictRed: number } {
    if (!this.tracks.length) return { general: 0, red: 0, strictGeneral: 0, strictRed: 0 };
    const counts = this.tracks.map((track) => track.count(t));
    const general = counts.map((c) => c[0] / 2);
    const red = counts.map((c) => c[1] / 2);
    return {
      general: quarterOf(general),
      red: quarterOf(red),
      strictGeneral: worstWindow(general, this.cols, this.rows),
      strictRed: worstWindow(red, this.cols, this.rows),
    };
  }
}

/** The rate at least a quarter of all the tiles reach. */
export function quarterOf(rates: number[]): number {
  if (!rates.length) return 0;
  const sorted = [...rates].sort((a, b) => b - a);
  return sorted[Math.max(0, Math.ceil(sorted.length / 4) - 1)];
}

/** WCAG's area: the rate at least a quarter of some window of a third of the tiles across and down reaches. */
export function worstWindow(rates: number[], cols: number, rows: number): number {

  const wc = Math.max(1, Math.ceil(cols / 3));
  const wr = Math.max(1, Math.ceil(rows / 3));
  let worst = 0;
  for (let y = 0; y + wr <= rows; y++) {
    for (let x = 0; x + wc <= cols; x++) {
      const inside: number[] = [];
      for (let j = 0; j < wr; j++) for (let i = 0; i < wc; i++) inside.push(rates[(y + j) * cols + x + i]);
      inside.sort((a, b) => b - a);
      worst = Math.max(worst, inside[Math.max(0, Math.ceil(inside.length / 4) - 1)]);
    }
  }
  return worst;
}

/** How much of the points moves between parts from weights `b` to `a`: half the summed change of each one's share. */
function sharesMoved(a: number[], b: number[]): number {
  const sa = a.reduce((x, w) => x + w, 0);
  const sb = b.reduce((x, w) => x + w, 0);
  if (!(sa > 0 && sb > 0) || a.length !== b.length) return 0;
  return a.reduce((x, w, i) => x + Math.abs(w / sa - b[i] / sb), 0) / 2;
}

/** All the meters, frame by frame. */
export class Meters {
  private flashes = new Flashes();
  private last: FrameStats | null = null;
  private looks: number[] = [];
  private motion = 0;
  private shape = 0;
  private colour = 0;
  private zoom = 0;
  private flash = { general: 0, red: 0, strictGeneral: 0, strictRed: 0 };

  add(s: FrameStats) {
    for (const tiles of s.tiles) this.flashes.add(tiles);
    this.flash = this.flashes.rates(s.t);
    if (s.look === 'dice' || s.look === 'next') this.looks.push(s.t);
    while (this.looks.length && this.looks[0] <= s.t - 60) this.looks.shift();
    const was = this.last;
    this.last = s;
    if (!was) return;
    const dt = s.t - was.t;
    if (!(dt > 0)) return;
    const k = 1 - Math.exp(-dt / FOLLOW_S);
    const mean = (v: number[]) => (v.length ? v.reduce((a, x) => a + x, 0) / v.length : 0);
    const change = (a: number[], b: number[]) => mean(a.map((x, i) => Math.abs(x - (b[i] ?? x)))) / dt;
    // A hue that comes round is no change: its shortest way.
    const hueChange = mean(s.hue.map((h, i) => Math.abs((((h - (was.hue[i] ?? h)) % 1) + 1.5) % 1 - 0.5))) / dt;
    const x = s.extra ?? {};
    this.motion += (mean(s.turnRates.map(Math.abs)) + (x.motion ?? 0) - this.motion) * k;
    // A new look jumps by design: that frame's change is the look's, counted in looksPerMinute, not churn.
    const jumped = s.look !== null;
    // Presence (65) moves the picture's points between its parts: the share that moves counts as a part's shape
    // changing by as much (a part flaring from a sixth of the points to a third, against the others, as much as one
    // part's shape changing by a sixth).
    const moved = s.weights && was.weights ? sharesMoved(s.weights, was.weights) / dt : 0;
    this.shape += ((jumped ? 0 : change(s.shape, was.shape) + change(s.spread, was.spread) + moved) + (x.shapeChurn ?? 0) - this.shape) * k;
    this.colour += ((jumped ? 0 : hueChange + Math.abs(s.saturation - was.saturation) / dt) + (x.colourChurn ?? 0) - this.colour) * k;
    const pump = s.scale > 0 && was.scale > 0 ? Math.abs(Math.log(s.scale / was.scale)) / dt : 0;
    this.zoom += (pump + (x.zoomPumping ?? 0) - this.zoom) * k;
  }

  now(): MeterValues {
    return {
      flash: this.flash.general,
      redFlash: this.flash.red,
      flashFails: Math.floor(this.flash.general) > MOST_FLASHES || Math.floor(this.flash.red) > MOST_FLASHES,
      flashStrict: this.flash.strictGeneral,
      redFlashStrict: this.flash.strictRed,
      motion: this.motion,
      shapeChurn: this.shape,
      colourChurn: this.colour,
      zoomPumping: this.zoom,
      looksPerMinute: this.looks.length,
    };
  }
}

/** One tile standing for the whole screen, from a luminance (and colour) the picture would show: the precheck's,
 *  which simulates without drawing. */
export function screenTile(t: number, luminance: number, rgb: [number, number, number] = [luminance, luminance, luminance]): Tiles {
  const [u, v] = chromaticity(...rgb);
  const sum = rgb[0] + rgb[1] + rgb[2];
  return {
    t,
    cols: 1,
    rows: 1,
    luminance: Float32Array.of(luminance),
    u: Float32Array.of(u),
    v: Float32Array.of(v),
    red: Float32Array.of(sum > 1e-6 ? rgb[0] / sum : 0),
  };
}
