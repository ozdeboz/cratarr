// A row of glass (App's row over the page, a panel's own bar) laid out in one place (lib/rowLayout; the owner,
// 2026-11-02: the parts each measured the others after the fact, and text ended up outside its glass). The row
// (`rowRoot`) measures only itself; a part hands in its pieces (`rowPiece`: a breadcrumb's level, a title, the open
// field, a button a glass took), each measured whole on its own. Everything else in the row (back's glass, the pill's
// buttons, paddings and gaps) is room it does not have. One decision for every piece, applied at once: a piece that
// goes is hidden (`data-row-gone`), one cut or grown gets its width.
import type { Attachment } from 'svelte/attachments';
import { layoutRow, type Piece } from './rowLayout';

/** `takes`: the open search field, which takes the row; `yields`: the left glass's extension (a page's title or picker,
 *  a panel's levels, its actions), which goes whole while the field is open (the owner, 2026-11-03: as it used to). */
type Spec = { id: string; rank: number; min?: number; max?: number; width?: number; takes?: boolean; yields?: boolean };
type Entry = { el: HTMLElement; spec: Spec };

const GONE = 'data-row-gone';
const rows = new WeakMap<Element, Row>();

/** In the flow of its box: not hidden by the row, not laid over it (a glass's layers, a leaving copy). */
function inFlow(c: Element): c is HTMLElement {
  if (!(c instanceof HTMLElement) || c.matches('[data-glass-ghost], .glass, .shade')) return false;
  const s = getComputedStyle(c);
  return s.display !== 'none' && s.position !== 'absolute' && s.position !== 'fixed';
}
const px = (v: string) => parseFloat(v) || 0;
const marginsOf = (el: Element) => {
  const s = getComputedStyle(el);
  return px(s.marginLeft) + px(s.marginRight);
};

class Row {
  readonly entries = new Map<string, Entry>();
  private before: Set<string> | null = null;
  private raf = 0;
  private readonly seen: ResizeObserver;
  private readonly changed: MutationObserver;

  constructor(readonly el: HTMLElement) {
    this.seen = new ResizeObserver(() => this.soon());
    this.seen.observe(el);
    // What is in it coming and going (a button, a part handed in): the room left changes.
    this.changed = new MutationObserver(() => this.soon());
    this.changed.observe(el, { childList: true, subtree: true, characterData: true });
  }

  soon() {
    this.raf ||= requestAnimationFrame(() => {
      this.raf = 0;
      this.decide();
    });
  }

  add(e: Entry) {
    this.entries.set(e.spec.id, e);
    this.seen.observe(e.el);
    this.soon();
  }

  remove(id: string, el: HTMLElement) {
    if (this.entries.get(id)?.el !== el) return;
    this.entries.delete(id);
    this.seen.unobserve(el);
    this.soon();
  }

  /** A piece laid out whole, as if nothing were cut or gone (in the same frame: nothing is painted meanwhile). */
  private whole(e: Entry): number {
    if (e.spec.width !== undefined) return e.spec.width;
    const el = e.el;
    const was = { gone: el.hasAttribute(GONE), width: el.style.width, flex: el.style.flex, max: el.style.maxWidth };
    el.removeAttribute(GONE);
    // Its own width: none the row set, not shrunk (a button keeps its stylesheet's 44 px; words their whole length).
    Object.assign(el.style, { width: '', flex: 'none', maxWidth: 'none' });
    const w = el.getBoundingClientRect().width + marginsOf(el);
    if (was.gone) el.setAttribute(GONE, '');
    Object.assign(el.style, { width: was.width, flex: was.flex, maxWidth: was.max });
    return w;
  }

  /** What `box` takes that is no piece: its padding and borders, its gaps, and whatever in it is not a piece. */
  private fixed(box: HTMLElement, pieces: Set<Element>): number {
    const s = getComputedStyle(box);
    const kids = [...box.children].filter(inFlow);
    let n = px(s.paddingLeft) + px(s.paddingRight) + px(s.borderLeftWidth) + px(s.borderRightWidth) + Math.max(0, kids.length - 1) * px(s.columnGap);
    for (const c of kids) {
      n += marginsOf(c);
      if (pieces.has(c)) continue;
      n += [...pieces].some((p) => c.contains(p)) ? this.fixed(c, pieces) : c.getBoundingClientRect().width;
    }
    return n;
  }

  decide() {
    if (!this.el.isConnected) return;
    const entries = [...this.entries.values()].filter((e) => e.el.isConnected && this.el.contains(e.el));
    // The search open: the left glass's extension gone whole, the rest laid out without it.
    const searching = entries.some((e) => e.spec.takes);
    const laid = entries.filter((e) => !(searching && e.spec.yields));
    const pieces: Piece[] = laid.map((e) => ({ id: e.spec.id, rank: e.spec.rank, min: e.spec.min, max: e.spec.max, width: this.whole(e) }));
    const room = this.el.clientWidth - this.fixed(this.el, new Set(entries.map((e) => e.el)));
    const d = layoutRow(room, pieces, this.before);
    this.before = d.shown;
    for (const e of entries) {
      const on = d.shown.has(e.spec.id);
      if (on === e.el.hasAttribute(GONE)) {
        if (on) e.el.removeAttribute(GONE);
        else e.el.setAttribute(GONE, '');
      }
      const w = d.width.get(e.spec.id);
      const width = w === undefined ? '' : `${w - marginsOf(e.el)}px`;
      if (e.el.style.width !== width) {
        e.el.style.width = width;
        e.el.style.flex = w === undefined ? '' : 'none';
      }
    }
  }

  destroy() {
    cancelAnimationFrame(this.raf);
    this.seen.disconnect();
    this.changed.disconnect();
  }
}

/** Pieces attached before their row was (a title mounted with the row on a fresh load: its attachment runs first),
 *  taken in by the row once it is. */
const waiting = new Set<Entry>();

/** A row laid out in one place: App's row over the page, a panel's own bar. */
export function rowRoot(): Attachment<HTMLElement> {
  return (el) => {
    const row = new Row(el);
    rows.set(el, row);
    el.setAttribute('data-row', '');
    for (const e of waiting) {
      if (!el.contains(e.el) || e.el.parentElement?.closest('[data-row]') !== el) continue;
      waiting.delete(e);
      row.add(e);
    }
    return () => {
      row.destroy();
      rows.delete(el);
      el.removeAttribute('data-row');
    };
  };
}

let uid = 0;
/** A piece of the row it is in (`spec`: its rank, how far it may be cut or grown; its width, where it is not its
 *  laid-out one): shown, cut, grown or gone as the row decides. Outside a row: always shown, whole. */
export function rowPiece(spec: () => Omit<Spec, 'id'> & { id?: string }): Attachment<HTMLElement> {
  const own = `piece${++uid}`;
  return (el) => {
    const host = el.parentElement?.closest('[data-row]');
    const row = host ? rows.get(host) : undefined;
    const s = spec();
    const id = s.id ?? own;
    const entry: Entry = { el, spec: { ...s, id } };
    // No row yet: one may still be made around it (rowRoot takes it in); outside any, always shown whole.
    if (row) row.add(entry);
    else waiting.add(entry);
    return () => {
      waiting.delete(entry);
      const host = el.parentElement?.closest('[data-row]');
      (host ? rows.get(host) : row)?.remove(id, el);
      el.removeAttribute(GONE);
      el.style.width = '';
      el.style.flex = '';
    };
  };
}
