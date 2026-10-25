// Focus rings for the keyboard only (the owner, 2026-10-25: a long press on a chart, a slider or the waveform drew one).
// Android counts a long press as focus the keyboard's way, so a ring showed on touch; the page says how it was last
// used (`data-input` on its root: pointer after a touch or a click, keys after a key), and app.css hides every ring
// while it is the pointer.
export function inputWay() {
  const root = document.documentElement;
  const set = (way: 'pointer' | 'keys') => root.dataset.input !== way && (root.dataset.input = way);
  addEventListener('pointerdown', () => set('pointer'), true);
  addEventListener('keydown', (e) => !e.isComposing && e.key !== 'Unidentified' && set('keys'), true);
}
