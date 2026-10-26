<script lang="ts">
  // A place that shows one thing at a time (the panel, the page), switching in one commit (lib/holdPlace.svelte): when `key`
  // changes, a still picture of what it showed is laid over it (the same elements, copied, scrolled as they were) while
  // the new content gets ready unseen beneath; once what it shows is ready (holdUntil), its pictures are in and the
  // whole of it shows at once. Nothing claimed: at once, as before. After HOLD_SIGN_MS the shared loading sign shows over
  // the picture; after HOLD_MAX_MS what was there gives way to the place's background with the sign (a page before kept
  // that long says nothing true any more), the new content still showing whole once ready; after HOLD_GIVE_UP_MS as it
  // is. A place opening (the app starting, the phone's panel coming up) holds its first content too, over its own empty
  // background. `shown`: the key whose content shows (what a page marks as open follows it, not the key asked for).
  // `along`: what of the window outside the place belongs to its content (a page's controls row, which the new page
  // fills at once): pictured with it, the new one hidden until the switch (a5's lab, 2026-10-27).
  import { mount, setContext, tick, unmount, untrack, type Snippet } from 'svelte';
  import { HOLD, type HoldPlace } from './holdPlace.svelte';
  import MarkLoader from './MarkLoader.svelte';

  let {
    key,
    shown = $bindable(),
    along = () => [],
    children,
  }: { key: unknown; shown?: unknown; along?: () => HTMLElement[]; children: Snippet } = $props();

  const HOLD_SIGN_MS = 300;
  const HOLD_MAX_MS = 3000;
  const HOLD_GIVE_UP_MS = 10000;
  /** How long the new content's big pictures may take to be drawn, once it is ready; the small ones (a row's) may follow. */
  const PICTURE_MS = 400;
  const BIG_PICTURE = 96 * 96;

  let box: HTMLElement;
  /** What is not ready yet, this hold's (HOLD_MAX_MS lets them all go). */
  const claims = new Set<object>();
  let holding = $state(false);
  let picture: HTMLElement | null = null;
  /** `along`'s pictures, and the live ones hidden meanwhile. */
  let parts: HTMLElement[] = [];
  let hidden: HTMLElement[] = [];
  let sign: ReturnType<typeof mount> | null = null;
  let signTimer: ReturnType<typeof setTimeout> | undefined;
  let maxTimer: ReturnType<typeof setTimeout> | undefined;
  let giveUpTimer: ReturnType<typeof setTimeout> | undefined;
  let round = 0;

  const place: HoldPlace = {
    claim() {
      const c = {};
      claims.add(c);
      return () => {
        if (claims.delete(c) && !claims.size) void release(round);
      };
    },
  };
  setContext(HOLD, place);

  /** A still copy of what shows now, laid over the place (once per hold: a key changing again keeps the first);
   *  `blank`, opening: the place's background only (what is there is the new content). */
  function takePicture(blank = false) {
    if (picture || !box) return;
    const frame = box.parentElement;
    if (!frame) return;
    const r = frame.getBoundingClientRect();
    if (!r.width || !r.height) return;
    const p = frame.cloneNode(false) as HTMLElement;
    // Down to the foot of what can be seen only (a crate's hundreds of rows below it cost a still tap, a5's lab); read
    // before anything is added, so nothing is laid out again meanwhile.
    const foot = Math.min(r.bottom, innerHeight);
    const scrolled: [HTMLElement, number, number][] = [];
    const drawings: [HTMLCanvasElement, HTMLCanvasElement][] = [];
    const copy = (from: Node, into: Node) => {
      for (const n of from.childNodes) {
        if (!(n instanceof Element)) {
          into.appendChild(n.cloneNode(false));
          continue;
        }
        if (n instanceof SVGElement || n.getBoundingClientRect().top >= foot) {
          if (!(n instanceof SVGElement)) continue;
          into.appendChild(n.cloneNode(true));
          continue;
        }
        const c = n.cloneNode(false) as Element;
        into.appendChild(c);
        if (n.scrollTop || n.scrollLeft) scrolled.push([c as HTMLElement, n.scrollTop, n.scrollLeft]);
        if (n instanceof HTMLCanvasElement) drawings.push([n, c as HTMLCanvasElement]);
        copy(n, c);
      }
    };
    if (!blank) copy(box, p);
    p.removeAttribute('id');
    for (const e of p.querySelectorAll('[id]')) e.removeAttribute('id');
    p.setAttribute('aria-hidden', 'true');
    p.inert = true;
    Object.assign(p.style, {
      position: 'fixed',
      left: `${r.left}px`,
      top: `${r.top}px`,
      width: `${r.width}px`,
      height: `${r.height}px`,
      margin: '0',
      pointerEvents: 'none',
      visibility: 'visible',
    });
    p.classList.add('hold-picture');
    // On the place's own layer, after it: over it (a page is lifted above what is behind it).
    const z = getComputedStyle(frame).zIndex;
    if (z !== 'auto') p.style.zIndex = z;
    frame.after(p);
    // Scrolled as it was, its drawings copied (a copy's canvas starts blank).
    for (const [e, top, left] of scrolled) {
      e.scrollTop = top;
      e.scrollLeft = left;
    }
    for (const [from, to] of drawings) {
      try {
        to.getContext('2d')?.drawImage(from, 0, 0);
      } catch {
        /* a drawing that cannot be copied stays blank */
      }
    }
    picture = p;
    if (!blank) {
      for (const el of along()) {
        const q = el.getBoundingClientRect();
        if (!q.width || !q.height) continue;
        const c = el.cloneNode(true) as HTMLElement;
        for (const e of c.querySelectorAll('[id]')) e.removeAttribute('id');
        c.setAttribute('aria-hidden', 'true');
        c.inert = true;
        Object.assign(c.style, { position: 'fixed', left: `${q.left}px`, top: `${q.top}px`, width: `${q.width}px`, height: `${q.height}px`, margin: '0', right: 'auto', bottom: 'auto', transform: 'none', pointerEvents: 'none' });
        c.classList.add('hold-picture');
        el.after(c);
        parts.push(c);
      }
    }
    // The live ones stay hidden while held: the new content's, filled at once.
    hidden = along();
    for (const el of hidden) el.style.visibility = 'hidden';
    signTimer = setTimeout(() => {
      if (picture !== p) return;
      const at = document.createElement('div');
      at.className = 'hold-sign';
      // In the middle of what can be seen of the place (a page may reach far past the window).
      const top = Math.max(0, -r.top);
      at.style.top = `${top}px`;
      at.style.height = `${Math.max(0, Math.min(r.height, innerHeight - r.top) - top)}px`;
      p.appendChild(at);
      sign = mount(MarkLoader, { target: at, props: { size: 40 } });
    }, HOLD_SIGN_MS);
  }

  /** `along`'s pictures gone, the live ones shown. */
  function dropParts() {
    for (const c of parts) c.remove();
    parts = [];
    for (const el of hidden) el.style.visibility = '';
    hidden = [];
  }

  function dropPicture() {
    clearTimeout(signTimer);
    dropParts();
    if (sign) void unmount(sign);
    sign = null;
    picture?.remove();
    picture = null;
  }

  /** The new content's pictures in the place's view, drawn (or PICTURE_MS). */
  function picturesIn(): Promise<unknown> {
    const frame = box?.parentElement?.getBoundingClientRect();
    if (!frame) return Promise.resolve();
    const imgs = [...box.querySelectorAll('img')].filter((i) => {
      const r = i.getBoundingClientRect();
      return r.width * r.height >= BIG_PICTURE && r.bottom > frame.top && r.top < frame.bottom && r.right > frame.left && r.left < frame.right;
    });
    const drawn = Promise.all(imgs.map((i) => i.decode().catch(() => {})));
    return Promise.race([drawn, new Promise((r) => setTimeout(r, PICTURE_MS))]);
  }

  /** `waited`: something claimed, and its pictures are waited for too; nothing claimed shows at once. */
  async function release(r: number, waited = true) {
    if (r !== round || !holding) return;
    if (waited) await picturesIn();
    if (r !== round || claims.size) return;
    clearTimeout(maxTimer);
    clearTimeout(giveUpTimer);
    holding = false;
    shown = key;
    dropPicture();
  }

  /** A hold begins: the picture of what shows (none yet: the place's background), until ready or HOLD_MAX_MS. */
  function begin(opening = false) {
    const r = ++round;
    takePicture(opening);
    holding = true;
    clearTimeout(maxTimer);
    clearTimeout(giveUpTimer);
    maxTimer = setTimeout(() => {
      if (r !== round || !picture) return;
      // The background and the sign, no longer what was there.
      for (const c of [...picture.children]) if (!c.classList.contains('hold-sign')) c.remove();
      dropParts();
    }, HOLD_MAX_MS);
    giveUpTimer = setTimeout(() => {
      if (r !== round) return;
      claims.clear();
      void release(r, false);
    }, HOLD_GIVE_UP_MS);
    // What shows says in this same update whether it is ready; nothing said: at once.
    void tick().then(() => {
      if (!claims.size) void release(r, false);
    });
  }

  let last = untrack(() => key);
  $effect.pre(() => {
    const k = key;
    if (k === last) return;
    last = k;
    untrack(() => begin());
  });
  // Opening: hidden from the start, the picture (its background only) once the place is there.
  holding = true;
  $effect(() => untrack(() => begin(true)));
  $effect(() => () => {
    clearTimeout(maxTimer);
    clearTimeout(giveUpTimer);
    dropPicture();
  });
</script>

<div class="hold" class:holding bind:this={box} inert={holding || undefined}>{@render children()}</div>

<style>
  .hold {
    display: contents;
  }

  /* Beneath the picture, getting ready unseen (laid out, its pictures loading). */
  .holding {
    visibility: hidden;
  }

  :global(.hold-picture .hold-sign) {
    position: absolute;
    left: 0;
    right: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
</style>
