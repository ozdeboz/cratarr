// The tape's passing states and their deadlines (0.10; the architecture audit, 2026-10-29: no check that finds a
// stray state and puts it right). Some of what the tape does is meant to last moments: a ramp the button shows while
// the app's word for it is on its way, a turn of the direction knob, a hold whose wind has not begun (a flight arms
// within a second or two), the tape's own quiet rest between two of its actions. Each is a state with a deadline, set
// as it is entered and dropped as it is left; reaching it is a transition of its own (the shown ramp ends, the knob
// is forward, the hold is over, the rest is over), the same as the check made of it before, at the same moments. Pure.

export type Grace = 'shownRamp' | 'knobTurn' | 'idleHold' | 'quietRest';

/** How long each may stand (ms); the quiet rest's own deadline is set where it begins (`quietUntil`). */
export const GRACE_MS = { shownRamp: 1000, knobTurn: 5000, idleHold: 4000 } as const;

/** Which passing states stand now; the quiet rest with its deadline. */
export type Standing = { shownRamp: boolean; knobTurn: boolean; idleHold: boolean; quietRest: number | null };
/** Each one's deadline (performance.now()); null: it does not stand. */
export type Deadlines = Record<Grace, number | null>;

export const NO_DEADLINES: Deadlines = { shownRamp: null, knobTurn: null, idleHold: null, quietRest: null };

/** The deadlines after a change at `now`: a state entered gets its deadline, one left loses it, one still standing
 *  keeps the deadline it had. */
export function entered(d: Deadlines, s: Standing, now: number): Deadlines {
  const keep = (k: 'shownRamp' | 'knobTurn' | 'idleHold', on: boolean) => (on ? (d[k] ?? now + GRACE_MS[k]) : null);
  return { shownRamp: keep('shownRamp', s.shownRamp), knobTurn: keep('knobTurn', s.knobTurn), idleHold: keep('idleHold', s.idleHold), quietRest: s.quietRest };
}

/** The states whose deadline has passed at `now` (past it, as the check had it). */
export const due = (d: Deadlines, now: number): Grace[] => (Object.keys(d) as Grace[]).filter((k) => d[k] !== null && now > d[k]!);

/** When the next deadline falls; null: none stands. */
export function soonest(d: Deadlines): number | null {
  const at = Object.values(d).filter((v): v is number => v !== null);
  return at.length ? Math.min(...at) : null;
}
