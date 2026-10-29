// Each band's Envelope (65, decision 0050, the definition the owner agreed): how present the band's sound is now.
// Pure, so the visuals and the precheck (both stepping music.svelte.ts's `musicOf` state) follow it alike.

import { B } from './routes';

/** At most this share of a band's level while its sound holds: a pad glows at it, a hit's flare stands out above it. */
export const SUSTAIN = 0.6;
/** How fast a band's hold rises to what its sound holds (s). */
export const RISE_S = 0.05;

/** What the envelopes keep from frame to frame: each band's signals (`B` order; its Envelope written there), each band's
 *  hold, and when they were last followed (ms; −1 never). */
export type Envelopes = { bands: Float32Array[]; held: Float32Array; heldAt: number };

/** Each band's Envelope at `at` (ms): its Hits signal (each hit's flare, risen into where it is known ahead, ringing
 *  down over the band's Linger) or what the sound still holds, whichever is more. The hold follows `SUSTAIN` × the
 *  band's level: up over `RISE_S`, down over the band's Linger (`lighting`, s), as a hit's light rings down. So a struck
 *  drum flares and is gone as its level drops; a pad, never struck, glows at its level; a struck chord flares, then
 *  holds while it rings. The first call, or one after a jump back, starts each hold where its sound is. */
export function followEnvelopes(st: Envelopes, at: number, lighting: readonly number[]) {
  const dt = st.heldAt < 0 ? -1 : Math.min(0.25, (at - st.heldAt) / 1000);
  for (let b = 0; b < st.bands.length; b++) {
    const v = st.bands[b];
    const target = SUSTAIN * Math.min(1, Math.max(0, v[B.level]));
    if (dt < 0) st.held[b] = target;
    else {
      const held = st.held[b];
      const time = target > held ? RISE_S : Math.max(0.02, lighting[b] ?? 0.15);
      st.held[b] = held + (target - held) * (1 - Math.exp(-dt / time));
    }
    v[B.envelope] = Math.min(1, Math.max(v[B.hits], st.held[b]));
  }
  st.heldAt = at;
}
