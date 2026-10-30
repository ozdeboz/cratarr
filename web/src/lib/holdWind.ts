// Holding Previous or Next winds the tape (decision 0074; the owner, 2026-10-27): a press kept past HOLD_MS starts
// `onhold`, letting go (or leaving the button) `onrelease`, and the tap that ends a hold is swallowed; a shorter press
// is the button's own tap. Null: the button is only tapped (winding off). Its label's long-press (lib/labels) gives way.
// From the press until it is let go, the press is the player's only gesture (the owner, 2026-10-28): its own pointer
// goes no further than the button (the player's swipes never see it), and any other finger's press or click is
// swallowed wherever it lands. The browser takes no part in it either: an armed button has no touch action of its own
// (the owner, 2026-10-29: a long hold stopped by itself after a few seconds, the finger's drift of a few pixels starting
// the player's sideways pan, whose pointercancel let the hold go). How each hold ended goes to the playback log.

import { startStopNow } from './device.svelte';
import { playback } from './deviceLog';
import { port } from './player/port.svelte';
import { windsBy } from './startStop';

const HOLD_MS = 350;

/** Whether holding Previous (`back`) or Next (`forward`) winds where the music plays: this device's that hold not off
 *  (the owner, 2026-10-28: each motion on its own), another of Cratarr's players deciding for itself (`player`). */
export function holdsHere(dir: 'back' | 'forward', player: unknown): boolean {
  if (!port().holdsQueue) return !!player;
  return windsBy(startStopNow(), dir === 'back' ? 'holdBack' : 'holdFwd');
}

/** `onpress`: the touch began (a hold's stream asked already, lib/localPlayer windArm); `oncancel`: it was a tap;
 *  `ondrag`: while held, how far the finger is dragged down (0..1), the hold's speed held back by it. */
export type Hold = { onhold: () => void; onrelease: () => void; onpress?: () => void; oncancel?: () => void; ondrag?: (share: number) => void } | null;
/** The finger dragged down while holding (the owner, 2026-10-28: to aim the look-ahead): from DRAG_FROM_PX it holds the
 *  hold back, all the way (to its starting speed) at DRAG_FULL_PX. */
const DRAG_FROM_PX = 12;
const DRAG_FULL_PX = 150;

export function holdWind(node: HTMLElement, hold: Hold) {
  let now = hold;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let held = false;
  /** The press under way: its pointer; null when none; where it went down. */
  let pressing: number | null = null;
  let downY = 0;
  /** When the hold began (its HOLD_MS passed). */
  let heldAt = 0;

  /** Another finger, or the press's own moves, while pressed: none of it reaches anything else. */
  function fence(e: PointerEvent) {
    if (pressing === null) return;
    if (e.pointerId === pressing) {
      if (e.type === 'pointermove') {
        e.stopPropagation();
        if (held) now?.ondrag?.(Math.min(1, Math.max(0, (e.clientY - downY - DRAG_FROM_PX) / (DRAG_FULL_PX - DRAG_FROM_PX))));
      }
      // Let go elsewhere (no pointer capture): the press ends all the same.
      else if (e.type !== 'pointerdown' && !node.contains(e.target as Node)) up(e);
      return;
    }
    e.stopImmediatePropagation();
    e.preventDefault();
  }
  function fenceClick(e: MouseEvent) {
    if (pressing === null || node.contains(e.target as Node)) return;
    e.stopImmediatePropagation();
    e.preventDefault();
  }
  function lock(id: number) {
    pressing = id;
    for (const t of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel'] as const) addEventListener(t, fence, true);
    addEventListener('click', fenceClick, true);
  }
  function unlock() {
    if (pressing === null) return;
    pressing = null;
    for (const t of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel'] as const) removeEventListener(t, fence, true);
    // The click a press's own release makes comes after it: the fence on clicks lifts once that has passed.
    setTimeout(() => pressing === null && removeEventListener('click', fenceClick, true), 0);
  }

  function down(e: PointerEvent) {
    if (!now || e.button > 0 || pressing !== null) return;
    // The player's own gestures (its swipes on the cover and the head) never see this press.
    e.stopPropagation();
    lock(e.pointerId);
    downY = e.clientY;
    held = false;
    node.setPointerCapture?.(e.pointerId);
    const h = now;
    h.onpress?.();
    timer = setTimeout(() => {
      timer = undefined;
      held = true;
      heldAt = performance.now();
      navigator.vibrate?.(8);
      h.onhold();
    }, HOLD_MS);
  }
  function up(e: PointerEvent) {
    if (e.pointerId !== pressing) return;
    e.stopPropagation();
    unlock();
    const waiting = timer !== undefined;
    clearTimeout(timer);
    timer = undefined;
    if (held) {
      // A lift, or the browser or the system taking the touch over (still a let-go: a hold never sticks).
      playback(e.type === 'pointercancel' ? 'warn' : 'info', 'hold let go', { by: e.type, heldMs: Math.round(performance.now() - heldAt) });
      now?.onrelease();
    } else if (waiting) now?.oncancel?.();
  }
  function click(e: MouseEvent) {
    if (!held) return;
    held = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  }
  const menu = (e: Event) => now && e.preventDefault();
  function mark() {
    if (now) node.setAttribute('data-longpress', '');
    else node.removeAttribute('data-longpress');
    // Set before a touch begins (the browser reads it then): an armed button's touch is the hold's alone.
    node.style.touchAction = now ? 'none' : '';
  }

  node.addEventListener('pointerdown', down);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', up);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', menu);
  node.style.setProperty('-webkit-touch-callout', 'none');
  node.style.userSelect = 'none';
  mark();

  return {
    update(next: Hold) {
      now = next;
      mark();
    },
    destroy() {
      if (held) now?.onrelease();
      clearTimeout(timer);
      unlock();
      removeEventListener('click', fenceClick, true);
      node.removeAttribute('data-longpress');
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointercancel', up);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', menu);
    },
  };
}
