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

/** The visuals open as the page reloaded for a new build (update.svelte.ts, sessionStorage): they open again. */
export const REOPEN = 'cratarr-visuals-reopen';
const reopened = (() => {
  try {
    const open = sessionStorage.getItem(REOPEN) === '1';
    sessionStorage.removeItem(REOPEN);
    return open;
  } catch {
    return false;
  }
})();

/** `auto`: opened by themselves (lib/visualsAuto), a touch closing them. */
export const visuals = $state({ open: reopened, side: keptSide(), auto: false });

/** The visuals beside the page (true) or over it. */
export function setVisualsSide(side: boolean) {
  visuals.side = side;
  try {
    localStorage.setItem(SIDE, side ? '1' : '0');
  } catch {
    /* for this visit only */
  }
}
