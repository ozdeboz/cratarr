// Holding Previous or Next winds the tape (decision 0074; the owner, 2026-10-27): a press kept past HOLD_MS starts
// `onhold`, letting go (or leaving the button) `onrelease`, and the tap that ends a hold is swallowed; a shorter press
// is the button's own tap. Null: the button is only tapped (winding off). Its label's long-press (lib/labels) gives way.
// From the press until it is let go, the press is the player's only gesture (the owner, 2026-10-28): its own pointer
// goes no further than the button (the player's swipes never see it), and any other finger's press or click is
// swallowed wherever it lands.

import { appInfo, inApp } from './app.svelte';
import { isLocal } from './output.svelte';
import { prefs } from './prefs.svelte';
import { windsBy } from './startStop';

const HOLD_MS = 350;

/** Whether holding Previous (`back`) or Next (`forward`) winds where the music plays: this device's that hold not off
 *  (the owner, 2026-10-28: each motion on its own), another of Cratarr's players deciding for itself (`player`). */
export function holdsHere(dir: 'back' | 'forward', player: unknown): boolean {
  if (!isLocal()) return !!player;
  return windsBy(inApp ? appInfo.startStop : prefs.startStop, dir === 'back' ? 'holdBack' : 'holdFwd');
}

/** `onpress`: the touch began (a hold's stream asked already, lib/localPlayer windArm); `oncancel`: it was a tap. */
export type Hold = { onhold: () => void; onrelease: () => void; onpress?: () => void; oncancel?: () => void } | null;

export function holdWind(node: HTMLElement, hold: Hold) {
  let now = hold;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let held = false;
  /** The press under way: its pointer; null when none. */
  let pressing: number | null = null;

  /** Another finger, or the press's own moves, while pressed: none of it reaches anything else. */
  function fence(e: PointerEvent) {
    if (pressing === null) return;
    if (e.pointerId === pressing) {
      if (e.type === 'pointermove') e.stopPropagation();
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
    held = false;
    node.setPointerCapture?.(e.pointerId);
    const h = now;
    h.onpress?.();
    timer = setTimeout(() => {
      timer = undefined;
      held = true;
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
    if (held) now?.onrelease();
    else if (waiting) now?.oncancel?.();
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
