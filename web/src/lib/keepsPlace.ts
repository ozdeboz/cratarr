// A panel list's place (F25, SB15: every panel keeps its place on back, by one rule): attached to the list's `.scroll`, it keeps
// where the list is scrolled with its step of the panel's history (lib/panel), and brings it back on a step reached by back.

import { panel, panelDepth, placeHere } from './panel.svelte';
import { placeBack } from './placeBack';

/** How long after a scroll its place is written (App's page writes its own so). */
const KEEP_MS = 150;

/** The attachment: where the list is scrolled kept 150 ms after a scroll and once more as its step is left (so the last scroll
 *  counts), with the row open in it where it has one (`open`); on a step reached by back, once its content shows (lib/Hold's
 *  `shown`), that place brought back (`placeBack`) and its row opened again (`reopen`). A forward step keeps its own rules (its
 *  top, a viewed-again's top, a marked track). */
export function keepsPlace(o: { open?: () => string | null; reopen?: (key: string | null) => void } = {}) {
  return (el: HTMLElement) => {
    // Each step its own: this runs again as the panel shows another, and once what it shows has come.
    void panelDepth();
    if (panel.viewed === null || panel.shown !== panel.viewed) return;
    const here = placeHere();
    const write = () => here.keep({ scroll: Math.round(el.scrollTop), ...(o.open ? { open: o.open() } : {}) });
    let t: ReturnType<typeof setTimeout> | undefined;
    const scrolled = () => {
      clearTimeout(t);
      t = setTimeout(write, KEEP_MS);
    };
    el.addEventListener('scroll', scrolled, { passive: true });
    const unleave = here.leaving(write);
    let cancel = () => {};
    const p = here.place;
    if (p) {
      if (o.reopen && p.open !== undefined) o.reopen(p.open);
      cancel = placeBack({ scrollTo: (y) => el.scrollTo(0, y), at: () => el.scrollTop, long: () => el.scrollHeight, target: el }, p.scroll);
    }
    return () => {
      clearTimeout(t);
      cancel();
      unleave();
      el.removeEventListener('scroll', scrolled);
    };
  };
}
