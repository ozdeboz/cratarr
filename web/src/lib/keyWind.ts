// The keyboard's transport (the owner, 2026-10-27): ← and → as Previous and Next, a tap their tap, a hold their hold
// (winding back or forward for as long as held, lib/holdWind's way), where nothing else takes the arrows: not in a
// field, on a slider, the seek bar, a list or a menu, nor with a modifier held. Installed once by App.
import { appInfo, inApp } from './app.svelte';
import { prefs } from './prefs.svelte';
import { holdsHere } from './holdWind';
import { playing, send, wind, windTouch } from './remote.svelte';

const HOLD_MS = 350;

/** An element that keeps the arrows for itself. */
function takesArrows(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  if (t.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]')) return true;
  // A modal sheet's own (the full player, a dialog too but not a modal one, keeps them for the transport).
  return !!t.closest('[role="slider"], [role="listbox"], [role="menu"], [aria-modal="true"], [role="tablist"], [role="radiogroup"], [role="grid"]');
}
/** Whether holding that way winds where the music plays (as the player's buttons: lib/holdWind). */
const winds = (dir: 'back' | 'forward') => holdsHere(dir, playing()?.player);

type Down = { dir: 'back' | 'forward'; timer: ReturnType<typeof setTimeout> | undefined; held: boolean };

export function installKeyWind(): () => void {
  let down = null as Down | null;
  const press = (e: KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || takesArrows(e.target) || !playing()) return;
    // A sheet or a dialog open: its own.
    if (document.querySelector('[aria-modal="true"], dialog[open]')) return;
    e.preventDefault();
    // The key's own repeats while held: one press.
    if (e.repeat || down) return;
    const dir = e.key === 'ArrowLeft' ? 'back' : 'forward';
    const d: Down = { dir, timer: undefined, held: false };
    down = d;
    if (!winds(dir)) return;
    windTouch(dir, true);
    d.timer = setTimeout(() => {
      d.timer = undefined;
      d.held = true;
      wind(dir, 'start');
    }, HOLD_MS);
  };
  const lift = (e: KeyboardEvent) => {
    const d = down;
    if (!d || (e.key !== (d.dir === 'back' ? 'ArrowLeft' : 'ArrowRight') && e.type === 'keyup')) return;
    down = null;
    clearTimeout(d.timer);
    if (d.held) return wind(d.dir, 'stop');
    if (winds(d.dir)) windTouch(d.dir, false);
    void send(d.dir === 'back' ? 'previous' : 'next');
  };
  // Focus lost while held (another window): let go.
  const away = () => {
    const d = down;
    if (!d) return;
    down = null;
    clearTimeout(d.timer);
    if (d.held) wind(d.dir, 'stop');
    else if (winds(d.dir)) windTouch(d.dir, false);
  };
  addEventListener('keydown', press);
  addEventListener('keyup', lift);
  addEventListener('blur', away);
  return () => {
    away();
    removeEventListener('keydown', press);
    removeEventListener('keyup', lift);
    removeEventListener('blur', away);
  };
}
