// The sidechaining's budgets (decision 0048): the meters that have one, and what a limit allows as the Calm ↔ Wild setting
// and the music have it. The one scale the sidechaining, the dice's precheck, the ratings and the thumbs all judge on: a
// meter over its budget at Wild's middle (`budgetOf(1, 0.5, activity)`) is how far it is from a limit, whatever the
// music was doing. Apart from engine/sidechain.ts so the precheck (whose limits the sidechaining stands on) can take it too.


/** The meters with a budget. */
export const BUDGETED = ['motion', 'shapeChurn', 'colourChurn', 'zoomPumping', 'looksPerMinute'] as const;
export type Budgeted = (typeof BUDGETED)[number];

/** The most the defaults do (the same runs' most, the flame's own: the looks without effects, as the limits take it),
 *  the other end of their usual range; new looks a minute: the limit. */
export const USUAL_MOST: Readonly<Record<Budgeted, number>> = {
  motion: 0.466,
  shapeChurn: 0.638,
  colourChurn: 0.354,
  zoomPumping: 0.351,
  looksPerMinute: 4,
};

/** A budget as the Calm ↔ Wild setting (0..1: half as much to twice, as much at its middle) and the music (its
 *  activity now, 0..1: half as much when quiet, one and a half times when loud) have it. */
export const budgetOf = (limit: number, wild: number, activity: number) => limit * 2 ** (2 * (wild - 0.5)) * (0.5 + Math.min(1, Math.max(0, activity)));

/** The limits a recipe is held to (the sidechaining's, at the Calm ↔ Wild setting). */
export type Limits = Partial<Record<'flash' | 'motion' | 'shapeChurn' | 'colourChurn' | 'zoomPumping' | 'looksPerMinute', number>>;

/** Until the owner's ratings fit them (fit.ts), on the sidechaining's scale: a little above (a quarter more, up to the next
 *  0.05) the most the flame itself did as Standard over 11 varied tracks, at a quarter and three fifths of each, three
 *  track looks a track (65, 2026-10-18, 506 runs: motion's most 0.47, shape's 0.64 (64: 0.42, before the meters saw
 *  Presence move the points between the parts), colour's 0.35, pumping's 0.35; flashes 3), so today's behaviour always
 *  passes, quiet passage or loud. The looks' own effects are drawn within the room these leave above that most
 *  (`DRAWN_ROOM`), so a track's look never needs holding; the limits are not raised for them (the wildest of
 *  65's first draws doubled motion's most); held to it, the looks with effects peak where the flame does. The dice's
 *  recipes: 83 of 440 refused (motion 79: the zooms they roll count as the motion they are; flash 9).
 *  Flashes: three a second over a quarter of the screen, the rule. */
export const DEFAULT_LIMITS: Required<Limits> = {
  flash: 3,
  motion: 0.6,
  shapeChurn: 0.8,
  colourChurn: 0.45,
  zoomPumping: 0.45,
  // New looks a minute (the dice's and the moments', a track's own not counted): Standard has none of its own; four
  // keeps a moment-driven recipe from turning into a slideshow.
  looksPerMinute: 4,
};

/** The room a drawn look's effects have (65): what the limits leave above the most the flame itself does
 *  (`USUAL_MOST`), at the quietest passage's budget (Wild's middle, no activity: `budgetOf`'s least), raw a
 *  second as the effects report it (effects.ts). A look drawn for a track, by Another look or the dice is held to it
 *  (flame.ts), so it never needs the sidechaining; effects chosen by hand are not. */
export const DRAWN_ROOM: Readonly<Record<'motion' | 'shapeChurn' | 'colourChurn' | 'zoomPumping', number>> = {
  motion: (DEFAULT_LIMITS.motion - USUAL_MOST.motion) * budgetOf(1, 0.5, 0),
  shapeChurn: (DEFAULT_LIMITS.shapeChurn - USUAL_MOST.shapeChurn) * budgetOf(1, 0.5, 0),
  colourChurn: (DEFAULT_LIMITS.colourChurn - USUAL_MOST.colourChurn) * budgetOf(1, 0.5, 0),
  zoomPumping: (DEFAULT_LIMITS.zoomPumping - USUAL_MOST.zoomPumping) * budgetOf(1, 0.5, 0),
};
