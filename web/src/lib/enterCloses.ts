// Enter in a text field with something in it closes the keyboard, in every field at once (the owner, 2026-10-25: "not
// just one"): the field loses focus once its own handlers have had the key (a search runs, a name saves). Listened
// for on the window, so a field that stops the key (to keep typing on) keeps its focus.
const TEXT = new Set(['text', 'search', 'email', 'url', 'tel', 'number', 'password']);

export function enterCloses() {
  addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' || e.isComposing || e.shiftKey) return;
    const el = e.target;
    if (!(el instanceof HTMLInputElement) || !TEXT.has(el.type) || !el.value) return;
    el.blur();
  });
}
