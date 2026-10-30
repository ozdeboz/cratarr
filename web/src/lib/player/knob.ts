// The direction knob and the holds against it (0.10.1; the owner, 2026-10-30: holding back came out playing forwards,
// the knob at L ended in forwards play, holding back while playing backwards did nothing). What lib/localPlayer does
// at each of those moments, said once. Pure.

/** The knob: what is wanted (L backwards, R forwards), and the tape's way, which follows it from a stop. */
export type Knob = { want: 'L' | 'R'; state: 'forward' | 'stopping' | 'backward' | 'returning' };
/** What sounds of the tape: playing backwards at 1× (`reverse`), or a hold flown from it that plays backwards again
 *  when let go (`backTo`); null: nothing of the tape sounds. */
export type Sounding = { reverse: boolean; backTo: boolean } | null;

/** Another track, or the track again from its start (Next, Previous, a jump, a restart): forwards, the knob at R. */
export const SKIPPED: Knob = { want: 'R', state: 'forward' };

/** The knob as what sounds has it: backwards only while something plays backwards, or a hold that returns to it; the
 *  knob put back with it, so it never shows L while the tape plays forwards. Null: it stands as it is. */
export function knobFor(k: Knob, s: Sounding): Knob | null {
  if (k.state !== 'backward' || s?.reverse || s?.backTo) return null;
  return SKIPPED;
}

/** A wind not sounding in time (or failing before it sounds): a tap's or a seek's goes there plainly, and so does what
 *  hands over to it (a flight from play, a let-go's brake). A hold let go already plays on where it was. A hold still
 *  held, or the knob's backwards, asks once more and waits longer; then the tape rests where it stood, never playing on
 *  the other way. A play the browser refused is not asked again. */
export type Late = { point: boolean; handed: boolean; held: boolean; reverse: boolean; retried: boolean; refused: boolean };
export function notSounding(l: Late): 'plain' | 'retry' | 'rest' {
  if (l.point || l.handed) return 'plain';
  if (!l.held && !l.reverse) return 'plain';
  return l.retried || l.refused ? 'rest' : 'retry';
}

/** A hold while playing backwards: back flies on from it (no stop, faster and faster); forward stops it, the knob to
 *  R, and winds forwards from the stop. */
export const holdInReverse = (dir: 1 | -1): 'fly' | 'turn' => (dir < 0 ? 'fly' : 'turn');

/** Where a hold let go brakes to (× the track's): forwards its own 1×; backwards a stand, or, flown from playing
 *  backwards with the knob still at L, 1× backwards, playing on that way. */
export const letGoTo = (dir: 1 | -1, backTo: boolean, want: Knob['want']): 0 | 1 => (dir > 0 || (backTo && want === 'L') ? 1 : 0);

/** A hold let go, its brake run out: on the way it was going at 1× backwards (`backwards`), forwards from there, or
 *  resting there (a pause pressed meanwhile). */
export function afterLetGo(dir: 1 | -1, backTo: boolean, want: Knob['want'], after: 'play' | 'pause'): 'backwards' | 'forwards' | 'rest' {
  if (after === 'pause') return 'rest';
  return dir < 0 && letGoTo(dir, backTo, want) === 1 ? 'backwards' : 'forwards';
}
