<script lang="ts">
  // Full-screen visuals (decision 0046), from the full player's top bar: each track's own fractal flame
  // (lib/visuals/flame.ts, made from its id) drawn on the GPU (lib/visuals/render.ts), its transforms turning a step
  // and the picture flashing on each of the track's beats, brighter where the track is loud (lib/visuals/music.svelte.ts),
  // blending into the next track's flame as it starts. Each of its three transforms answers to a band of the sound
  // (low, middle, high): the band's transients light that transform's colour, its loudness evolves that transform.
  // Over everything, on black. A tap or a mouse move shows the track, the close button and the dice for a few seconds;
  // the dice show another flame at random, until the track changes or they are pressed again. Esc, Back or a swipe down
  // closes it. The screen stays on while it shows, where the browser allows that.
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import Icon from '../Icon.svelte';
  import { reducedMotion } from '../motion';
  import { playing } from '../remote.svelte';
  import { BANDS } from './bands';
  import { between, evolved, flameFrom, seedOf, type Flame, type Part } from './flame';
  import { musicNow } from './music.svelte';
  import { FlameRenderer } from './render';

  let { onclose }: { onclose: () => void } = $props();

  let canvas: HTMLCanvasElement;
  let chrome = $state(true);
  let unsupported = $state(false);
  const np = $derived(playing());

  // The controls show for a few seconds after a tap or a move.
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function showChrome() {
    chrome = true;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => (chrome = false), 3000);
  }

  // Closing: by the button, Esc or a swipe down this step of the history goes back too; Back has done that already.
  let closed = false;
  function close() {
    if (closed) return;
    closed = true;
    if (history.state?.visuals) history.back();
    onclose();
  }

  // A swipe down closes, a tap shows the controls.
  let down: { x: number; y: number } | null = null;
  function pointerDown(e: PointerEvent) {
    down = { x: e.clientX, y: e.clientY };
  }
  function pointerUp(e: PointerEvent) {
    if (!down) return;
    const dx = e.clientX - down.x;
    const dy = e.clientY - down.y;
    down = null;
    if (dy > 80 && Math.abs(dy) > 2 * Math.abs(dx)) close();
    else if (Math.hypot(dx, dy) < 10) (chrome ? (chrome = false) : showChrome());
  }

  // The dice: a flame from another seed than the track's own, each press another (`asked` counts the presses).
  let another: number | null = null;
  let asked = 0;
  function rollAnother() {
    another = Math.floor(Math.random() * 0x100000000);
    asked++;
    showChrome();
  }

  // How fast it may draw: 30 frames a second on touch devices (battery and heat), 60 elsewhere.
  const FRAME_MS = matchMedia('(pointer: coarse)').matches ? 1000 / 30 : 1000 / 60;
  // The points, and the passes over them per frame, adapted to what the device keeps up with.
  const POINTS = [1 << 15, 1 << 16, 1 << 17, 1 << 18];
  const MAX_PASSES = 12;
  // A new track's flame blends in over this long; one asked for with the dice, quicker.
  const BLEND_MS = 6000;
  const ANOTHER_MS = 1500;
  // A beat's turn of the transforms: once around in 32 beats.
  const STEP = (2 * Math.PI) / 32;

  onMount(() => {
    showChrome();
    history.pushState({ ...history.state, visuals: true }, '');
    const popped = () => {
      if (!closed && !history.state?.visuals) {
        closed = true;
        onclose();
      }
    };
    addEventListener('popstate', popped);
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && close();
    addEventListener('keydown', onKey);
    document.documentElement.requestFullscreen?.({ navigationUI: 'hide' }).catch(() => {});

    let lock: WakeLockSentinel | null = null;
    const wake = () => {
      if (document.visibilityState !== 'visible' || !('wakeLock' in navigator)) return;
      navigator.wakeLock.request('screen').then(
        (l) => (lock = l),
        () => {},
      );
    };
    wake();
    document.addEventListener('visibilitychange', wake);

    let renderer: FlameRenderer | null = null;
    let level = 2;
    try {
      renderer = new FlameRenderer(canvas, POINTS[level]);
    } catch {
      unsupported = true;
    }

    // What shows: the flame blended from (`from`) to the playing track's (`to`); `shown` is the last one drawn, so a
    // track that starts mid-blend blends on from where it was.
    let key: string | null | undefined;
    let from: Flame | null = null;
    let to: Flame | null = null;
    let shown: Flame | null = null;
    let blendStart = 0;
    let blendMs = BLEND_MS;
    let seen = 0;
    let turn = 0;
    let speed = 0;
    // Each band's transform: how far it has turned, and its band's loudness, followed over a quarter of a second (no
    // bands known: as if each were half way).
    const turned = [0, 0, 0];
    const bandLoud = [0.5, 0.5, 0.5];
    // Beats counted at the last frame (each eased: most of a beat's turn comes at its start), and a beat's flash and
    // swell, 0..1.
    let beatCount: number | null = null;
    let pulse = 0;
    let loud = 0.6;
    let glow = 1;
    let passes = 4;
    let raf = 0;
    let last = performance.now();
    let drawnAt = 0;
    // Frame times over the last second, to adapt the load.
    let slow = 0;
    let quick = 0;

    // A turn of `by` (radians): each band's transform turns by it, further the louder its band.
    const turnBands = (by: number) => {
      for (let i = 0; i < BANDS; i++) turned[i] += by * (0.4 + 1.2 * bandLoud[i]);
    };

    const loop = (now: number) => {
      raf = requestAnimationFrame(loop);
      if (!renderer || now - drawnAt < FRAME_MS - 2) return;
      const spent = now - drawnAt;
      drawnAt = now;
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      if (renderer.lost) {
        renderer = null;
        unsupported = true;
        return;
      }

      const m = musicNow(now);
      if (m.key !== key || asked !== seen) {
        // The first track shows at once; each next one blends in, as does another look from the dice. A new track
        // brings back its own.
        const track = m.key !== key;
        if (track) another = null;
        seen = asked;
        from = key === undefined ? null : shown;
        key = m.key;
        to = flameFrom(another ?? seedOf(m.key ?? 'cratarr'));
        shown = to;
        blendStart = now;
        blendMs = track ? BLEND_MS : ANOTHER_MS;
      }
      // The transforms turn a step on every beat, most of it at once (once around in 32 beats), and each beat flashes
      // and swells the picture, fading within a fifth of a second (sooner at a fast tempo), most on the track's strong
      // beats. Between known beats (none worked out yet, no tempo, a break) they turn steadily at the tempo (once per
      // 21 s without one); paused, they slow to a stop. Reduced motion: a quarter of the turning, no swell, a softer
      // flash.
      const still = reducedMotion();
      const b = m.playing ? m.beat : null;
      if (b) {
        const f = Math.min(1, b.since / b.length);
        const counted = b.index + (still ? f : 1 - (1 - f) ** 3);
        const moved = beatCount === null ? 0 : counted - beatCount;
        // Backwards or by more than two beats: a seek, not turned for.
        if (moved > 0 && moved < 2) {
          turnBands(STEP * moved * (still ? 0.25 : 1));
          turn += STEP * 0.12 * moved;
        }
        beatCount = counted;
        speed = (STEP / b.length) * (still ? 0.25 : 1);
        pulse = (0.35 + 0.65 * b.strength) * Math.exp(-b.since / Math.min(0.2, 0.35 * b.length));
      } else {
        beatCount = null;
        const beats = (m.bpm ?? 90) / 60;
        const want = m.playing ? STEP * beats * (still ? 0.25 : 1) : 0;
        speed += (want - speed) * Math.min(1, dt * 1.5);
        turnBands(speed * dt);
        turn += speed * 0.12 * dt;
        pulse *= Math.exp(-dt / 0.15);
      }
      // Brighter where the track is loud, following it quickly; dimmer while paused.
      loud += ((m.energy ?? 0.6) - loud) * Math.min(1, dt * 6);
      glow += ((m.playing ? 1 : 0.55) - glow) * Math.min(1, dt * 2);
      // The bands: each one's loudness evolves its transform (turning further on each beat, larger, its variations
      // given over to its band's own, its colour moved along the palette), its transients light its colour.
      for (let i = 0; i < BANDS; i++) bandLoud[i] += ((m.bands?.[i].loud ?? 0.5) - bandLoud[i]) * Math.min(1, dt * 4);
      const hits: [number, number, number] = [0, 1, 2].map((i) => (m.playing ? (m.bands?.[i].hit ?? 0) : 0)) as [number, number, number];
      const parts: Part[] = bandLoud.map((l, i) => ({
        turn: turned[i],
        size: 0.94 + 0.12 * l,
        morph: (still ? 0.15 : 0.35) * l,
        hue: 0.08 * (l - 0.5),
      }));

      if (!to) return;
      const k = from ? Math.min(1, (now - blendStart) / blendMs) : 1;
      shown = from && k < 1 ? between(from, to, k * k * (3 - 2 * k)) : to;
      if (k >= 1) from = null;
      renderer.setFlame(evolved(shown, parts));

      const dpr = Math.min(devicePixelRatio || 1, 2);
      const w = Math.round(canvas.clientWidth * dpr);
      const h = Math.round(canvas.clientHeight * dpr);
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      // The points fall into a buffer of at most about 0.6 megapixels; the screen scales it up (softer, and cheaper).
      const scale = Math.min(1, Math.sqrt(600_000 / (w * h)));
      renderer.resize(Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale)));
      renderer.frame(
        passes,
        {
          brightness: (0.6 + 0.5 * loud) * glow,
          keep: 0.93,
          gamma: 3,
          vibrancy: 1,
          turn,
          zoom: 1 + (still ? 0 : 0.04) * pulse,
          // With the bands known, their own lights carry most of the beat.
          flash: 1 + (still ? 0.15 : m.bands ? 0.1 : 0.35) * pulse,
          hits: still ? (hits.map((h) => h * 0.5) as [number, number, number]) : hits,
        },
        w,
        h,
      );

      // Adapting: a second of late frames takes passes (then points) away, a few seconds of quick ones add them back.
      if (spent > FRAME_MS * 1.35) slow++;
      else if (spent < FRAME_MS * 1.1) quick++;
      if (slow + quick >= 30) {
        if (slow > 10) {
          if (passes > 2) passes--;
          else if (level > 0) renderer.setPoints(POINTS[--level]);
          quick = 0;
        } else if (quick > 28 && passes < MAX_PASSES) passes++;
        slow = 0;
        if (quick > 90) quick = 0;
      }
    };
    raf = requestAnimationFrame(loop);

    return () => {
      // Gone some other way (the shell's page changed): its step of the history goes too.
      if (!closed && history.state?.visuals) history.back();
      cancelAnimationFrame(raf);
      clearTimeout(hideTimer);
      removeEventListener('popstate', popped);
      removeEventListener('keydown', onKey);
      document.removeEventListener('visibilitychange', wake);
      lock?.release().catch(() => {});
      if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
      renderer?.dispose();
    };
  });
</script>

<div
  class="visuals"
  class:bare={!chrome}
  role="dialog"
  aria-modal="true"
  aria-label="Visuals"
  tabindex="-1"
  onpointerdown={pointerDown}
  onpointerup={pointerUp}
  onpointermove={(e) => e.pointerType === 'mouse' && !down && showChrome()}
>
  <canvas bind:this={canvas}></canvas>
  {#if unsupported}
    <p class="note">This device cannot draw the visuals.</p>
  {/if}
  {#if chrome}
    <div class="chrome" transition:fade={{ duration: reducedMotion() ? 0 : 200 }}>
      <button class="close" aria-label="Close the visuals" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={close}
        ><Icon name="close" size={26} /></button
      >
      <button class="another" aria-label="Another look" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={rollAnother}
        ><Icon name="dice" size={26} /></button
      >
      {#if np}
        <div class="track">
          <strong>{np.name}</strong>
          {#if np.artist}<span>{np.artist}</span>{/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .visuals {
    position: fixed;
    inset: 0;
    z-index: 2000;
    background: #000;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .visuals.bare {
    cursor: none;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }

  .note {
    position: absolute;
    inset: auto 0 45% 0;
    margin: 0;
    color: #bbb;
    text-align: center;
    font-size: 0.95rem;
  }

  .chrome {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(to bottom, rgb(0 0 0 / 0.45), transparent 22%, transparent 75%, rgb(0 0 0 / 0.55));
  }

  .close,
  .another {
    position: absolute;
    top: max(12px, env(safe-area-inset-top));
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: #fff;
    pointer-events: auto;
    cursor: pointer;
  }

  .close {
    left: max(12px, env(safe-area-inset-left));
  }

  .another {
    right: max(12px, env(safe-area-inset-right));
  }

  .close:focus-visible,
  .another:focus-visible {
    background: rgb(255 255 255 / 0.12);
  }

  /* A touch screen keeps a tapped button "hovered": the circle only under a mouse. */
  @media (hover: hover) {
    .close:hover,
    .another:hover {
      background: rgb(255 255 255 / 0.12);
    }
  }

  .track {
    position: absolute;
    left: max(24px, env(safe-area-inset-left));
    right: 24px;
    bottom: max(24px, env(safe-area-inset-bottom));
    display: grid;
    gap: 2px;
    color: #fff;
    text-shadow: 0 1px 6px rgb(0 0 0 / 0.6);
  }

  .track strong {
    font-size: 1.15rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track span {
    color: rgb(255 255 255 / 0.75);
    font-size: 0.95rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
