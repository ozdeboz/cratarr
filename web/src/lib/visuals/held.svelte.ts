// A setting's label held down shows what the setting does (help.ts), in a popover by it (Help.svelte): held for
// `HOLD_MS` on a touch screen or with the mouse, right-clicked, or `?` pressed on its control. A tap anywhere else,
// Escape or a scroll puts it away. The owner: "each setting should be explained by you in a popover on long press on
// label". Nothing shows otherwise: the panel stays labels and values.

import type { Action } from 'svelte/action';
import { HELP } from './help';

/** How long a press is held before the help shows (ms), and how far it may wander (px) and still be held. */
const HOLD_MS = 450;
const SLOP = 8;

/** The help showing now: its text, and where its label is (the viewport's pixels). */
export const helpShown = $state({ open: false, text: '', left: 0, top: 0, bottom: 0 });

let owner: HTMLElement | null = null;

export function hideHelp() {
  helpShown.open = false;
  owner = null;
}

/** `use:held={key}` on a label: holding it shows `HELP[key]`. */
export const held: Action<HTMLElement, string> = (node, initial) => {
  let key = initial;
  let timer = 0;
  let from: { x: number; y: number } | null = null;
  // A hold that showed the help: the click that ends it does nothing else (a label would focus its control).
  let shown = false;
  // A held label is not text to select, nor a link to call a menu up for.
  node.style.userSelect = 'none';
  node.style.setProperty('-webkit-user-select', 'none');
  node.style.setProperty('-webkit-touch-callout', 'none');

  const show = () => {
    const text = HELP[key];
    if (!text) return;
    const r = node.getBoundingClientRect();
    owner = node;
    Object.assign(helpShown, { open: true, text, left: r.left, top: r.top, bottom: r.bottom });
  };
  const cancel = () => {
    clearTimeout(timer);
    timer = 0;
    from = null;
  };
  const down = (e: PointerEvent) => {
    if (e.button !== 0) return;
    cancel();
    shown = false;
    from = { x: e.clientX, y: e.clientY };
    timer = window.setTimeout(() => {
      timer = 0;
      shown = true;
      show();
    }, HOLD_MS);
  };
  const move = (e: PointerEvent) => {
    if (from && Math.hypot(e.clientX - from.x, e.clientY - from.y) > SLOP) cancel();
  };
  const click = (e: MouseEvent) => {
    if (!shown) return;
    shown = false;
    e.preventDefault();
    e.stopPropagation();
  };
  // A right click, and on Android the long press's own menu: the help instead.
  const menu = (e: MouseEvent) => {
    e.preventDefault();
    cancel();
    show();
  };
  // `?` on the label's control (its slider, focused from the keyboard).
  const control: HTMLElement = node.closest('label') ?? node;
  const ask = (e: KeyboardEvent) => {
    if (e.key !== '?') return;
    e.preventDefault();
    show();
  };

  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', cancel);
  node.addEventListener('pointercancel', cancel);
  node.addEventListener('pointerleave', cancel);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', menu);
  control.addEventListener('keydown', ask);
  return {
    update(next: string) {
      key = next;
    },
    destroy() {
      cancel();
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', cancel);
      node.removeEventListener('pointercancel', cancel);
      node.removeEventListener('pointerleave', cancel);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', menu);
      control.removeEventListener('keydown', ask);
      if (owner === node) hideHelp();
    },
  };
};
