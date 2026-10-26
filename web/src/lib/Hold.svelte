<script lang="ts">
  // A place that shows one thing at a time (the panel, the page), switching in one commit (lib/holdPlace.svelte): when `key`
  // changes, a still picture of what it showed is laid over it (the same elements, copied, scrolled as they were) while
  // the new content gets ready unseen beneath; once what it shows is ready (holdUntil), its pictures are in and the
  // whole of it shows at once. Nothing claimed: at once, as before. After HOLD_SIGN_MS the shared loading sign shows over
  // the picture; after HOLD_MAX_MS the new content shows as it is.
  import { mount, setContext, tick, unmount, untrack, type Snippet } from 'svelte';
  import { HOLD, type HoldPlace } from './holdPlace.svelte';
  import MarkLoader from './MarkLoader.svelte';

  let { key, shown = $bindable(), children }: { key: unknown; shown?: unknown; children: Snippet } = $props();

  const HOLD_SIGN_MS = 300;
  const HOLD_MAX_MS = 3000;
  /** How long a picture of the new content may take to be drawn, once it is ready. */
  const PICTURE_MS = 500;

  let box: HTMLElement;
  /** What is not ready yet, this hold's (HOLD_MAX_MS lets them all go). */
  const claims = new Set<object>();
  let holding = $state(false);
  let picture: HTMLElement | null = null;
  let sign: ReturnType<typeof mount> | null = null;
  let signTimer: ReturnType<typeof setTimeout> | undefined;
  let maxTimer: ReturnType<typeof setTimeout> | undefined;
  let round = 0;
  shown = untrack(() => key);

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

  /** A still copy of what shows now, laid over the place (once per hold: a key changing again keeps the first). */
  function takePicture() {
    if (picture || !box) return;
    const frame = box.parentElement;
    if (!frame || !box.firstElementChild) return;
    const r = frame.getBoundingClientRect();
    if (!r.width || !r.height) return;
    const p = frame.cloneNode(false) as HTMLElement;
    for (const c of box.childNodes) p.appendChild(c.cloneNode(true));
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
    frame.after(p);
    // Scrolled as it was, its drawings copied (a copy's canvas starts blank).
    const from = document.createTreeWalker(box, NodeFilter.SHOW_ELEMENT);
    const to = document.createTreeWalker(p, NodeFilter.SHOW_ELEMENT);
    to.nextNode();
    for (let a = from.nextNode(), b = to.nextNode(); a && b; a = from.nextNode(), b = to.nextNode()) {
      const ea = a as HTMLElement;
      const eb = b as HTMLElement;
      if (ea.scrollTop || ea.scrollLeft) {
        eb.scrollTop = ea.scrollTop;
        eb.scrollLeft = ea.scrollLeft;
      }
      if (ea instanceof HTMLCanvasElement && eb instanceof HTMLCanvasElement) {
        try {
          eb.getContext('2d')?.drawImage(ea, 0, 0);
        } catch {
          /* a drawing that cannot be copied stays blank */
        }
      }
    }
    picture = p;
    signTimer = setTimeout(() => {
      if (picture !== p) return;
      const at = document.createElement('div');
      at.className = 'hold-sign';
      p.appendChild(at);
      sign = mount(MarkLoader, { target: at, props: { size: 40 } });
    }, HOLD_SIGN_MS);
  }

  function dropPicture() {
    clearTimeout(signTimer);
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
      return r.width > 0 && r.bottom > frame.top && r.top < frame.bottom && r.right > frame.left && r.left < frame.right;
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
    holding = false;
    shown = key;
    dropPicture();
  }

  let last = untrack(() => key);
  $effect.pre(() => {
    const k = key;
    if (k === last) return;
    last = k;
    untrack(() => {
      const r = ++round;
      takePicture();
      holding = true;
      clearTimeout(maxTimer);
      maxTimer = setTimeout(() => {
        if (r !== round) return;
        claims.clear();
        void release(r, false);
      }, HOLD_MAX_MS);
      // What shows says in this same update whether it is ready; nothing said: at once.
      void tick().then(() => {
        if (!claims.size) void release(r, false);
      });
    });
  });
  $effect(() => () => {
    clearTimeout(maxTimer);
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
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
</style>
