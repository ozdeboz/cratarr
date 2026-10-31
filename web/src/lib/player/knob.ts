// The direction knob and the holds against it (0.10.1; the owner, 2026-10-30: holding back came out playing forwards,
// the knob at L ended in forwards play, holding back while playing backwards did nothing). What lib/localPlayer does
// at each of those moments, said once. Pure.
//
// Directions are pure (0.12.4; the owner, 2026-10-31: "continuous hold winding should never alter the direction from
// what it was before initiating it"): the knob is the setting, and only the user's turn of it changes it; the deck's L
// and R say the way the tape goes now (`travelOf`), whatever the setting.

/** The knob: what is wanted (L backwards, R forwards), and the tape's way, which follows it from a stop. */
export type Knob = { want: 'L' | 'R'; state: 'forward' | 'stopping' | 'backward' | 'returning' };
/** What sounds of the tape: playing backwards at 1× (`reverse`), or a hold flown from it that plays backwards again
 *  when let go (`backTo`); null: nothing of the tape sounds. */
export type Sounding = { reverse: boolean; backTo: boolean } | null;

/** A new queue plays forwards, the knob at R (a play asked anew, not a skip). */
export const SKIPPED: Knob = { want: 'R', state: 'forward' };

/** The tape's way as what sounds has it: `backward` only while something plays backwards, or a hold that returns to
 *  it; else it stands forwards, the setting (`want`) as the user left it. Null: it stands as it is. */
export function knobFor(k: Knob, s: Sounding): Knob | null {
  if (k.state !== 'backward' || s?.reverse || s?.backTo) return null;
  return { want: k.want, state: 'forward' };
}

/** The deck's L or R: the way the tape goes (its speed's sign; negative backwards), the last way kept while it stands
 *  or nearly (a brake through zero turns it there). */
export const travelOf = (speed: number, last: 'L' | 'R'): 'L' | 'R' => (speed < -0.02 ? 'L' : speed > 0.02 ? 'R' : last);

/** Another track (Next, Previous, a jump in the queue) plays the way the knob is set: backwards from its end at L. */
export const skipWay = (want: Knob['want']): 'forwards' | 'backwards' => (want === 'L' ? 'backwards' : 'forwards');

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

/** A hold while playing backwards: back flies on from it (no stop, faster and faster); forward stops it and winds
 *  forwards from the stop, the knob staying at L, so it plays backwards again once let go. */
export const holdInReverse = (dir: 1 | -1): 'fly' | 'turn' => (dir < 0 ? 'fly' : 'turn');

/** Where a hold let go brakes to (× the track's): the way the knob is set at 1×, a hold that way braking to it on the
 *  same way; one against it to a stand, the knob's way taking over from there. The knob's setting alone decides, from
 *  playing backwards or from a pause alike (0.12.4: a hold from a pause at L ended forwards; `_backTo` no longer counts). */
export const letGoTo = (dir: 1 | -1, _backTo: boolean, want: Knob['want']): 0 | 1 => ((dir < 0) === (want === 'L') ? 1 : 0);

/** A hold let go, its brake run out: on the way it was going at 1× backwards (`backwards`), forwards from there, or
 *  resting there (a pause pressed meanwhile). */
export function afterLetGo(_dir: 1 | -1, _backTo: boolean, want: Knob['want'], after: 'play' | 'pause'): 'backwards' | 'forwards' | 'rest' {
  if (after === 'pause') return 'rest';
  return want === 'L' ? 'backwards' : 'forwards';
}
