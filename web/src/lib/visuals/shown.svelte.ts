// Whether the full-screen visuals show (decision 0046): opened from the full player, drawn by the shell (App.svelte),
// so the player moving beside or over the page (the window resizes as it goes full screen) does not close them.
// `side`: beside the page, in the panel at the right where the player goes (on a window roomy enough), rather than over
// the whole page; remembered on this browser, as the player's own place is.

const SIDE = 'cratarr-visuals-side';
const keptSide = () => {
  try {
    return localStorage.getItem(SIDE) === '1';
  } catch {
    return false;
  }
};

export const visuals = $state({ open: false, side: keptSide() });

/** The visuals beside the page (true) or over it. */
export function setVisualsSide(side: boolean) {
  visuals.side = side;
  try {
    localStorage.setItem(SIDE, side ? '1' : '0');
  } catch {
    /* for this visit only */
  }
}
