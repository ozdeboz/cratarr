// The sound as it plays, for the visuals while the server's signals for the track are not there yet (decision 0046,
// amended: the server works them out the first time a track is asked for, or ahead of it in a queue; until then, here).
// Listens beside the page player's own route (`audioTap`), adds nothing to what is heard, and gives what format 3's
// band sections give, from running statistics instead of the whole track: each band's level (dB over the 30 under its
// running loudest) and flux (against its running loudest), its hits (picked as the server picks them: a peak of the
// flux over the median around it), and the loudness as heard (EBU R128 momentary, through its K-weighting filters).
// Only what can be heard as it plays: novelty, sections, bars and harmonic change wait for the server.

/** Each band's edges (Hz; 0 open), as the server's section 0 has them. */
export const LIVE_EDGES: readonly (readonly [number, number])[] = [
  [0, 60],
  [60, 150],
  [150, 400],
  [400, 2000],
  [2000, 5000],
  [5000, 0],
];

export type LiveBand = {
  /** 0..1: dB over the 30 under the band's loudest of the last half minute. */
  level: number;
  /** 0..1: against the band's loudest flux of the last half minute. */
  flux: number;
  /** How many hits the band has had: a new one arrives as this goes up. */
  hits: number;
  /** The last hit's strength, 0..1 (1: as strong as the band's strong hits lately). */
  lastHitStrength: number;
};

export type LiveNow = {
  /** LUFS, momentary (the last 400 ms or so); null before there is enough sound. */
  loudness: number | null;
  bands: LiveBand[];
};

/** Listened this often (ms): about the server's frame rate, whatever the screen's. */
const TICK_MS = 12;
/** The running statistics' memory (s), and how often their tops are worked out again. */
const MEMORY_S = 30;
const TOPS_EVERY_MS = 1000;
/** Magnitudes compressed as ln(1 + γ·|X|) before the flux, as on the server. */
const GAMMA = 100;
const RANGE_DB = 30;
/** Hits closer than this to the last one in their band are one (ms), as on the server. */
const GAP_MS = [100, 100, 80, 70, 60, 50];

/** A ring of the last values, with its 98th percentile (or another) worked out on demand. */
class Recent {
  private values: Float32Array;
  private at = 0;
  private filled = 0;

  constructor(size: number) {
    this.values = new Float32Array(size);
  }

  push(v: number) {
    this.values[this.at] = v;
    this.at = (this.at + 1) % this.values.length;
    this.filled = Math.min(this.filled + 1, this.values.length);
  }

  percentile(p: number, above = -Infinity): number | null {
    const v = Array.from(this.values.subarray(0, this.filled)).filter((x) => x > above);
    if (v.length === 0) return null;
    v.sort((a, b) => a - b);
    return v[Math.round((v.length - 1) * p)];
  }

  /** The last `n` values, oldest first. */
  last(n: number): number[] {
    const out: number[] = [];
    const len = this.values.length;
    for (let i = Math.min(n, this.filled); i > 0; i--) out.push(this.values[(this.at - i + len) % len]);
    return out;
  }
}

const median = (v: number[]) => {
  if (v.length === 0) return 0;
  const s = [...v].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
};

/**
 * Listens to `source` (the page player's route, before its gain) in `context`. `now()` is what it has heard lately;
 * `stop()` lets go of it.
 */
export function startLive(source: AudioNode, context: AudioContext): { now(): LiveNow; stop(): void } {
  const rate = context.sampleRate;
  // The spectrum: about 46 ms a window, as on the server, without the analyser's own smoothing.
  const spectral = context.createAnalyser();
  spectral.fftSize = rate > 64_000 ? 4096 : 2048;
  spectral.smoothingTimeConstant = 0;
  // The loudness: R128's K-weighting (a high shelf, then a high pass), then about 400 ms of the sound.
  const shelf = context.createBiquadFilter();
  shelf.type = 'highshelf';
  shelf.frequency.value = 1681.97;
  shelf.gain.value = 3.99984;
  const pass = context.createBiquadFilter();
  pass.type = 'highpass';
  pass.frequency.value = 38.135;
  pass.Q.value = 0.5003;
  const temporal = context.createAnalyser();
  temporal.fftSize = 2 ** Math.round(Math.log2(rate * 0.4));
  source.connect(spectral);
  source.connect(shelf).connect(pass).connect(temporal);
  const stereo = source.channelCount >= 2;

  const bins = spectral.frequencyBinCount;
  const db = new Float32Array(bins);
  const wave = new Float32Array(temporal.fftSize);
  const band = new Int8Array(bins).fill(-1);
  for (let k = 1; k < bins; k++) {
    const hz = (k * rate) / spectral.fftSize;
    band[k] = LIVE_EDGES.findIndex(([lo, hi]) => hz >= lo && (hi === 0 || hz < hi));
  }
  const previous = new Float32Array(bins);
  const ticksOf = (ms: number) => Math.max(1, Math.round(ms / TICK_MS));
  const memory = ticksOf(MEMORY_S * 1000);
  const levels = LIVE_EDGES.map(() => new Recent(memory));
  const fluxes = LIVE_EDGES.map(() => new Recent(memory));
  const strengths = LIVE_EDGES.map(() => new Recent(64));
  const tops = LIVE_EDGES.map(() => ({ level: -100, flux: 1e-6, strong: 1e-6 }));
  const bands: LiveBand[] = LIVE_EDGES.map(() => ({ level: 0, flux: 0, hits: 0, lastHitStrength: 0 }));
  const lastHit = LIVE_EDGES.map(() => -Infinity);
  let loudness: number | null = null;
  let toppedAt = 0;
  let tick = 0;
  // The analyser's dB as a magnitude (its own scale: only how the flux compresses depends on it).
  const toMagnitude = (d: number) => (d === -Infinity ? 0 : 10 ** (d / 20));

  const listen = () => {
    tick++;
    spectral.getFloatFrequencyData(db);
    const energy = new Float64Array(LIVE_EDGES.length);
    const flux = new Float64Array(LIVE_EDGES.length);
    for (let k = 1; k < bins; k++) {
      const b = band[k];
      const m = toMagnitude(db[k]);
      const y = Math.log1p(GAMMA * m);
      const rise = Math.max(0, y - previous[k]);
      previous[k] = y;
      if (b < 0) continue;
      energy[b] += m * m;
      flux[b] += rise;
    }
    const now = performance.now();
    if (now - toppedAt > TOPS_EVERY_MS) {
      toppedAt = now;
      tops.forEach((t, b) => {
        t.level = levels[b].percentile(0.98, -100) ?? -100;
        t.flux = Math.max(fluxes[b].percentile(0.98, 0) ?? 1e-6, 1e-6);
        t.strong = Math.max(strengths[b].percentile(0.9, 0) ?? 1e-6, 1e-6);
      });
    }
    for (let b = 0; b < LIVE_EDGES.length; b++) {
      const level = 10 * Math.log10(energy[b] + 1e-12);
      levels[b].push(level);
      fluxes[b].push(flux[b]);
      const t = tops[b];
      bands[b].level = level > -100 ? Math.min(1, Math.max(0, (level - (t.level - RANGE_DB)) / RANGE_DB)) : 0;
      bands[b].flux = Math.min(1, flux[b] / t.flux);
      // A hit, three ticks late (about 35 ms): the flux then was the largest around it, over the median of the 120 ms
      // before and the 35 ms after it by a tenth of the band's top, and not under a sixth of it.
      const near = 3;
      const window = fluxes[b].last(ticksOf(120) + near + 1);
      const at = window.length - 1 - near;
      if (at < near) continue;
      const f = window[at];
      const around = window.slice(Math.max(0, at - near), at).concat(window.slice(at + 1));
      if (f < t.flux / 6 || around.some((x, i) => (i < near ? x >= f : x > f))) continue;
      if (f < median(window) + t.flux / 10) continue;
      const when = now - near * TICK_MS;
      if (when - lastHit[b] < GAP_MS[b]) continue;
      lastHit[b] = when;
      strengths[b].push(f);
      bands[b].hits++;
      bands[b].lastHitStrength = Math.min(1, f / t.strong);
    }
    // The loudness, about every 100 ms.
    if (tick % ticksOf(100) === 0) {
      temporal.getFloatTimeDomainData(wave);
      let sum = 0;
      for (const x of wave) sum += x * x;
      const meanSquare = sum / wave.length;
      // Two channels mixed to one by the analyser: as if both carried it (R128 sums the channels).
      loudness = meanSquare > 1e-10 ? -0.691 + 10 * Math.log10(meanSquare * (stereo ? 2 : 1)) : null;
    }
  };
  const timer = setInterval(listen, TICK_MS);

  return {
    now: () => ({ loudness, bands: bands.map((b) => ({ ...b })) }),
    stop: () => {
      clearInterval(timer);
      for (const node of [spectral, shelf]) {
        try {
          source.disconnect(node);
        } catch {
          // Already let go of.
        }
      }
      shelf.disconnect();
      pass.disconnect();
    },
  };
}
