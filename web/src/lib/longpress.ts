// Long-press (touch or pen, ~500 ms) and right-click open an item's menu (docs/ui-conventions.md). A long-press
// never also counts as a tap, moving or scrolling cancels it, and the browser's own menu stays closed. A press on an
// icon button inside the item is the button's (its label, lib/labels).
import { iconButton } from './labels.svelte';

const HOLD_MS = 500;
const SLOP_PX = 10;

export function longpress(target: HTMLElement | SVGElement, onlong: (rect: DOMRect) => void) {
  // An SVG element (a point in Insights' graph) takes the same events and inline style; typed as one for the listeners.
  const node = target as HTMLElement;
  let handler = onlong;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let start: { x: number; y: number } | null = null;
  let fired = false;
  const inner = (t: EventTarget | null) => {
    const b = iconButton(t);
    return b !== null && b !== node;
  };

  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
    start = null;
  };
  const fire = () => {
    fired = true;
    cancel();
    handler(node.getBoundingClientRect());
  };

  function down(e: PointerEvent) {
    fired = false;
    if (e.pointerType === 'mouse' || inner(e.target)) return; // a mouse has right-click
    start = { x: e.clientX, y: e.clientY };
    timer = setTimeout(fire, HOLD_MS);
  }
  function move(e: PointerEvent) {
    if (start && Math.hypot(e.clientX - start.x, e.clientY - start.y) > SLOP_PX) cancel();
  }
  // The tap that ends a long-press is swallowed.
  function click(e: MouseEvent) {
    if (!fired) return;
    fired = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  }
  // Right-click, and touch browsers' own long-press signal: open the menu unless this press already did.
  function contextmenu(e: MouseEvent) {
    if (inner(e.target)) return;
    e.preventDefault();
    if (!fired) fire();
  }

  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', cancel);
  node.addEventListener('pointercancel', cancel);
  node.addEventListener('pointerleave', cancel);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', contextmenu);
  addEventListener('scroll', cancel, true);
  // An icon button with its own long-press keeps it (lib/labels leaves it be).
  node.setAttribute('data-longpress', '');
  node.style.setProperty('-webkit-touch-callout', 'none');
  node.style.userSelect = 'none';

  return {
    update(next: (rect: DOMRect) => void) {
      handler = next;
    },
    destroy() {
      cancel();
      node.removeAttribute('data-longpress');
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', cancel);
      node.removeEventListener('pointercancel', cancel);
      node.removeEventListener('pointerleave', cancel);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', contextmenu);
      removeEventListener('scroll', cancel, true);
    },
  };
}
