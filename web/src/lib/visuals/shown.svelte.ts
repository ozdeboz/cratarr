// Whether the full-screen visuals show (decision 0046): opened from the full player, drawn by the shell (App.svelte),
// so the player moving beside or over the page (the window resizes as it goes full screen) does not close them.
export const visuals = $state({ open: false });
