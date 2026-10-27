// A wind's shape (decision 0074; the owner, 2026-10-27): an envelope on the tape's speed. The server's stream runs at
// the speed limit; the player's own ramp (`rate`, 0..1) spins it up over the attack and brakes it over the release, on
// raised cosines. A tap lasts `ratio` seconds per minute it winds (at most WIND_CAP_MS); one shorter than attack plus
// release peaks lower, its release starting from where its attack had got. Shared by the settings' graph and the
// players, so what is drawn is what plays.
import { WIND_CAP_MS, type Wind } from './startStop';

/** A tap's length (ms) for winding `distance` seconds of track. */
export const tapMs = (w: Wind, distance: number) => Math.min(WIND_CAP_MS, Math.max(200, (w.ratio * Math.abs(distance) * 1000) / 60));

/** The player's rate at `t` ms into a tap of `D` ms (0..1). */
export function rate(w: Wind, D: number, t: number): number {
  const up = t >= w.attackMs ? 1 : (1 - Math.cos((Math.PI * Math.max(0, t)) / w.attackMs)) / 2;
  const left = D - t;
  const down = left >= w.releaseMs ? 1 : (1 - Math.cos((Math.PI * Math.max(0, left)) / w.releaseMs)) / 2;
  return Math.min(up, down);
}

/** The stream time (ms) a tap of `D` ms has consumed by `t`: the integral of its rate. */
export function consumed(w: Wind, D: number, t: number, step = 4): number {
  let c = 0;
  for (let s = 0; s < Math.min(t, D); s += step) c += rate(w, D, s + step / 2) * Math.min(step, t - s);
  return c;
}

/** How a tap of `distance` seconds plays: its length, the stream it consumes (ms), whether it is cut (the speed limit
 *  over that stream covers less track than the distance) and into how many pieces. */
export function tapPlan(w: Wind, distance: number) {
  const D = tapMs(w, distance);
  const C = consumed(w, D, D);
  const covered = (C / 1000) * w.speed;
  const cut = covered < Math.abs(distance);
  // The cruise's stream time between the ramps, cut into pieces of about pieceMs.
  const cruise = Math.max(0, D - w.attackMs - w.releaseMs);
  const pieces = cut ? Math.max(1, Math.round(cruise / w.pieceMs)) : 0;
  return { D, C, cut, pieces, peak: Math.max(...Array.from({ length: 64 }, (_, i) => rate(w, D, (D * i) / 63))) };
}
