// One motion for the glass (the owner, 2026-11-01: the top glass stuttered, "a bottom up guarantee it will always
// animate smoothly"). A glass part is a box that paints nothing itself: a `glass` layer in it (the blur and the colour),
// a `shade` (the ring and the shadow) and its own children. Whatever changes in it (a child comes or goes, words, its
// size), it is laid out once, at once, as it ends; then only opacity, translate and clip-path move (and the shade's
// edges), from the shape it had to the one it has, each move's first frame its element's own style at once (`begin`):
// - the glass: its clip-path from the old outline to the new (the layer drawn as large as both meanwhile);
// - the shade: its edges from the old outline to the new (its own layout only, an empty layer: never stretched, so its
//   round ends stay round);
// - a child that stays: moved from where it was (translate); one that comes: faded in; one that goes: a copy of it
//   where it was, faded out. Each is clipped to the glass's outline as it moves, so nothing shows past the glass.
// Nothing in the page's flow animates a size or a place, so no step of it lays the page out again. A blur inside a clipped element sees only that element (a backdrop
// root), so a child that is a glass of its own (the round back button) is never clipped by an ancestor here: each child
// gets its own clip. A child hidden (display: none) goes as one removed: its copy shows as it was.
import { decelerate, DECELERATE_CSS, reducedMotion } from './motion';
import { between, grownTo, insetOf, rectIn, sameRect, union, type Rect } from './glassShape';

export const GLASS_MS = 200;

type Part = {
  box: HTMLElement;
  glass: HTMLElement;
  shade: HTMLElement | null;
  radius: number;
  /** No motion now: what changes is there at once (where the owner wanted it so). */
  still: () => boolean;
  frame: () => { left: number; top: number };
  /** The box and each child where they are at rest (in the frame), and the glass's opacity. */
  at: Rect | null;
  kids: Map<Element, Rect>;
  /** How each child is displayed, for its copy when it is hidden. */
  shows: Map<Element, string>;
  seen: number;
  /** The move under way: from, to, and the glass's animation, whose own clock says how far it got. */
  run: { from: Rect; to: Rect; clock: Animation } | null;
  anims: Animation[];
  window: string;
};

const GHOST = 'data-glass-ghost';
const isLayer = (p: Part, el: Element) => el === p.glass || el === p.shade || el.hasAttribute(GHOST);
const kidsOf = (p: Part) => [...p.box.children].filter((el) => !isLayer(p, el));
/** The window's width: a change of it (turning the phone, a window resized) is laid out at once, not moved. Not its
 *  height, which the keyboard changes as the search opens (the owner, 2026-11-01: after the first search it no longer
 *  moved). */
const windowNow = () => `${innerWidth}`;

/** The frame a part's places are measured in: the window, moved as the nearest fixed or sticky element (the row it
 *  floats in, or the part itself: the bar) is moved by its transforms, so a page scrolling under it, or the row or
 *  the bar sliding away, changes no place in it; a change of its own layout (the bar along the bottom growing up) does. */
function frameOf(box: HTMLElement): () => { left: number; top: number } {
  let el: HTMLElement | null = box;
  while (el && !['fixed', 'sticky'].includes(getComputedStyle(el).position)) el = el.parentElement;
  const f = el;
  if (!f) return () => ({ left: 0, top: 0 });
  // Its offsets are its layout's, without the transforms.
  return () => {
    const r = f.getBoundingClientRect();
    return { left: r.left - f.offsetLeft, top: r.top - f.offsetTop };
  };
}

function measure(p: Part) {
  const f = p.frame();
  const at = rectIn(p.box.getBoundingClientRect(), f);
  const kids = new Map<Element, Rect>();
  const shows = new Map<Element, string>();
  for (const el of kidsOf(p)) {
    const display = getComputedStyle(el).display;
    if (display === 'none') continue;
    kids.set(el, rectIn(el.getBoundingClientRect(), f));
    shows.set(el, display);
  }
  return { at, kids, shows, glass: Number(getComputedStyle(p.glass).opacity) };
}

/** The outline shown now: where the move under way has got to, else where it rests. By the glass's own animation, as
 *  each child's translate is read from its own (offsetNow), never by the clock on the wall: a move whose first frame a
 *  busy page held back (a page change after the tap that folded the search) has not moved yet, and the next move
 *  takes over from where both are seen (the owner, 2026-11-02: back with the search open, the lens and its glass came
 *  apart). */
function shownNow(p: Part): Rect | null {
  if (!p.run || !p.at) return p.at;
  const ms = Number(p.run.clock.currentTime ?? 0);
  return between(p.run.from, p.run.to, decelerate(Math.min(1, ms / GLASS_MS)));
}

/** A child's offset from its rest now (its translate under way). */
function offsetNow(el: Element): [number, number] {
  const t = getComputedStyle(el).translate;
  if (!t || t === 'none') return [0, 0];
  const [x, y] = t.split(' ').map((v) => parseFloat(v) || 0);
  return [x, y ?? 0];
}

/** Each element's move under way, so a move that ends does not clear the start of the next; and each move's own
 *  clearing, done at once when a move is taken over (before the next measures, which its start style would shift). */
const moving = new WeakMap<Element, Animation>();
const clearing = new WeakMap<Animation, () => void>();
const stop = (a: Animation) => {
  a.cancel();
  clearing.get(a)?.();
};
const kebab = (k: string) => k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
/** A move from its first frame, that frame also set as the element's own style at once (the owner, 2026-11-02, on the
 *  tablet: a busy page drew the frame the layout changed before the move's first frame, so the glass showed as large
 *  as both outlines, a rectangle, and what it holds already at its new place, outside it). Taken off as the move ends. */
function begin(el: HTMLElement, frames: Keyframe[], timing: KeyframeAnimationOptions): Animation {
  const first = frames[0];
  const props = Object.keys(first).filter((k) => k !== 'offset' && k !== 'easing' && k !== 'composite');
  for (const k of props) el.style.setProperty(kebab(k), String(first[k as keyof Keyframe]));
  const a = el.animate(frames, timing);
  moving.set(el, a);
  const clear = () => {
    if (moving.get(el) !== a) return;
    moving.delete(el);
    for (const k of props) el.style.removeProperty(kebab(k));
  };
  clearing.set(a, clear);
  a.finished.then(clear, clear);
  return a;
}

function play(p: Part) {
  const from = shownNow(p);
  // Where each child that was there is seen now: its rest, moved by what it is still moving.
  const was = new Map<Element, Rect>();
  for (const [el, r] of p.kids) {
    // Read only while a move is under way: at rest there is none, and the read costs the tap's task a style pass.
    const [dx, dy] = p.run && el.isConnected ? offsetNow(el) : [0, 0];
    was.set(el, { ...r, x: r.x + dx, y: r.y + dy });
  }
  const glassWas = p.seen;
  const showed = p.shows;
  for (const a of p.anims) stop(a);
  p.anims = [];
  p.glass.style.inset = '';
  const after = measure(p);
  const win = windowNow();
  const resized = win !== p.window;
  p.window = win;
  const settle = () => {
    p.at = after.at;
    p.kids = after.kids;
    p.shows = after.shows;
    p.seen = after.glass;
    p.run = null;
  };
  const kept = [...after.kids.keys()].filter((el) => was.has(el));
  const changed =
    !from ||
    !sameRect(from, after.at) ||
    glassWas !== after.glass ||
    after.kids.size !== kept.length ||
    p.kids.size !== kept.length ||
    kept.some((el) => !sameRect(was.get(el)!, after.kids.get(el)!));
  // All of it hidden, or all of it back (app.css's `.away` hides what slides away, the bar's tabs among it): no move.
  const whole = after.kids.size > 0 && p.kids.size > 0;
  if (!from || !changed || !whole || resized || reducedMotion() || p.still()) return settle();

  const to = after.at;
  const timing: KeyframeAnimationOptions = { duration: GLASS_MS, easing: DECELERATE_CSS };
  const all = union(from, to);
  const r = p.radius;
  // The glass: drawn as large as both, its outline from the old to the new. Shown whole while it moves, taking its
  // own strength at the end: a title's glass shrinks onto the round button it gives way to and goes behind it, or
  // grows out of it (the owner, 2026-11-01: the navigation glass faded instead of gliding to collapse).
  const seen = Math.max(glassWas, after.glass);
  p.glass.style.inset = grownTo(all, to);
  const clock = begin(
    p.glass,
    [
      { clipPath: insetOf(from, all, r), opacity: seen },
      { clipPath: insetOf(to, all, r), opacity: seen },
    ],
    timing,
  );
  p.anims.push(clock);
  // Its ring and shadow: its edges moved from the old outline to the new, never stretched (the owner, 2026-11-02: a
  // scale squashed its round ends into an oval, and its ring showed as a second, misplaced outline). An empty layer
  // laid out alone, so a cheap move; and on the same clock as the glass's outline, never ahead of it.
  if (p.shade)
    p.anims.push(
      begin(
        p.shade,
        [
          { inset: grownTo(from, to), opacity: seen },
          { inset: '0px', opacity: seen },
        ],
        timing,
      ),
    );
  // A child that comes or goes where its neighbour that stays passes over it (the one before it, else after it) moves
  // with that neighbour: the field comes in with the magnifier it opens from, and goes with it, rather than lying still
  // as the magnifier slides over it (the owner, 2026-11-02: the lens came apart from its glass). One the neighbour does
  // not pass (a button at the pill's still end) fades where it is.
  const order = [...was.keys()];
  const moveOf = (box: Rect, near: Element[]): [number, number] => {
    const k = near.find((n) => was.has(n) && after.kids.has(n));
    if (!k) return [0, 0];
    const [a, b] = [was.get(k)!, after.kids.get(k)!];
    const swept = union(a, b);
    if (box.x >= swept.x + swept.w || box.x + box.w <= swept.x) return [0, 0];
    return [b.x - a.x, b.y - a.y];
  };
  const shifted = (k: Rect, [dx, dy]: [number, number]): Rect => ({ ...k, x: k.x + dx, y: k.y + dy });
  const kidsNow = [...after.kids.keys()];
  for (const [el, k1] of after.kids) {
    const k0 = was.get(el);
    let frames: Keyframe[];
    if (k0) {
      frames = [
        { translate: `${k0.x - k1.x}px ${k0.y - k1.y}px`, clipPath: insetOf(from, k0, r) },
        { translate: '0px 0px', clipPath: insetOf(to, k1, r) },
      ];
    } else {
      const i = kidsNow.indexOf(el);
      const [dx, dy] = moveOf(k1, [...kidsNow.slice(0, i).reverse(), ...kidsNow.slice(i + 1)]);
      const start = shifted(k1, [-dx, -dy]);
      frames = [
        { opacity: 0, translate: `${-dx}px ${-dy}px`, clipPath: insetOf(from, start, r) },
        { opacity: 1, translate: '0px 0px', clipPath: insetOf(to, k1, r) },
      ];
    }
    p.anims.push(begin(el as HTMLElement, frames, timing));
  }
  // What went: a copy where it was, fading as the glass leaves it, moving with its neighbour.
  for (const [el, k0] of was) {
    if (after.kids.has(el)) continue;
    const i = order.indexOf(el);
    const move = moveOf(k0, [...order.slice(0, i).reverse(), ...order.slice(i + 1)]);
    const ghost = el.cloneNode(true) as HTMLElement;
    ghost.setAttribute(GHOST, '');
    ghost.setAttribute('aria-hidden', 'true');
    ghost.inert = true;
    Object.assign(ghost.style, {
      position: 'absolute',
      left: `${k0.x - to.x}px`,
      top: `${k0.y - to.y}px`,
      width: `${k0.w}px`,
      height: `${k0.h}px`,
      margin: '0',
      display: showed.get(el) ?? '',
      translate: 'none',
      pointerEvents: 'none',
    });
    p.box.append(ghost);
    const a = begin(
      ghost,
      [
        { opacity: 1, translate: '0px 0px', clipPath: insetOf(from, k0, r) },
        { opacity: 0, translate: `${move[0]}px ${move[1]}px`, clipPath: insetOf(to, shifted(k0, move), r) },
      ],
      { ...timing, fill: 'forwards' },
    );
    const gone = () => ghost.remove();
    a.finished.then(gone, gone);
  }
  p.at = to;
  p.kids = after.kids;
  p.shows = after.shows;
  p.seen = after.glass;
  p.run = { from, to, clock };
  const anims = p.anims;
  Promise.all(anims.map((a) => a.finished)).then(
    () => {
      if (p.anims !== anims) return;
      p.glass.style.inset = '';
      p.run = null;
    },
    () => {},
  );
}

/** Makes `box` a glass part (an attachment: `{@attach glassPart({ radius: 26 })}`): its first children of class `glass`
 *  and `shade` are its layers. Its motion is its own from then on, whatever changes it. */
export function glassPart({ radius, still = () => false }: { radius: number; still?: () => boolean }) {
  return (box: HTMLElement) => {
    const glass = box.querySelector<HTMLElement>(':scope > .glass');
    if (!glass) throw new Error('glassPart: no .glass layer');
    const p: Part = {
      box,
      glass,
      shade: box.querySelector<HTMLElement>(':scope > .shade'),
      radius,
      still,
      frame: frameOf(box),
      at: null,
      kids: new Map(),
      shows: new Map(),
      seen: 1,
      run: null,
      anims: [],
      window: windowNow(),
    };
    const first = measure(p);
    p.at = first.at;
    p.kids = first.kids;
    p.shows = first.shows;
    p.seen = first.glass;
    // A change to its children or words: played before the frame is drawn (animation frames come before layout).
    let asked = 0;
    const ask = () => {
      if (!asked) asked = requestAnimationFrame(() => ((asked = 0), play(p)));
    };
    const ours = (n: Node) => n instanceof Element && (n.hasAttribute(GHOST) || n === p.glass || n === p.shade);
    const changes = new MutationObserver((list) => {
      if (list.some((m) => !(ours(m.target) || (m.type === 'childList' && [...m.addedNodes, ...m.removedNodes].every(ours))))) ask();
    });
    changes.observe(box, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['class'] });
    // Its size changed otherwise (the room beside it, a font): played at once, still before that frame is drawn.
    const sized = new ResizeObserver(() => {
      if (asked) return;
      const now = measure(p);
      if (p.at && !p.run && !sameRect(p.at, now.at)) play(p);
    });
    sized.observe(box);
    return () => {
      changes.disconnect();
      sized.disconnect();
      cancelAnimationFrame(asked);
      for (const a of p.anims) stop(a);
    };
  };
}
