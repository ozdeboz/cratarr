// The tape's state, said once (0.10; the architecture audit, 2026-10-29: the tape was two dozen flags and a check
// that put stray ones right). What the tape is doing is one of a few phases, read from what the player holds: its ramp
// (lib/player/rampSlot), its motion (lib/player/motionSlot), its hold. Its passing states and their deadlines are
// lib/player/graces. Pure.

/** What the tape is doing. `ramp`: a Start and stop effect's (or the tape's own) wind-up or wind-down, no wind;
 *  `arming`: a hold pressed, its wind not sounding yet; `winding`: a wind sounding (to a point, or held); `braking`: a
 *  wind let go, slowing to where it lands; `plainHold`: a hold on a track that cannot be wound; `gliding`: a wind on
 *  the player driven, the counter between its reports. */
export type TapePhase = 'idle' | 'ramp' | 'arming' | 'winding' | 'braking' | 'plainHold' | 'gliding';

/** What the phase and the rules read of the player's tape (lib/localPlayer's `tape`, and the knob's state). */
export type TapeView = {
  committed: boolean | null;
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
