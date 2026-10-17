<script lang="ts">
  // Full-screen visuals (decision 0046), from the full player's top bar: each track's own fractal flame
  // (lib/visuals/flame.ts, made from its id) drawn on the GPU (lib/visuals/render.ts), its transforms turning a step
  // and the picture flashing on each of the track's beats (lib/visuals/music.svelte.ts), blending into the next track's
  // flame as it starts. The whole show is as intense as the music is loud as heard: its size, colour, motion and every
  // light (not its brightness: a quiet passage is less of a show, not a darker screen). Within that, each of its three transforms answers to a band of the sound (low, middle, high): the band's
  // transients light that transform's colour, its loudness evolves that transform.
  // Over everything, on black. A tap or a mouse move shows the track, the close button, the sliders and the dice for a
  // few seconds; the dice show another flame at random, until the track changes or they are pressed again; the sliders
  // open a panel of settings at the right (lib/visuals/settings.svelte.ts), live. Esc, Back or a swipe down closes it. The screen
  // stays on while it shows, where the browser allows that.
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import Icon from '../Icon.svelte';
  import { reducedMotion } from '../motion';
  import { playing } from '../remote.svelte';
  import { BANDS } from './bands';
  import { between, evolved, flameFrom, seedOf, type Flame, type Part } from './flame';
  import { musicNow } from './music.svelte';
  import { FlameRenderer } from './render';
  import { autoResolution, RANGES, resetVisualSettings, saveVisualSettings, visualSettings as s, type VisualSettings } from './settings.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let canvas: HTMLCanvasElement;
  let chrome = $state(true);
  let unsupported = $state(false);
  // The settings panel, and what it shows the device reaching: frames a second, points and steps a frame (the
  // automatic step-down may keep them under the settings).
  let panel = $state(false);
  let fps = $state(0);
  let drawing = $state({ points: 0, steps: 0 });
  const np = $derived(playing());

  // The controls show for a few seconds after a tap or a move.
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function showChrome() {
    chrome = true;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(
      () => {
        chrome = false;
        panel = false;
      },
      panel ? 8000 : 3000,
    );
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
    else if (Math.hypot(dx, dy) < 10) {
      if (chrome) {
        chrome = false;
        panel = false;
      } else showChrome();
    }
  }

  // The dice: a flame from another seed than the track's own, each press another (`asked` counts the presses).
  let another: number | null = null;
  let asked = 0;
  function rollAnother() {
    another = Math.floor(Math.random() * 0x100000000);
    asked++;
    showChrome();
  }

  // The settings' rows: the drawing's cost first (ceilings: the device's own pace may keep it lower), then the look.
  type One = Exclude<keyof VisualSettings, 'lighting' | 'evolution' | 'continuity'>;
  type Row = { key: One; label: string; step: number; text: (v: number) => string };
  const percent = (v: number) => `${Math.round(v * 100)}%`;
  const thousands = (log2: number) => {
    const n = 2 ** log2;
    return n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)}M` : `${Math.round(n / 1024)}k`;
  };
  const COMPUTE: Row[] = [
    { key: 'points', label: 'Points', step: 0.25, text: thousands },
    { key: 'steps', label: 'Steps', step: 1, text: (v) => `${v}` },
    { key: 'resolution', label: 'Resolution', step: 0.05, text: percent },
    { key: 'fps', label: 'FPS', step: 5, text: (v) => `${v}` },
  ];
  const LOOK: Row[] = [
    { key: 'intensity', label: 'Intensity', step: 0.05, text: percent },
    { key: 'speed', label: 'Speed', step: 0.05, text: percent },
    { key: 'beat', label: 'Beat', step: 0.05, text: percent },
    { key: 'bands', label: 'Bands', step: 0.05, text: percent },
    { key: 'brightness', label: 'Brightness', step: 0.05, text: percent },
  ];
  // Each band's times, on a doubling scale.
  const BAND_ROWS = ['Sub', 'Mid', 'Treble'];
  const TIMES = (['lighting', 'evolution'] as const).map((key) => ({
    key,
    label: key === 'lighting' ? 'Lighting' : 'Evolution',
    min: Math.log2(RANGES[key][0]),
    max: Math.log2(RANGES[key][1]),
  }));
  const seconds = (v: number) => (v < 1 ? `${Math.round(v * 1000)} ms` : `${v.toFixed(1)} s`);
  function setTime(key: 'lighting' | 'evolution', band: number, log2: number) {
    s[key][band] = 2 ** log2;
    saveVisualSettings();
    showChrome();
  }
  function setContinuity(band: number, v: number) {
    s.continuity[band] = v;
    saveVisualSettings();
    showChrome();
  }
  const devicePixels = () => innerWidth * innerHeight * Math.min(devicePixelRatio || 1, 2) ** 2;
  const valueOf = (key: One) => (key === 'resolution' ? (s.resolution ?? autoResolution(devicePixels())) : s[key]) as number;
  function set(key: One, v: number) {
    (s as Record<One, number | null>)[key] = v;
    saveVisualSettings();
    showChrome();
  }
  function togglePanel() {
    panel = !panel;
    showChrome();
  }
  // The fewest points the device's own pace steps down to.
  const MIN_POINTS = 1 << 14;
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
    try {
      renderer = new FlameRenderer(canvas, Math.round(2 ** s.points));
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
    // Each band's steps: its last transient seen (or beat, where its transients are not known yet), its loudness then
    // (what a stepping band evolves to), and the turn of its steps still to come (each eased in over 80 ms).
    const lastHit: (number | null)[] = [null, null, null];
    const held = [0.5, 0.5, 0.5];
    const pending = [0, 0, 0];
    // A beat's flash and swell, 0..1.
    let pulse = 0;
    // How intense the show is (the loudness as heard, followed over a fifth of a second; `lit`: as far as the
    // Intensity setting lets it count), and paused or not.
    let intensity = 0.5;
    let lit = 0.5;
    let glow = 1;
    let passes = s.steps;
    let raf = 0;
    let last = performance.now();
    let drawnAt = 0;
    // The display's own frame interval, measured (a frame rate that is not a whole part of it stutters: frames are
    // drawn every so many of the display's, never between).
    let refresh = 1000 / 60;
    let lastTick = performance.now();
    // Frame times over the last second, to adapt the load; frames drawn, for the panel's figure.
    let slow = 0;
    let quick = 0;
    let worst = 0;
    let judged = performance.now();
    let counted = { since: performance.now(), frames: 0 };

    // How much of the turning the music's loudness allows (nearly none in silence), and the Speed setting.
    const motion = () => (0.1 + 0.9 * lit) * s.speed;

    const loop = (now: number) => {
      raf = requestAnimationFrame(loop);
      const tick = now - lastTick;
      lastTick = now;
      if (tick > 4 && tick < 50) refresh += (tick - refresh) * 0.05;
      // Every so many of the display's frames, as many as keep it at or under the FPS setting.
      const every = Math.max(1, Math.ceil((1000 / s.fps - 1) / refresh));
      const frameMs = every * refresh;
      if (!renderer || now - drawnAt < frameMs - refresh / 2) return;
      const spent = now - drawnAt;
      drawnAt = now;
      counted.frames++;
      if (now - counted.since >= 1000) {
        fps = Math.round((counted.frames * 1000) / (now - counted.since));
        counted = { since: now, frames: 0 };
        drawing = { points: renderer.pointCount, steps: passes };
      }
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
      // The whole show follows how loud the track is as heard: small, still, pale and unlit in silence or a fade, all of
      // it at the loudest. Dimmer while paused.
      intensity += (m.intensity - intensity) * Math.min(1, dt * 5);
      lit = 1 - s.intensity * (1 - intensity);
      glow += ((m.playing ? 1 : 0.55) - glow) * Math.min(1, dt * 2);
      // The transforms turn once around in 32 beats on average, at the track's tempo (its beats' where they are known,
      // 90 BPM without one), as far as the loudness allows; paused, they slow to a stop. Each beat flashes and swells the
      // picture, fading within a fifth of a second (sooner at a fast tempo), most on the strong beats. Reduced motion: a
      // quarter of the turning, all of it steady, no swell, softer lights.
      const still = reducedMotion();
      const b = m.playing ? m.beat : null;
      const beatsASecond = b ? 1 / b.length : (m.bpm ?? 90) / 60;
      const want = m.playing ? STEP * beatsASecond * (still ? 0.25 : 1) * motion() : 0;
      speed += (want - speed) * Math.min(1, dt * 1.5);
      turn += speed * 0.12 * dt;
      pulse = b ? (0.35 + 0.65 * b.strength) * Math.exp(-b.since / Math.min(0.2, 0.35 * b.length)) : pulse * Math.exp(-dt / 0.15);
      // Each band, within that: its loudness evolves its transform (larger, its variations given over to its band's own,
      // its colour moved along the palette; and it turns further), its transients light its colour. Its Continuity
      // setting shares its motion between steady (turning at its speed, evolving as its loudness goes) and steps on its
      // own transients (or the beats, where those are not known yet): a step turns it by as much as it would have
      // turned steadily since the band's last, on average (a strong transient further, a weak one less), and evolves it
      // to its loudness at that moment, held until the next.
      for (let i = 0; i < BANDS; i++) {
        const band = m.playing ? m.bands?.[i] : undefined;
        const c = still ? 1 : s.continuity[i];
        const loud = (band?.loud ?? 0.5) * lit;
        const rate = speed * (0.4 + 1.2 * bandLoud[i]);
        const at = band ? band.index : b ? b.index : null;
        if (at !== lastHit[i]) {
          const was = lastHit[i];
          // Backwards or by more than three: a seek or another track, not stepped for.
          if (at !== null && was !== null && at > was && at - was <= 3) {
            held[i] = loud;
            const often = Math.max(0.5, band ? band.perSecond : beatsASecond);
            pending[i] += ((1 - c) * rate * (band ? band.weight : 1)) / often;
          }
          lastHit[i] = at;
        }
        const eased = pending[i] * Math.min(1, dt / 0.08);
        turned[i] += c * rate * dt + eased;
        pending[i] -= eased;
        bandLoud[i] += (c * loud + (1 - c) * held[i] - bandLoud[i]) * Math.min(1, dt / s.evolution[i]);
      }
      const hits = [0, 1, 2].map((i) => (m.playing ? (m.bands?.[i].hit ?? 0) * lit * s.bands * (still ? 0.5 : 1) : 0)) as [number, number, number];
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

      // The Resolution setting (by default about a megapixel, `autoResolution`) of the screen's own pixels; the screen
      // scales it up.
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const r = s.resolution ?? autoResolution(canvas.clientWidth * canvas.clientHeight * dpr * dpr);
      const w = Math.max(1, Math.round(canvas.clientWidth * dpr * r));
      const h = Math.max(1, Math.round(canvas.clientHeight * dpr * r));
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
          brightness: 1,
          keep: 0.93,
          gamma: 3,
          vibrancy: 1,
          turn,
          // Smaller when quiet (half its size in silence), its full size at the loudest; a beat's swell as loud as the
          // music.
          scale: 0.5 + 0.5 * lit,
          zoom: 1 + (still ? 0 : 0.04) * pulse * lit * s.beat,
          // As bright at any loudness (dimmer only while paused); a beat's flash as loud as the music, smaller where the
          // bands' own lights carry the beat.
          flash: glow * (1 + (still ? 0.15 : m.bands ? 0.1 : 0.35) * pulse * lit * s.beat),
          hits,
          ceiling: s.brightness,
          // Paler when quiet: a third of its colour in silence.
          saturation: 0.35 + 0.65 * lit,
        },
        w,
        h,
      );

      // Adapting, under the Points and Steps settings: each second (or 30 frames, if sooner), a third of them late takes
      // a step away (half of them where the frames took over twice as long as they should; then points, halved), 28
      // quick ones of 30 add a step back (then points), never past the settings. Each window is judged once and the
      // next starts afresh: a count of quick frames kept on past its judgement added a step at every frame after it,
      // from one step to the most within a second, and the device fell behind again (until 2026-10-17).
      const most = Math.round(2 ** s.points);
      if (renderer.pointCount > most) renderer.setPoints(most);
      passes = Math.min(passes, s.steps);
      if (spent > frameMs * 1.35) slow++;
      else if (spent < frameMs * 1.1) quick++;
      worst = Math.max(worst, spent);
      if (slow + quick >= 30 || (now - judged >= 1000 && slow + quick >= 2)) {
        if (slow * 3 > slow + quick) {
          const far = worst > frameMs * 2;
          if (passes > 1) passes = far ? Math.max(1, passes >> 1) : passes - 1;
          else if (renderer.pointCount > MIN_POINTS) renderer.setPoints(Math.max(MIN_POINTS, renderer.pointCount >> (far ? 2 : 1)));
          quick = 0;
        } else if (quick >= 28) {
          if (passes < s.steps) passes++;
          else if (renderer.pointCount < most) renderer.setPoints(Math.min(most, renderer.pointCount * 2));
        }
        slow = 0;
        quick = 0;
        worst = 0;
        judged = now;
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
      <div class="corner">
        <button
          class="round"
          class:on={panel}
          aria-label="Settings"
          aria-expanded={panel}
          onpointerdown={(e) => e.stopPropagation()}
          onpointerup={(e) => e.stopPropagation()}
          onclick={togglePanel}><Icon name="sliders" size={24} /></button
        >
        <button class="round" aria-label="Another look" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={rollAnother}
          ><Icon name="dice" size={26} /></button
        >
      </div>
      {#if panel}
        <div class="panel" role="group" aria-label="Settings" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()}>
          {#each [COMPUTE, LOOK] as rows, g (g)}
            <div class="rows" class:look={g === 1}>
              {#each rows as row (row.key)}
                <label class="row">
                  <span>{row.label}</span>
                  <input
                    type="range"
                    min={RANGES[row.key][0]}
                    max={RANGES[row.key][1]}
                    step={row.step}
                    value={valueOf(row.key)}
                    oninput={(e) => set(row.key, Number(e.currentTarget.value))}
                  />
                  <output>{row.text(valueOf(row.key))}</output>
                </label>
              {/each}
            </div>
          {/each}
          {#each BAND_ROWS as band, b (band)}
            <div class="rows band">
              <h3>{band}</h3>
              {#each TIMES as time (time.key)}
                <label class="row">
                  <span>{time.label}</span>
                  <input
                    type="range"
                    min={time.min}
                    max={time.max}
                    step="0.05"
                    value={Math.log2(s[time.key][b])}
                    oninput={(e) => setTime(time.key, b, Number(e.currentTarget.value))}
                  />
                  <output>{seconds(s[time.key][b])}</output>
                </label>
              {/each}
              <label class="row">
                <span>Continuity</span>
                <input type="range" min={RANGES.continuity[0]} max={RANGES.continuity[1]} step="0.05" value={s.continuity[b]} oninput={(e) => setContinuity(b, Number(e.currentTarget.value))} />
                <output>{percent(s.continuity[b])}</output>
              </label>
            </div>
          {/each}
          <div class="foot">
            <span class="fps">{fps} fps · {thousands(Math.log2(Math.max(1, drawing.points)))} × {drawing.steps}</span>
            <button
              class="reset"
              onclick={() => {
                resetVisualSettings();
                showChrome();
              }}>Reset</button
            >
          </div>
        </div>
      {/if}
      {#if np && !panel}
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
  .round {
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
    position: absolute;
    top: max(12px, env(safe-area-inset-top));
    left: max(12px, env(safe-area-inset-left));
  }

  .corner {
    position: absolute;
    top: max(12px, env(safe-area-inset-top));
    right: max(12px, env(safe-area-inset-right));
    display: flex;
    gap: 4px;
  }

  .round.on {
    background: rgb(255 255 255 / 0.18);
  }

  .close:focus-visible,
  .round:focus-visible {
    background: rgb(255 255 255 / 0.12);
  }

  /* A touch screen keeps a tapped button "hovered": the circle only under a mouse. */
  @media (hover: hover) {
    .close:hover,
    .round:hover {
      background: rgb(255 255 255 / 0.12);
    }
  }

  .panel {
    position: absolute;
    top: calc(max(12px, env(safe-area-inset-top)) + 56px);
    right: max(12px, env(safe-area-inset-right));
    bottom: max(12px, env(safe-area-inset-bottom));
    width: min(340px, calc(100% - 24px));
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: rgb(255 255 255 / 0.25) transparent;
    box-sizing: border-box;
    padding: 8px 16px 10px;
    border-radius: 14px;
    background: rgb(20 20 24 / 0.88);
    backdrop-filter: blur(8px);
    color: #fff;
    pointer-events: auto;
    touch-action: pan-y;
  }

  .rows.look,
  .rows.band {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgb(255 255 255 / 0.12);
  }

  .rows h3 {
    margin: 2px 0 0;
    font-size: 0.8rem;
    font-weight: 600;
    color: rgb(255 255 255 / 0.6);
  }

  .row {
    display: grid;
    grid-template-columns: 6.2em 1fr 3.4em;
    align-items: center;
    gap: 12px;
    min-height: 38px;
    font-size: 0.92rem;
  }

  .row input {
    width: 100%;
    min-width: 0;
    margin: 0;
    accent-color: var(--accent);
  }

  .row output {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.75);
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 6px;
  }

  .fps {
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.6);
  }

  .reset {
    min-height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 18px;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
    font: inherit;
    font-size: 0.9rem;
    cursor: pointer;
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
