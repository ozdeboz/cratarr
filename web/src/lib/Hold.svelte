<script lang="ts">
  // A place that shows one thing at a time (the panel, the page), switching in one commit (lib/holdPlace.svelte): when `key`
  // changes, what it showed goes at once (the owner, 2026-10-27: never old content on navigation, always responsive):
  // the place's own background is laid over it, the shared loading sign on it after HOLD_SIGN_MS, while the new content
  // gets ready unseen beneath; once what it shows is ready (holdUntil), its pictures are in and the whole of it shows at
  // once (what comes from elsewhere may follow). Nothing claimed: at once, as before. After HOLD_GIVE_UP_MS as it is. A
  // place opening (the app starting, the phone's panel coming up) holds its first content the same way. `shown`: the key
  // whose content shows (what a page marks as open follows it, not the key asked for).
  import { mount, setContext, tick, unmount, untrack, type Snippet } from 'svelte';
  import { HOLD, type HoldPlace } from './holdPlace.svelte';
  import MarkLoader from './MarkLoader.svelte';

  // `seeThrough`: opening, the place is not there at all until its content is ready (the phone's panel over the page):
  // what was tapped shows the loading sign meanwhile (lib/panel `opening`); after HOLD_MAX_MS the place's background
  // with the sign.
  let {
    key,
    shown = $bindable(),
    seeThrough = false,
    children,
  }: { key: unknown; shown?: unknown; seeThrough?: boolean; children: Snippet } = $props();

  /** The sign a moment after (a page there at once shows no flash of it). */
  const HOLD_SIGN_MS = 100;
  /** How long a see-through place opening stays away before its background and the sign show. */
  const HOLD_MAX_MS = 3000;
  const HOLD_GIVE_UP_MS = 10000;
  /** How long the new content's big pictures may take to be drawn, once it is ready; the small ones (a row's) may follow. */
  const PICTURE_MS = 400;
  const BIG_PICTURE = 96 * 96;

  let box: HTMLElement;
  /** What is not ready yet, this hold's (HOLD_GIVE_UP_MS lets them all go). */
  const claims = new Set<object>();
  let holding = $state(false);
  let picture: HTMLElement | null = null;
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

  /** The place's own background laid over it (once per hold: a key changing again keeps the first): the frame's
   *  element alone, without what is in it. */
  function takePicture() {
    if (picture || !box) return;
    const frame = box.parentElement;
    if (!frame) return;
    const r = frame.getBoundingClientRect();
    if (!r.width || !r.height) return;
    const p = frame.cloneNode(false) as HTMLElement;
    p.removeAttribute('id');
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
    picture = p;
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

  /** A see-through opening's place, hidden until then. */
  let away: HTMLElement | null = null;

  function dropPicture() {
    clearTimeout(signTimer);
    if (away) away.style.visibility = '';
    away = null;
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

  /** A hold begins: the place's background over it, the sign on it, until ready. */
  function begin(opening = false) {
    const r = ++round;
    if (opening && seeThrough && box?.parentElement) {
      away = box.parentElement;
      away.style.visibility = 'hidden';
    } else takePicture();
    holding = true;
    clearTimeout(maxTimer);
    clearTimeout(giveUpTimer);
    maxTimer = setTimeout(() => {
      if (r !== round) return;
      if (!away) return;
      away.style.visibility = '';
      away = null;
      takePicture();
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
