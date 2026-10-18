<script lang="ts">
  // Full-screen visuals (decision 0046), from the full player's top bar: each track's own fractal flame
  // (lib/visuals/flame.ts, made from its id) drawn on the GPU (lib/visuals/render.ts), blending into the next track's
  // flame as it starts. Its six parts each answer to a band of the sound (sub to air). The music's signals
  // (lib/visuals/music.svelte.ts) drive the picture's aspects through the routes (lib/visuals/routes.ts), each by an
  // amount: by default the loudness as heard sets its size and colour, the beat flashes and swells it, each band's level
  // evolves, turns and tints its part and its hits light it. The loudness is the envelope of everything else: a quiet
  // passage is less of a show, not a darker screen. Its colours are its seed's own, the playing track's cover's or a
  // fire's (the Colours setting).
  // Over everything, on black. A tap or a mouse move shows the track, the close button, the sliders and the dice for a
  // few seconds; the dice show another flame at random, until the track changes or they are pressed again; the sliders
  // open a panel of settings at the right (lib/visuals/settings.svelte.ts) with the routes' grid, live, until it is
  // closed. Esc, Back or a swipe down closes the visuals. The screen stays on while they show, where the browser allows.
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { coverUrl } from '../api';
  import Icon from '../Icon.svelte';
  import { reducedMotion } from '../motion';
  import { playing } from '../remote.svelte';
  import { between, coloured, evolved, flameFrom, measuring, PARTS, seedOf, withSymmetry, type ColourMode, type Flame, type Part } from './flame';
  import { musicNow, stopMusic } from './music.svelte';
  import { coverPalette, type PaletteColour } from './palette';
  import { held } from './held.svelte';
  import Help from './Help.svelte';
  import { FlameRenderer } from './render';
  import {
    A,
    ASPECT_LABEL,
    ASPECTS,
    B,
    BAND_SIGNALS,
    compile,
    give,
    given,
    routeKey,
    SIGNAL_LABEL,
    T,
    TRACK_SIGNALS,
    type Aspect,
    type Signal,
  } from './routes';
  import {
    autoResolution,
    BANDS,
    DEFAULTS,
    RANGES,
    resetRoutes,
    resetVisualSettings,
    saveVisualSettings,
    visualSettings as s,
    type VisualSettings,
  } from './settings.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let canvas: HTMLCanvasElement;
  let chrome = $state(true);
  let unsupported = $state(false);
  // The settings panel, and what it shows the device reaching: frames a second, points and steps a frame (the
  // automatic step-down may keep them under the settings).
  let panel = $state(false);
  let fps = $state(0);
  let drawing = $state({ points: 0, steps: 0, cleared: 0 });
  const np = $derived(playing());

  // The controls show for a few seconds after a tap or a move; with the panel open, until it is closed.
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function showChrome() {
    chrome = true;
    clearTimeout(hideTimer);
    if (!panel) hideTimer = setTimeout(() => (chrome = false), 3000);
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
  type One = Exclude<keyof VisualSettings, 'lighting' | 'evolution' | 'continuity' | 'routes'>;
  const MIRROR = ['Auto', 'Off', 'On'];
  const COLOURS = ['Rainbow', 'Cover', 'Fire'];
  const MODES: ColourMode[] = ['rainbow', 'cover', 'fire'];
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
    { key: 'flash', label: 'Flash', step: 0.05, text: percent },
    { key: 'speed', label: 'Speed', step: 0.05, text: percent },
    { key: 'brightness', label: 'Brightness', step: 0.05, text: percent },
    { key: 'colours', label: 'Colours', step: 1, text: (v) => COLOURS[v] ?? 'Rainbow' },
    { key: 'purity', label: 'Purity', step: 0.05, text: percent },
    { key: 'spread', label: 'Spread', step: 0.05, text: percent },
    { key: 'smoothing', label: 'Smoothing', step: 0.05, text: percent },
    { key: 'floor', label: 'Floor', step: 0.05, text: percent },
    { key: 'depth', label: 'Depth', step: 0.05, text: percent },
    { key: 'symmetry', label: 'Symmetry', step: 1, text: (v) => (v ? `${v}` : 'Auto') },
    { key: 'mirror', label: 'Mirror', step: 1, text: (v) => MIRROR[v] ?? 'Auto' },
  ];
  // Each band's times, on a doubling scale.
  const BAND_ROWS = ['Sub', 'Bass', 'Low mid', 'Mid', 'Presence', 'Air'];
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

  // The routes' grid: a row a signal (the whole track's, then the bands'), a column an aspect, each cell its amount;
  // a tap on a cell gives its slider. The bands' rows are the picked band's, or all six together (their mean shown,
  // marked where they differ; a change sets all six).
  const matrix = $derived(compile(s.routes, BANDS));
  let pick = $state<number | null>(null);
  let cell = $state<{ signal: Signal; aspect: Aspect } | null>(null);
  const isBandSignal = (signal: Signal) => (BAND_SIGNALS as readonly string[]).includes(signal);
  const keysOf = (signal: Signal, aspect: Aspect) =>
    !isBandSignal(signal)
      ? [routeKey(signal, null, aspect)]
      : pick !== null
        ? [routeKey(signal, pick, aspect)]
        : Array.from({ length: BANDS }, (_, b) => routeKey(signal, b, aspect));
  function amountOf(signal: Signal, aspect: Aspect): { v: number; mixed: boolean } {
    const v = keysOf(signal, aspect).map((k) => s.routes[k] ?? 0);
    return { v: v.reduce((a, b) => a + b, 0) / v.length, mixed: v.some((x) => Math.abs(x - v[0]) > 1e-6) };
  }
  function setRoute(signal: Signal, aspect: Aspect, v: number) {
    for (const k of keysOf(signal, aspect)) {
      // A 0 is kept only where it turns a default off.
      if (v === 0 && !DEFAULTS.routes[k]) delete s.routes[k];
      else s.routes[k] = v;
    }
    saveVisualSettings();
  }
  const signed = (v: number) => `${v > 0 ? '+' : ''}${Math.round(v * 100)}%`;
  const cellLabel = (signal: Signal, aspect: Aspect) => `${SIGNAL_LABEL[signal]}${isBandSignal(signal) ? ` (${pick === null ? 'All' : BAND_ROWS[pick]})` : ''} → ${ASPECT_LABEL[aspect]}`;
  // Each signal now, for the meters by the rows' names (the picked band's, or the bands' mean), ten times a second
  // while the panel shows.
  let meters = $state<{ track: number[]; bands: number[][] }>({ track: [], bands: [] });
  const meterOf = (signal: Signal) => {
    if (!isBandSignal(signal)) return meters.track[T[signal as keyof typeof T]] ?? 0;
    const i = B[signal as keyof typeof B];
    if (pick !== null) return meters.bands[pick]?.[i] ?? 0;
    return meters.bands.length ? meters.bands.reduce((a, b) => a + (b[i] ?? 0), 0) / meters.bands.length : 0;
  };
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
    } catch (e) {
      // Why, for a debugger attached to the device (the page says only that it cannot).
      console.warn('Visuals:', e);
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
    // The playing track's flame as made (or the dice's), and the symmetry it is shown with (its own, or the
    // settings'): a change of either blends in as the dice do.
    let base: Flame | null = null;
    let symmetryShown = '';
    // A moment's symmetry (the routes' Symmetry), until the next one or another flame.
    let momentSymmetry: { order: number; mirror: boolean } | null = null;
    // The Colours setting's Cover: the playing track's cover's colours, asked for as it starts (and the next one's, so
    // they are in as it starts); null until they are, and where it has no cover. Fire: how much of its heat shows,
    // followed over a second and a half.
    let cover: { key: string; colours: PaletteColour[] | null } | null = null;
    let fire = 0;
    let turn = 0;
    // The turning's clock: a route's full amount turns a part once around in 16 beats (the default routes, once in 32
    // on average), at the Speed setting; paused, it slows to a stop.
    let clock = 0;
    // The signals now (the loudness's as the Intensity setting lets it count), the bands' stand-ins while none are
    // known (each half way), and what the routes give each aspect.
    const track = new Float32Array(TRACK_SIGNALS.length);
    const standIn = Array.from({ length: BANDS }, () => {
      const v = new Float32Array(BAND_SIGNALS.length);
      v[B.level] = 0.5;
      return v;
    });
    const got = given(BANDS);
    // Each part: how far it has turned, and its evolving aspects (shape, hue, spread) as followed over its band's
    // Evolution time.
    const turned = Array<number>(PARTS).fill(0);
    const evolving = Array.from({ length: PARTS }, () => [0.5, 0, 0]);
    // Each part's steps: its band's last hit seen (or beat, where its hits are not known yet), what its evolving aspects
    // were given then (what a stepping part evolves to), and the turn of its steps still to come (each eased in over
    // 80 ms).
    const lastHit = Array<number | null>(PARTS).fill(null);
    const held = Array.from({ length: PARTS }, () => [0.5, 0, 0]);
    const pending = Array<number>(PARTS).fill(0);
    // The moments' state: ready again once what they are given falls under a third, and not within two seconds.
    const moments = { reseed: { armed: true, at: -Infinity }, symmetry: { armed: true, at: -Infinity } };
    const MOMENT_GAP_MS = 2000;
    // The Flash setting: each quickly changing aspect (each part's lights, the glow, size and saturation) as its slow
    // course (followed twice over a third of a second: a beat's or a hit's swing all but gone from it, a passage's
    // change kept) and the setting's share of its swing about that course.
    const SLOW_S = 0.3;
    const courses = { lights: Array.from({ length: PARTS }, () => [] as number[]), glow: [] as number[], size: [] as number[], saturation: [] as number[] };
    let follow2 = 0;
    const flashed = (v: number, course: number[]) => {
      if (!course.length) course.push(v, v);
      course[0] += (v - course[0]) * follow2;
      course[1] += (course[0] - course[1]) * follow2;
      return course[1] + s.flash * (v - course[1]);
    };
    // The grain's settings (0..1) as their routes move them: a route's full amount moves one half of its range.
    const grain = (setting: number, aspect: number) => clamp(setting + 0.5 * got.whole[aspect], 0, 1);
    // Floor: none at 0, then from 0.002 of the display's scale (a stray speck's) up to 1 (the flame's mean), evenly in
    // its log.
    const floorOf = (v: number) => (v > 0 ? 0.002 * 500 ** v : 0);
    // How intense the show is (the loudness as heard, followed over a fifth of a second; `lit`: as far as the
    // Intensity setting lets it count), and paused or not.
    let intensity = 0.5;
    let lit = 0.5;
    let glow = 1;
    let metered = 0;
    let passes = s.steps;
    let raf = 0;
    let last = performance.now();
    let drawnAt = 0;
    // The display's own frame interval, measured (a frame rate that is not a whole part of it stutters: frames are
    // drawn every so many of the display's, never between).
    let refresh = 1000 / 60;
    let lastTick = performance.now();
    // Frame times over the last second, to adapt the load; frames drawn, for the panel's figure.
    // The device's pace: each second's frames late for their slot; the step-down's state (see the end of the loop).
    let late = 0;
    let drawn = 0;
    let worst = 0;
    let judged = performance.now();
    // The resolution the step-down gives up before points (a share of the Resolution setting's).
    let fit = 1;
    // Seconds in a row with no late frame needed before a step back up; doubled each time a step up was soon taken
    // back, so the load settles instead of swinging (back to 3 after a minute with nothing taken back).
    let calm = 0;
    let wait = 3;
    let steppedUp = 0;
    let steppedDown = 0;
    // The framing and exposure, measured from the flame twice a second (a quarter of the measure each frame, so no one
    // frame waits for all of it) and followed over a second and a half.
    let frame: { center: [number, number]; size: number; coverage: number } | null = null;
    let measured = 0;
    let measure: ReturnType<typeof measuring> | null = null;
    let counted = { since: performance.now(), frames: 0 };

    const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
    // A moment: what its routes give rising past a half.
    const moment = (m: { armed: boolean; at: number }, v: number, now: number) => {
      if (v < 1 / 3) m.armed = true;
      if (!(v > 0.5 && m.armed && now - m.at > MOMENT_GAP_MS)) return false;
      m.armed = false;
      m.at = now;
      return true;
    };

    const loop = (now: number) => {
      raf = requestAnimationFrame(loop);
      const tick = now - lastTick;
      lastTick = now;
      // Learnt from the display's quick frames only (up to 50 a second): a device behind on every frame calls back as
      // slowly as it draws, and its own pace taken for the display's made every frame look on time (2026-10-18).
      if (tick > 4 && tick < 21) refresh += (tick - refresh) * 0.05;
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
        drawing = { points: renderer.pointCount, steps: passes, cleared: renderer.scrubbed };
      }
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      if (renderer.lost) {
        console.warn('Visuals: the GPU took the context away', { points: renderer.pointCount, steps: passes, precision: renderer.precision });
        renderer = null;
        unsupported = true;
        return;
      }

      const m = musicNow(now);
      if (m.key !== key || asked !== seen) {
        // The first track shows at once; each next one blends in, as does another look from the dice. A new track
        // brings back its own.
        const next = m.key !== key;
        if (next) another = null;
        seen = asked;
        from = key === undefined ? null : shown;
        key = m.key;
        base = flameFrom(another ?? seedOf(m.key ?? 'cratarr'));
        symmetryShown = '';
        momentSymmetry = null;
        blendStart = now;
        blendMs = next ? BLEND_MS : ANOTHER_MS;
      }
      // The colours: the cover's asked for as a track starts, where they are wanted.
      const mode = MODES[s.colours] ?? 'rainbow';
      const playingNow = playing();
      if (mode === 'cover' && playingNow && m.key && cover?.key !== m.key) {
        const want = m.key;
        cover = { key: want, colours: null };
        coverPalette(coverUrl(playingNow)).then(
          (p) => {
            if (cover?.key === want) cover = { key: want, colours: p?.colours ?? null };
          },
          () => {},
        );
        if (playingNow.upcoming[0]) coverPalette(coverUrl({ source: playingNow.source, id: playingNow.upcoming[0].id })).catch(() => {});
      }
      const covered = mode === 'cover' && cover?.key === m.key ? cover.colours : null;
      fire += ((mode === 'fire' ? 1 : 0) - fire) * Math.min(1, dt / 1.5);
      // Its symmetry: its own (or a moment's), or as the settings ask; its colours, as the Colours setting asks. A change
      // blends in over a second and a half; the cover's colours coming in while a track's flame blends in join that
      // blend instead.
      if (base) {
        const order = s.symmetry || momentSymmetry?.order || base.symmetry.order;
        const mirror = s.mirror ? s.mirror === 2 : (momentSymmetry?.mirror ?? base.symmetry.mirror);
        const shape = `${order}/${mirror}`;
        const want = `${shape}/${mode}/${covered ? m.key : ''}`;
        if (want !== symmetryShown) {
          const coloursOnly = from && symmetryShown.startsWith(`${shape}/`);
          if (symmetryShown !== '' && shown && !coloursOnly) {
            from = shown;
            blendStart = now;
            blendMs = ANOTHER_MS;
          }
          symmetryShown = want;
          to = coloured(order === base.symmetry.order && mirror === base.symmetry.mirror ? base : withSymmetry(base, order, mirror), mode, covered);
          if (!from) shown = to;
        }
      }
      // The show's intensity: the loudness as heard, followed over a fifth of a second, as far as the Intensity setting
      // lets it count. It is the loudness's signal, and the envelope of every other one's routes (none while paused).
      // Dimmer while paused.
      intensity += (m.intensity - intensity) * Math.min(1, dt * 5);
      lit = 1 - s.intensity * (1 - intensity);
      glow += ((m.playing ? 1 : 0.55) - glow) * Math.min(1, dt * 2);
      // Reduced motion: a quarter of the turning, all of it steady, half the beats' and hits' share, no moments.
      const still = reducedMotion();
      track.set(m.track);
      track[T.loudness] = lit;
      if (still) for (const t of [T.beat, T.downbeat, T.section]) track[t] *= 0.5;
      const bandsNow = m.bands ?? standIn;
      give(matrix, track, bandsNow, m.playing ? lit : 0, got);
      const W = got.whole;
      follow2 = 1 - Math.exp(-dt / SLOW_S);
      if (panel && now - metered > 100) {
        metered = now;
        meters = { track: Array.from(track), bands: bandsNow.map((v) => Array.from(v)) };
      }
      // The turning's clock, at the track's tempo (its beats' where they are known, 90 BPM without one); the whole
      // picture turns with the whole track's routes to Rotation.
      const b = m.playing ? m.beat : null;
      const want = m.playing ? STEP * m.bps * (still ? 0.25 : 1) * s.speed : 0;
      clock += (want - clock) * Math.min(1, dt * 1.5);
      turn += clock * 0.6 * W[A.rotation] * dt;
      // Each part, given the whole track's routes and its band's: Shape evolves it (larger, its variations given over
      // to its band's own), Hue moves its colour round, Spread draws it out from the middle or in (all three followed
      // over its band's Evolution time), Rotation turns it, Lights light its colour. Its band's Continuity shares its
      // motion between steady (turning at its rate, evolving as it is given) and steps on its band's own hits (or the
      // beats, where those are not known yet): a step turns it by as much as it would have turned steadily since the
      // band's last, on average (a strong hit further, a weak one less), and evolves it to what it was given at that
      // moment, held until the next.
      const hits: number[] = [];
      const parts: Part[] = [];
      for (let i = 0; i < PARTS; i++) {
        const k = Math.floor((i * BANDS) / PARTS);
        const own = got.band[k];
        const c = still ? 1 : s.continuity[k];
        const wants = [W[A.shape] + own[A.shape], W[A.hue] + own[A.hue], W[A.spread] + own[A.spread]];
        const rate = clock * 2 * (W[A.rotation] + own[A.rotation]);
        const step = m.playing && m.hits ? m.hits[k] : null;
        const at = step ? step.index : b ? b.index : null;
        if (at !== lastHit[i]) {
          const was = lastHit[i];
          // Backwards or by more than three: a seek or another track, not stepped for.
          if (at !== null && was !== null && at > was && at - was <= 3) {
            held[i] = wants;
            const often = Math.max(0.5, step ? step.perSecond : m.bps);
            pending[i] += ((1 - c) * rate * (step ? step.weight : 1)) / often;
          }
          lastHit[i] = at;
        }
        const eased = pending[i] * Math.min(1, dt / 0.08);
        turned[i] += c * rate * dt + eased;
        pending[i] -= eased;
        const follow = Math.min(1, dt / s.evolution[k]);
        for (let e = 0; e < 3; e++) evolving[i][e] += (c * wants[e] + (1 - c) * held[i][e] - evolving[i][e]) * follow;
        const [shape, hue, spread] = evolving[i];
        const grown = clamp(shape, 0, 1.25);
        parts.push({ turn: turned[i], size: 0.94 + 0.12 * grown, morph: (still ? 0.15 : 0.35) * grown, hue, spread: 2 ** clamp(spread, -2, 2) });
        // Lights: its colour's points lit up, or dimmed where its routes take it below nothing.
        hits.push(clamp(flashed(clamp(W[A.lights] + own[A.lights], -1, 3) * (still ? 0.5 : 1), courses.lights[i]), -1, 3));
      }
      // The moments: another flame (as the dice give), another symmetry (where the Symmetry setting is Auto).
      if (!still && moment(moments.reseed, W[A.reseed], now)) {
        another = Math.floor(Math.random() * 0x100000000);
        asked++;
      }
      if (!still && moment(moments.symmetry, W[A.symmetry], now) && base && !s.symmetry) {
        const order = momentSymmetry?.order ?? base.symmetry.order;
        momentSymmetry = { order: ((order + Math.floor(Math.random() * 7)) % 8) + 1, mirror: Math.random() < 0.5 };
      }

      if (!to) return;
      const k = from ? Math.min(1, (now - blendStart) / blendMs) : 1;
      shown = from && k < 1 ? between(from, to, k * k * (3 - 2 * k)) : to;
      if (k >= 1) from = null;
      const drawnFlame = evolved(shown, parts, s.purity, s.spread);
      if (!measure && now - measured > 500) {
        measured = now;
        // The first all at once: the picture starts framed.
        measure = frame ? measuring(drawnFlame) : measuring(drawnFlame, 6000, 6000);
      }
      const measuredNow = measure?.next();
      if (measuredNow?.done) {
        measure = null;
        const seen = measuredNow.value;
        if (seen) {
          if (!frame) frame = seen;
          else {
            const k = 1 - Math.exp(-0.5 / 1.5);
            frame = {
              center: [frame.center[0] + (seen.center[0] - frame.center[0]) * k, frame.center[1] + (seen.center[1] - frame.center[1]) * k],
              size: Math.exp(Math.log(frame.size) + (Math.log(seen.size) - Math.log(frame.size)) * k),
              coverage: frame.coverage + (seen.coverage - frame.coverage) * k,
            };
          }
        }
      }
      renderer.setFlame(frame ? { ...drawnFlame, center: frame.center, size: frame.size } : drawnFlame);

      // The Resolution setting (by default about a megapixel, `autoResolution`) of the screen's own pixels; the screen
      // scales it up.
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const r = (s.resolution ?? autoResolution(canvas.clientWidth * canvas.clientHeight * dpr * dpr)) * fit;
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
          // Trails: each frame keeps this much of the last (longer trails given more, shorter given less).
          keep: clamp(1 - 0.07 * 2 ** (-1.5 * W[A.trails]), 0.5, 0.985),
          // Depth: the display's gamma, 1.5 to 6 (3 as before at its middle); higher lifts more of the faint detail.
          gamma: 1.5 * 4 ** grain(s.depth, A.depth),
          vibrancy: 1,
          turn,
          // Size: half its size at rest, all of it given all (by default the loudness: smaller when quiet, and a
          // beat's swell).
          scale: clamp(flashed(0.5 + 0.5 * W[A.size], courses.size), 0.25, 1.4),
          zoom: 1,
          // Glow: as bright at rest (dimmer only while paused), brighter given more (by default a beat's flash).
          flash: glow * clamp(flashed(1 + 0.5 * W[A.brightness], courses.glow), 0.15, 3),
          hits,
          ceiling: s.brightness,
          // Saturation: a third of its colour at rest, all of it given all (by default the loudness: paler when quiet).
          saturation: clamp(flashed(0.35 + 0.65 * W[A.saturation], courses.saturation), 0, 1.25),
          // The whole picture as bright however much of its frame the flame covers: a flame spread over most of it is
          // shown dimmer, one gathered into a few cells brighter (against one covering a third of its frame), so its
          // shape's changes are not flares.
          exposure: frame ? Math.min(1.5, Math.max(0.4, 0.33 / Math.max(0.05, frame.coverage))) : 1,
          // Smoothing: up to 12 pixels (3 as before at its middle). Floor: from none to about the flame's mean density.
          smoothing: 12 * grain(s.smoothing, A.smoothing) ** 2,
          floor: floorOf(grain(s.floor, A.floor)),
          fire,
        },
        w,
        h,
      );

      // Adapting, under the Points, Steps and Resolution settings, to hold the FPS setting rather than settle under it.
      // Each second: if more than a fifth of its frames came late for their slot, a step down (steps, then resolution to
      // 60 % of the setting's, then points, halved; several at once where frames took over twice their slot). A step
      // back up (points, then resolution, then steps) only after `wait` seconds in a row with no late frame; a step up
      // taken back within five seconds doubles the wait (up to a minute), so the load settles instead of swinging. The
      // step-down judged its window of frames once per frame after it filled until 2026-10-17 (from one step to the most
      // within a second, behind again, over and over).
      const most = Math.round(2 ** s.points);
      if (renderer.pointCount > most) renderer.setPoints(most);
      passes = Math.min(passes, s.steps);
      drawn++;
      if (spent > frameMs * 1.08 + 2) late++;
      worst = Math.max(worst, spent);
      if (now - judged >= 1000) {
        if (late * 5 > drawn) {
          calm = 0;
          if (now - steppedUp < 5000) wait = Math.min(60, wait * 2);
          steppedDown = now;
          for (let n = worst > frameMs * 2 ? 2 : 1; n > 0; n--) {
            if (passes > 1) passes--;
            else if (fit > 0.6) fit = Math.max(0.6, fit * 0.85);
            else if (renderer.pointCount > MIN_POINTS) renderer.setPoints(Math.max(MIN_POINTS, renderer.pointCount >> 1));
          }
        } else if (late === 0 && ++calm >= wait) {
          calm = 0;
          steppedUp = now;
          if (renderer.pointCount < most) renderer.setPoints(Math.min(most, renderer.pointCount * 2));
          else if (fit < 1) fit = Math.min(1, fit / 0.85);
          else if (passes < s.steps) passes++;
        }
        if (now - steppedDown > 60_000) wait = 3;
        late = 0;
        drawn = 0;
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
      stopMusic();
    };
  });
</script>

{#snippet routeRow(signal: Signal)}
  <span class="signal" use:held={`signal:${signal}`}>{SIGNAL_LABEL[signal]}<i style:width="{Math.round(Math.min(1, Math.max(0, meterOf(signal))) * 100)}%"></i></span>
  {#each ASPECTS as aspect (aspect)}
    {@const a = amountOf(signal, aspect)}
    <button
      class="cell"
      class:negative={a.v < 0}
      class:mixed={a.mixed}
      class:picked={cell?.signal === signal && cell?.aspect === aspect}
      style:--amount={Math.min(1, Math.abs(a.v))}
      aria-label="{cellLabel(signal, aspect)}, {signed(a.v)}"
      onclick={() => (cell = { signal, aspect })}><span>{a.v ? Math.round(a.v * 100) : ''}</span></button
    >
  {/each}
{/snippet}

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
        <button class="round" aria-label="Another look" use:held={'dice'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={rollAnother}
          ><Icon name="dice" size={26} /></button
        >
      </div>
      {#if panel}
        <div class="panel" role="group" aria-label="Settings" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()}>
          {#each [COMPUTE, LOOK] as rows, g (g)}
            <div class="rows" class:look={g === 1}>
              {#each rows as row (row.key)}
                <label class="row">
                  <span use:held={row.key}>{row.label}</span>
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
          <div class="routes" role="group" aria-label="Routes">
            <div class="head">
              <h3 use:held={'routes'}>Routes</h3>
              <button
                class="reset"
                use:held={'routes:reset'}
                onclick={() => {
                  resetRoutes();
                  cell = null;
                }}>Reset</button
              >
            </div>
            <div class="grid" style:--columns={ASPECTS.length}>
              <span></span>
              {#each ASPECTS as aspect (aspect)}
                <span class="aspect" use:held={`aspect:${aspect}`}>{ASPECT_LABEL[aspect]}</span>
              {/each}
              {#each TRACK_SIGNALS as signal (signal)}
                {@render routeRow(signal)}
              {/each}
              <div class="bands" role="radiogroup" aria-label="Band">
                {#each [null, 0, 1, 2, 3, 4, 5] as b (b ?? -1)}
                  <button class="chip" class:on={pick === b} role="radio" aria-checked={pick === b} onclick={() => (pick = b)}
                    >{b === null ? 'All' : BAND_ROWS[b]}</button
                  >
                {/each}
              </div>
              {#each BAND_SIGNALS as signal (signal)}
                {@render routeRow(signal)}
              {/each}
            </div>
            {#if cell}
              {@const { signal, aspect } = cell}
              {@const a = amountOf(signal, aspect)}
              <label class="route">
                <span>{cellLabel(signal, aspect)}</span>
                <input
                  class="amount"
                  class:negative={a.v < 0}
                  type="range"
                  min="-1"
                  max="1"
                  step="0.05"
                  value={a.v}
                  style:--lo="{50 + Math.min(0, a.v) * 50}%"
                  style:--hi="{50 + Math.max(0, a.v) * 50}%"
                  oninput={(e) => setRoute(signal, aspect, Number(e.currentTarget.value))}
                />
                <output>{signed(a.v)}</output>
              </label>
            {/if}
          </div>
          {#each BAND_ROWS as band, b (band)}
            <div class="rows band">
              <h3>{band}</h3>
              {#each TIMES as time (time.key)}
                <label class="row">
                  <span use:held={time.key}>{time.label}</span>
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
                <span use:held={'continuity'}>Continuity</span>
                <input type="range" min={RANGES.continuity[0]} max={RANGES.continuity[1]} step="0.05" value={s.continuity[b]} oninput={(e) => setContinuity(b, Number(e.currentTarget.value))} />
                <output>{percent(s.continuity[b])}</output>
              </label>
            </div>
          {/each}
          <div class="foot">
            <span class="fps"
              >{fps} fps · {thousands(Math.log2(Math.max(1, drawing.points)))} × {drawing.steps}{drawing.cleared ? ` · ${drawing.cleared} cleared` : ''}</span
            >
            <button
              class="reset"
              use:held={'reset'}
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
  <Help />
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
    width: min(500px, calc(100% - 24px));
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: rgb(255 255 255 / 0.25) transparent;
    box-sizing: border-box;
    padding: 8px 16px 10px;
    border-radius: 14px;
    /* No blur behind it: blurring the moving picture under it every frame cost a tablet more than the picture did. */
    background: rgb(20 20 24 / 0.92);
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

  .rows h3,
  .routes h3 {
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

  .routes {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgb(255 255 255 / 0.12);
    container-type: inline-size;
  }

  .routes .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(4.4em, 1fr) repeat(var(--columns), minmax(0, 2.1em));
    gap: 2px;
    margin-top: 4px;
  }

  .aspect {
    justify-self: center;
    padding: 4px 0;
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    font-size: 0.72rem;
    color: rgb(255 255 255 / 0.6);
    white-space: nowrap;
  }

  .signal {
    position: relative;
    align-self: center;
    padding: 0 4px 3px 0;
    overflow: hidden;
    font-size: 0.8rem;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  /* What the signal is now. */
  .signal i {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    border-radius: 1px;
    background: rgb(255 255 255 / 0.55);
  }

  .cell {
    position: relative;
    min-width: 0;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    /* Opaque: the picture behind the panel would tint an empty cell. */
    background: rgb(42 42 48);
    color: #fff;
    font: inherit;
    font-size: 0.62rem;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }

  .cell::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--accent);
    opacity: calc(var(--amount) * 0.85);
  }

  .cell.negative::before {
    background: #e8708f;
  }

  .cell span {
    position: relative;
  }

  .cell.mixed {
    outline: 1px dashed rgb(255 255 255 / 0.5);
    outline-offset: -2px;
  }

  .cell.picked {
    outline: 2px solid #fff;
    outline-offset: -1px;
  }

  /* Too narrow for the amounts: their colour alone. */
  @container (max-width: 420px) {
    .cell span {
      display: none;
    }
  }

  .bands {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 8px 0 4px;
  }

  .chip {
    min-height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 15px;
    background: rgb(255 255 255 / 0.08);
    color: #fff;
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .chip.on {
    background: rgb(255 255 255 / 0.85);
    color: #000;
  }

  /* The picked route's slider, kept in sight under the grid. */
  .route {
    position: sticky;
    /* Down to the panel's edge (its padding), so nothing shows under it. */
    bottom: -10px;
    display: grid;
    grid-template-columns: 1fr 3.4em;
    align-items: center;
    gap: 2px 12px;
    margin: 8px -8px 0;
    padding: 8px 10px 10px;
    border-radius: 10px 10px 0 0;
    background: rgb(36 36 42);
    box-shadow: 0 -6px 12px rgb(0 0 0 / 0.35);
    font-size: 0.85rem;
  }

  .route span {
    grid-column: 1 / -1;
    color: rgb(255 255 255 / 0.75);
  }

  /* An amount's slider fills from its middle (0) toward its thumb: green up, pink down. */
  .route input {
    --fill: var(--accent);
    --track: rgb(255 255 255 / 0.2);
    width: 100%;
    min-width: 0;
    height: 28px;
    margin: 0;
    background: transparent;
    -webkit-appearance: none;
    appearance: none;
  }

  .route input.negative {
    --fill: #e8708f;
  }

  .route input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background:
      linear-gradient(to right, transparent calc(50% - 1px), rgb(255 255 255 / 0.7) calc(50% - 1px) calc(50% + 1px), transparent calc(50% + 1px)),
      linear-gradient(to right, var(--track) var(--lo), var(--fill) var(--lo) var(--hi), var(--track) var(--hi));
  }

  .route input::-moz-range-track {
    height: 4px;
    border-radius: 2px;
    background:
      linear-gradient(to right, transparent calc(50% - 1px), rgb(255 255 255 / 0.7) calc(50% - 1px) calc(50% + 1px), transparent calc(50% + 1px)),
      linear-gradient(to right, var(--track) var(--lo), var(--fill) var(--lo) var(--hi), var(--track) var(--hi));
  }

  .route input::-webkit-slider-thumb {
    width: 20px;
    height: 20px;
    margin-top: -8px;
    border: 0;
    border-radius: 50%;
    background: var(--fill);
    -webkit-appearance: none;
    appearance: none;
  }

  .route input::-moz-range-thumb {
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 50%;
    background: var(--fill);
  }

  .route output {
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
