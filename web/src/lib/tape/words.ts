// What the deck says when the tape cannot do what it was turned to (the owner's truth rule, 2026-11-03: the knob stays
// as turned, and the reason shows; a5's app sends `{type: 'tape', refused, way}`, the page's own head the same words).

/** Why a wind could not sound: its sound came too late, or Cratarr could not be reached for it. */
export type Refused = 'late' | 'unreachable';
/** Which way it was to go: backwards (the knob at L), or a wind forwards. */
export type Way = 'back' | 'forward';

/** The deck's one line for a refusal; null for a reason it does not know (nothing said rather than a wrong word). */
export function refusedText(refused: unknown, way: unknown): string | null {
  const why = refused === 'unreachable' ? "Cratarr isn't reachable" : refused === 'late' ? 'its sound came too late' : null;
  if (!why) return null;
  return `${way === 'back' ? "Can't play backwards" : "Can't wind"}: ${why}`;
}

/** What the deck says while a wind waits for its track's file (0102: a wind pressed before the file has come waits). */
export const WAITS_FOR_FILE = 'Winds once the track is here';
