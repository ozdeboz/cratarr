// Cratarr's desktop app (decision 0078): these pages in a window of the app's own, the music played by the page as in a
// browser. The app says so on `window.CratarrDesktop` (desktop/preload.js): its version and the computer's name. There
// the page counts as installed (Downloads), and the computer's name is the player's first name.

type Desktop = { version: string | null; name: string | null };

/** The desktop app this page runs in, or null (a browser, the Android app). */
export const desktop: Desktop | null =
  typeof window === 'undefined' ? null : ((window as unknown as { CratarrDesktop?: Desktop }).CratarrDesktop ?? null);
