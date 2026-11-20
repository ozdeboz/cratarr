// The device's Back (BACK's second part; the owner, 2026-11-20: "sometimes using the phone back feature, it closes the app instead of
// following the back button that is clearly present"). The app asks the page (`{type: 'back'}`), and the page runs exactly its own
// Back, the one it shows: an open sheet first, as Esc closes it; then what is a step of the history over the page (the panel's
// trail, lib/panel's guard; the player over everything; the visuals), and the page's own history; then Home. Only where the page
// shows no Back (Home at the visit's first step) does it say so, and the app goes to the background. Pure, so the tests take it.

/** What the page's Back does now. */
export type BackDoes = 'sheet' | 'history' | 'home' | 'none';

/** Where the page stands: a sheet open; the step shown one the page put over itself (the panel's guard, the player, the
 *  visuals); the visit's step (`navAt`, 0 for the one it opened with); Show all of a search open; the page. */
export type BackFacts = { sheet: boolean; over: boolean; navAt: number; searchAll: boolean; page: string };

export function backDoes(f: BackFacts): BackDoes {
  if (f.sheet) return 'sheet';
  if (f.over || f.navAt > 0 || f.searchAll) return 'history';
  return f.page === 'home' ? 'none' : 'home';
}

/** The step shown is one the page put over itself, which Back takes first: the panel's trail, the player, the visuals. */
export const overThePage = (state: unknown) => {
  const st = (state ?? {}) as Record<string, unknown>;
  return !!(st.panelGuard || st.player || st.visuals);
};
