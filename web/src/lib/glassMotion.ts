// One motion for the glass (the owner, 2026-11-01: the top glass stuttered, "a bottom up guarantee it will always
// animate smoothly"). A glass part is a box that paints nothing itself: a `glass` layer in it (the blur and the colour),
// a `shade` (the ring and the shadow) and its own children. Whatever changes in it (a child comes or goes, words, its
// size), it is laid out once, at once, as it ends; then only opacity, translate and clip-path move (and the shade's
// edges), from the shape it had to the one it has, each move's first frame its element's own style at once (`begin`):
// - the glass: its clip-path from the old outline to the new (the layer drawn as large as both meanwhile);
// - the shade: its edges from the old outline to the new (its own layout only, an empty layer: never stretched, so its
//   round ends stay round);
// - a child that stays: moved from where it was (translate); one that comes: faded in where it rests, once the glass
//   has reached it (the move's second half); one that goes: gone at once, never a copy of it sliding or fading (the
//   owner, 2026-11-04, one rule for every glass: the search's field went as a copy sliding under the magnifier, its ✕
//   past the glass's end, its words turned to its placeholder; the top-left's title leaves at once). So nothing ever
//   slides across a button. Each child is clipped to the glass's outline as it moves, so nothing shows past the glass
//   (the owner, 2026-11-02: "unconfined content in animations! Structural fix is required app wide"): its clip is part
//   of its own move, on the same clock and curve as the glass's outline, so the two can never drift apart (the owner,
//   2026-11-04: "some drift somewhere"; a clip worked out each frame in script trailed the glass by up to 28 px, and
//   the moving magnifier showed as a sliver). A change during a move (a child coming, a badge) moves again from where
//   everything is seen then.
// Nothing in the page's flow animates a size or a place, so no step of it lays the page out again. A blur inside a clipped element sees only that element (a backdrop
// root), so a child that is a glass of its own (the round back button) is never clipped by an ancestor here: each child
// gets its own clip. A child hidden (display: none) goes as one removed: at once.
import { decelerate, DECELERATE_CSS, reducedMotion } from './motion';
import { between, grownTo, insetOf, rectIn, sameRect, union, type Rect } from './glassShape';

export const GLASS_MS = 200;

type Part = {
  box: HTMLElement;
  glass: HTMLElement;
  shade: HTMLElement | null;
  /** The main glass over the back layer, where a part has one (the pill: the thin back layer is its extension, the
   *  owner 2026-11-03), and how far in from the box's edges it rests (top, right, bottom, left). It moves as the glass
   *  does, its outline on the same clock (the owner, 2026-11-03: the two came apart, the main one at its end at once). */
  main: HTMLElement | null;
  mainIn: [number, number, number, number] | null;
  radius: number;
  /** No motion now: what changes is there at once (where the owner wanted it so). */
  still: () => boolean;
  frame: () => { left: number; top: number };
  /** The box and each child where they are at rest (in the frame), and the glass's opacity. */
  at: Rect | null;
  kids: Map<Element, Rect>;
  seen: number;
  /** The move under way: from, to, and the glass's animation, whose own clock says how far it got. */
  run: { from: Rect; to: Rect; clock: Animation } | null;
  anims: Animation[];
  window: string;
};

/** The children fading in now (their arrival under way): a move that takes over goes on from how far they got. */
const arriving = new WeakSet<Element>();

const isLayer = (p: Part, el: Element) => el === p.glass || el === p.shade || el === p.main;
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
  for (const el of kidsOf(p)) {
    if (getComputedStyle(el).display === 'none') continue;
    kids.set(el, rectIn(el.getBoundingClientRect(), f));
  }
  let mainIn: [number, number, number, number] | null = null;
  if (p.main) {
    const m = rectIn(p.main.getBoundingClientRect(), f);
    mainIn = [m.y - at.y, at.x + at.w - (m.x + m.w), at.y + at.h - (m.y + m.h), m.x - at.x];
  }
  return { at, kids, glass: Number(getComputedStyle(p.glass).opacity), mainIn };
}

/** A box's outline less the main glass's insets. */
const within = (r: Rect, [t, rt, b, l]: [number, number, number, number]): Rect => ({ x: r.x + l, y: r.y + t, w: r.w - l - rt, h: r.h - t - b });

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
  /** How far each one coming had faded in, where a move is taken over. */
  const shown = new Map<Element, number>();
  for (const [el, r] of p.kids) {
    // Read only while a move is under way: at rest there is none, and the read costs the tap's task a style pass.
    const [dx, dy] = p.run && el.isConnected ? offsetNow(el) : [0, 0];
    was.set(el, { ...r, x: r.x + dx, y: r.y + dy });
    if (p.run && el.isConnected && arriving.has(el)) shown.set(el, Number(getComputedStyle(el).opacity));
  }
  const glassWas = p.seen;
  for (const a of p.anims) stop(a);
  p.anims = [];
  p.glass.style.inset = '';
  if (p.main) p.main.style.inset = '';
  const mainWas = p.mainIn;
  const after = measure(p);
  const win = windowNow();
  const resized = win !== p.window;
  p.window = win;
  const settle = () => {
    p.at = after.at;
    p.kids = after.kids;
    p.seen = after.glass;
    p.mainIn = after.mainIn;
    p.run = null;
  };
  const kept = [...after.kids.keys()].filter((el) => was.has(el));
  const changed =
    !from ||
    !sameRect(from, after.at) ||
    glassWas !== after.glass ||
    String(mainWas) !== String(after.mainIn) ||
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
  // The main glass over it: its own outline (the box's less its insets, before and after) from the old to the new, on
  // the glass's clock, drawn as large as both meanwhile.
  if (p.main && mainWas && after.mainIn) {
    const [m0, m1] = [within(from, mainWas), within(to, after.mainIn)];
    const both = union(m0, m1);
    p.main.style.inset = grownTo(both, to);
    p.anims.push(
      begin(
        p.main,
        [
          { clipPath: insetOf(m0, both, r), opacity: seen },
          { clipPath: insetOf(m1, both, r), opacity: seen },
        ],
        timing,
      ),
    );
  }
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
  // Each child: one move on the glass's clock and curve, its clip the glass's outline seen from where it is, from the
  // old outline to the new (both move along the same curve, so the clip between them is the outline all the way). One
  // that stays moves from where it was seen (the magnifier gliding as the field opens and shuts). One that comes fades
  // in where it rests: once the glass is halfway there, or, where one that stays will pass over its place, only as that
  // one arrives (the owner, 2026-11-04: "the glass glides open first and the field fades in", nothing sliding across
  // another); one that was fading in goes on from how far it got. One that went is gone already, at once (the owner,
  // 2026-11-04: as the top-left's title leaves).
  const sweeps = kept.map((el) => union(was.get(el)!, after.kids.get(el)!));
  const passed = (k: Rect) => sweeps.some((s) => k.x < s.x + s.w && k.x + k.w > s.x && k.y < s.y + s.h && k.y + k.h > s.y);
  // The extension is the thin glass beyond the foreground glass (the main layer; where a part has none, its own round
  // buttons, a glass of their own: back, a panel's ✕). What the extension holds is clipped to the extension itself, cut
  // where the foreground begins, at each end of the move and so all the way between: it is never drawn in front of the
  // foreground (the owner, 2026-11-04: "the content of the extension should always be contained by the extension
  // itself").
  const own = (kids: Map<Element, Rect>) => [...kids].find(([el]) => el.matches('.navpill'))?.[1] ?? null;
  const fg0 = p.main && mainWas ? within(from, mainWas) : own(was);
  const fg1 = p.main && after.mainIn ? within(to, after.mainIn) : own(after.kids);
  /** The outline, less the foreground on `side` of it. */
  const beyond = (out: Rect, fg: Rect, side: 'left' | 'right'): Rect =>
    side === 'left' ? { ...out, w: Math.max(0, fg.x - out.x) } : { ...out, x: fg.x + fg.w, w: Math.max(0, out.x + out.w - (fg.x + fg.w)) };
  for (const [el, k1] of after.kids) {
    const k0 = was.get(el) ?? k1;
    const fading = was.has(el) ? shown.get(el) : 0;
    const side = fg0 && fg1 && !el.matches('.navpill') ? (k1.x + k1.w <= fg1.x + 0.5 ? 'left' : k1.x >= fg1.x + fg1.w - 0.5 ? 'right' : null) : null;
    const [out0, out1] = side && fg0 && fg1 ? [beyond(from, fg0, side), beyond(to, fg1, side)] : [from, to];
    const frames: Keyframe[] = [
      { translate: `${k0.x - k1.x}px ${k0.y - k1.y}px`, clipPath: insetOf(out0, k0, r) },
      { translate: '0px 0px', clipPath: insetOf(out1, k1, r) },
    ];
    if (fading !== undefined && fading < 1) {
      frames[0].opacity = fading;
      frames[1].opacity = 1;
      // One that comes: unseen until the glass is halfway there, or until what passes over it has nearly arrived.
      if (!was.has(el)) frames.splice(1, 0, { opacity: 0, offset: passed(k1) ? 0.97 : 0.5 });
    }
    p.anims.push(begin(el as HTMLElement, frames, timing));
    if (fading !== undefined && fading < 1) {
      arriving.add(el);
      const done = () => arriving.delete(el);
      p.anims[p.anims.length - 1].finished.then(done, done);
    }
  }
  p.at = to;
  p.kids = after.kids;
  p.seen = after.glass;
  p.mainIn = after.mainIn;
  p.run = { from, to, clock };
  const anims = p.anims;
  Promise.all(anims.map((a) => a.finished)).then(
    () => {
      if (p.anims !== anims) return;
      p.glass.style.inset = '';
      if (p.main) p.main.style.inset = '';
      p.run = null;
    },
    () => {},
  );
}


/** Makes `box` a glass part (an attachment: `{@attach glassPart({ radius: 26 })}`): its first children of class `glass`,
 *  `shade` and `main` are its layers. Its motion is its own from then on, whatever changes it. */
export function glassPart({ radius, still = () => false }: { radius: number; still?: () => boolean }) {
  return (box: HTMLElement) => {
    const glass = box.querySelector<HTMLElement>(':scope > .glass');
    if (!glass) throw new Error('glassPart: no .glass layer');
    const p: Part = {
      box,
      glass,
      shade: box.querySelector<HTMLElement>(':scope > .shade'),
      main: box.querySelector<HTMLElement>(':scope > .main'),
      mainIn: null,
      radius,
      still,
      frame: frameOf(box),
      at: null,
      kids: new Map(),
      seen: 1,
      run: null,
      anims: [],
      window: windowNow(),
    };
    const first = measure(p);
    p.at = first.at;
    p.kids = first.kids;
    p.seen = first.glass;
    p.mainIn = first.mainIn;
    // A change to its children or words: played before the frame is drawn (animation frames come before layout).
    let asked = 0;
    const ask = () => {
      if (!asked) asked = requestAnimationFrame(() => ((asked = 0), play(p)));
    };
    const ours = (n: Node) => n instanceof Element && (n === p.glass || n === p.shade || n === p.main);
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
