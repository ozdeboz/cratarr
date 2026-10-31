// One thing moving between its places (the owner, 2026-10-31: "making a panel full screen, it first closes before
// showing it full screen; I see a flash of the page content … I need this entire class of bug eradicated"): beside the
// page (the panel, the player in it), over the page (a viewed playlist's layer) and over everything (the player). The
// place it leaves stays drawn until the place it goes to shows (lib/Hold's `onshown`, or the player's first frame);
// then the old one goes in the frame after, and the new one comes in without an entrance that would show the page
// through it. Opening from nothing or closing to nothing is not a hand-over: those keep their own entrances.

export type Places = { beside: boolean; over: boolean; overlay: boolean };
export const KEYS = ['beside', 'over', 'overlay'] as const;
export const NONE: Places = { beside: false, over: false, overlay: false };

/** How long what comes may take to show before what it takes over from goes anyway (lib/Hold's HOLD_MAX_MS). */
export const HANDOVER_GIVE_UP_MS = 3000;

/** The places asked for and the ones held: what is drawn. */
export const drawn = (to: Places, held: Places): Places => ({ beside: to.beside || held.beside, over: to.over || held.over, overlay: to.overlay || held.overlay });

/** Whether anything is held (a hand-over under way: what comes enters without its entrance). */
export const handing = (held: Places) => KEYS.some((k) => held[k]);

/** What stays drawn once the places asked for go from `from` to `to`, `held` being held: where a place comes that was not
 *  drawn, every place drawn and no longer asked for, until it shows; where none comes (closing, or back to a place still
 *  drawn, so already shown), nothing. */
export function nextHeld(held: Places, from: Places, to: Places): Places {
  const was = drawn(from, held);
  const comes = KEYS.some((k) => to[k] && !was[k]);
  const out = { ...NONE };
  if (comes) for (const k of KEYS) out[k] = was[k] && !to[k];
  return out;
}
