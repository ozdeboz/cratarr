// The tape's state, said once (0.9.19; the architecture audit, 2026-10-29: the tape is two dozen flags and a check that
// puts stray ones right). What the tape is doing is one of a few phases, read from what the player holds; its rules
// (what may not hold together, and how long a passing state may stand) are one pure function, so the check that logs
// and mends them (lib/localPlayer checkTape) is these rules applied, and the rules are tested. Pure.

/** What the tape is doing. `ramp`: a Start and stop effect's (or the tape's own) wind-up or wind-down, no wind;
 *  `arming`: a hold pressed, its wind not sounding yet; `winding`: a wind sounding (to a point, or held); `braking`: a
 *  wind let go, slowing to where it lands; `plainHold`: a hold on a track that cannot be wound; `gliding`: a wind on
 *  the player driven, the counter between its reports. */
export type TapePhase = 'idle' | 'ramp' | 'arming' | 'winding' | 'braking' | 'plainHold' | 'gliding';

/** What the phase and the rules read of the player's tape (lib/localPlayer's `tape`, and the knob's state). */
export type TapeView = {
  committed: boolean | null;
  heldAt: number | null;
  rewinding: { brake: number | null; reverse: boolean } | null;
  holding: { dir: 1 | -1; released: boolean } | null;
  plainHold: unknown;
  glide: unknown;
  /** The ramp the button and the waveform show (null: none). */
  shownRamp: 'in' | 'out' | null;
  knob: 'forward' | 'stopping' | 'backward' | 'returning';
};

export function phaseOf(t: TapeView): TapePhase {
  if (t.rewinding) return t.rewinding.brake !== null ? 'braking' : 'winding';
  if (t.plainHold) return 'plainHold';
  if (t.glide) return 'gliding';
  if (t.holding) return 'arming';
  if (t.committed !== null) return 'ramp';
  return 'idle';
}

/** How long a passing state may stand before it is taken for stuck (ms): a ramp shown with none running (an app's word
 *  may take a moment), a turn of the knob, a hold with nothing under way (a flight arms within a second or two). */
export const GRACE = { shownRamp: 1000, knobTurn: 5000, idleHold: 4000 } as const;

/** Since when each passing state has stood (performance.now(); 0: it does not). */
export type Since = { shownRamp: number; knobTurn: number; idleHold: number };

/** A broken rule, and what puts it right (none: only logged). */
export type Broken = { rule: string; fix: 'none' | 'unholdCounter' | 'endShownRamp' | 'knobForward' | 'dropHold' };

/** The tape's rules at `now`: the ones broken, and since when each passing state stands (for the next check). */
export function tapeRules(t: TapeView, since: Since, now: number): { broken: Broken[]; since: Since } {
  const broken: Broken[] = [];
  const next: Since = { shownRamp: 0, knobTurn: 0, idleHold: 0 };
  const over = (k: keyof Since, on: boolean) => {
    if (!on) return false;
    next[k] = since[k] || now;
    if (since[k] && now - since[k] > GRACE[k]) {
      next[k] = 0;
      return true;
    }
    return false;
  };
  // A wind runs only committed to playing; a ramp's committed state never outlives it.
  if (t.rewinding && t.committed === null) broken.push({ rule: 'a wind without its commitment', fix: 'none' });
  // The counter held for a ramp only while one runs.
  if (t.heldAt !== null && t.committed === null) broken.push({ rule: 'the counter held with no ramp', fix: 'unholdCounter' });
  // The button's and the waveform's ramp shown only while a ramp or a wind runs.
  if (over('shownRamp', t.shownRamp !== null && t.committed === null && !t.rewinding)) broken.push({ rule: 'a ramp shown with none running', fix: 'endShownRamp' });
  // The knob's state and what sounds agree: backwards only while the tape plays backwards; a turn lasts moments.
  if (t.knob === 'backward' && !t.rewinding?.reverse) broken.push({ rule: 'the knob backwards with nothing playing backwards', fix: 'knobForward' });
  if (over('knobTurn', t.knob === 'stopping' || t.knob === 'returning')) broken.push({ rule: 'a turn of the knob stuck', fix: 'knobForward' });
  // A hold with neither its stop nor its wind under way for a while: over.
  if (over('idleHold', !!t.holding && !t.rewinding && t.committed === null)) broken.push({ rule: 'a hold with nothing under way', fix: 'dropHold' });
  return { broken, since: next };
}
