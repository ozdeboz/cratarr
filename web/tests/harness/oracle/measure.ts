// A frame's numbers metered without a picture (frozen from the precheck, which the dice no longer run: decision
// 0077): the one engine's sidechaining's tests meter today's chain with it, against the governor it replaced.
import { wavesMotion, zoomsMotion, type AspectsNow } from '../../../src/lib/visuals/aspects.ts';
import { settingsReport, type Report } from '../../../src/lib/visuals/effects.ts';
import { screenTile, type Meters } from '../../../src/lib/visuals/meters.ts';
import { A } from '../../../src/lib/visuals/routes.ts';
import type { VisualSettings } from '../../../src/lib/visuals/settings.svelte.ts';
import { DIVE_MOST } from '../../../src/lib/visuals/zoom.ts';

/** How bright the flame's dense parts are on the screen, about (relative luminance): the flashes' scale. */
const BRIGHT = 0.35;

/** A report with `motion` more (a second). */
const withMotion = (r: Report, motion: number): Report => (motion > 0 ? { ...r, motion: (r.motion ?? 0) + motion } : r);

/** A frame's mapping, measured as the visuals measure it, without a picture: the flashes from the brightness the whole
 *  picture would pulse with (the glow, the parts' lights), one tile for the screen. The precheck's, and the one
 *  engine's sidechaining's tests (decision 0057). `divePerRate`: the dive's turn a second at a rate of 1 (0: none). */
export function measure(meters: Meters, a: AspectsNow, t: number, settings: VisualSettings, divePerRate = 0) {
  const lights = a.hits.reduce((s, h) => s + Math.max(0, h), 0) / Math.max(1, a.hits.length);
  const luminance = Math.min(1, BRIGHT * a.flash * (1 + 0.6 * lights));
  meters.add({
    t,
    turnRates: divePerRate && a.dive ? a.rates.map((r) => r + DIVE_MOST * a.dive * divePerRate) : a.rates,
    shape: a.shape,
    weights: a.parts.map((p) => p.weight ?? 1),
    hue: a.parts.map((p) => p.hue),
    spread: a.parts.map((p) => p.spread ?? 1),
    scale: a.scale,
    saturation: a.saturation,
    // A moment (another symmetry, lens, effect…) is a new look, as the visuals count it (65.1: it was never counted).
    look: Object.values(a.nexts).some(Boolean) ? 'next' : null,
    symmetry: a.nexts.symmetry,
    tiles: [screenTile(t, luminance)],
    // What the look's effects add, the waves where nothing flows and the zooms' streaming out, as the visuals' meters
    // take them (65): so a recipe's effects, waves and zooms are held to the limits.
    extra: withMotion(settingsReport(settings, a.effects, undefined, a.routed), wavesMotion(a.given[A.waves], a.fluid) + zoomsMotion(a.tunnel, a.dive)),
  });
}
