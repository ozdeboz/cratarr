<script lang="ts">
  // Full-screen visuals (decision 0046), from the full player's top bar: a fractal flame (lib/visuals/flame.ts) drawn on
  // the GPU (lib/visuals/render.ts), made from the look's settings alone. Each track loads its own preset into them (from
  // its id), blending in as it starts; a setting the owner moved is pinned and stays. Its six parts each answer to a band
  // of the sound (sub to air). The music's signals (lib/visuals/music.svelte.ts) drive the picture's aspects through the
  // routes (lib/visuals/routes.ts), each by an amount: by default the loudness as heard sets its size and colour, the
  // beat flashes and swells it, each band's level evolves, turns and tints its part and its hits light it. The loudness
  // is the envelope of everything else: a quiet passage is less of a show, not a darker screen. Its colours are its
  // preset's own, the playing track's cover's or a fire's (the Colours setting).
  // Over everything, on black. A tap or a mouse move shows the track, the close button, the sliders and the dice for a
  // few seconds; the dice load a random preset into the settings not pinned; the sliders open a panel of settings at the
  // right (lib/visuals/settings.svelte.ts) with the routes' grid, live, until it is closed. Esc, Back or a swipe down
  // closes the visuals. The screen stays on while they show, where the browser allows.
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { coverUrl, type VisualsSnapshot } from '../api';
  import Icon from '../Icon.svelte';
  import { reducedMotion } from '../motion';
  import { player } from '../live.svelte';
  import { panel as sidePanel } from '../panel.svelte';
  import { prefs } from '../prefs.svelte';
  import { playing, send } from '../remote.svelte';
  import {
    between,
    coloured,
    evolved,
    flameOf,
    LENSES,
    measuring,
    PARTS,
    partsAt,
    PRESET_KEYS,
    randomFrom,
    drawing as presetDraws,
    seedOf,
    type ColourMode,
    type Flame,
    type Part,
    type Drawn,
    type Preset,
  } from './flame';
  import { musicNow, stopMusic } from './music.svelte';
  import { newAspects, stepAspects } from './aspects';
  import { newDrift, stepDrift } from './drift';
  import { Meters, type FrameStats } from './meters';
  import RateMode from './RateMode.svelte';
  import { trial } from './trial.svelte';
  import { coverPalette, type PaletteColour } from './palette';
  import { setVisualsSide, visuals } from './shown.svelte';
  import { applyDevice, applyShared, followGroup, groupLook, share, SHARED_KEYS, sharedNow, showing, STANDARD } from './shared.svelte';
  import SnapshotsSheet from './SnapshotsSheet.svelte';
  import { held } from './held.svelte';
  import Help from './Help.svelte';
  import type { Force } from './fluid';
  import { FlameRenderer } from './render';
  import {
    A,
    ASPECT_LABEL,
    ASPECTS,
    B,
    BAND_SIGNALS,
    compile,
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
    FLUID_FULL,
    fluidFrom63,
    RANGES,
    resetRoutes,
    loadPreset,
    pinned,
    resetVisualSettings,
    saveVisualSettings,
    visualSettings as s,
    type VisualSettings,
  } from './settings.svelte';

  // `docked`: beside the page, in the panel at the right (App.svelte), rather than over all of it.
  let { onclose, docked = false }: { onclose: () => void; docked?: boolean } = $props();

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

  // The look: the preset last loaded (the playing track's, or the dice's or a moment's), which a pin let go of returns
  // to; how the next change of the look blends in (set by whatever loads a preset: a track over six seconds, the dice
  // and the moments over a second and a half); and a count of the panel's Resets, after which the track's own comes back.
  let current: Preset | null = null;
  let nextBlend: number | null = null;
  let resets = 0;
  // A preset being drawn (a track's, the dice's or a moment's), a few draws a frame: each is checked on the CPU, a few
  // milliseconds on a slow tablet, and all of them at once held a frame up by a seventh of a second there (2026-10-18).
  // Then how it blends in, and whether it goes to the sync group (the dice's, a Reset's: a track's and the moments' come
  // out the same on every device anyway).
  let drawingNow: { draws: Generator<void, Drawn>; blend: number; share: boolean; load: boolean; why: Look } | null = null;
  function loadAnother(seed: number, toGroup = false, why: Look = 'dice') {
    drawingNow = { draws: presetDraws(seed, pinned()), blend: ANOTHER_MS, share: toGroup, load: true, why };
  }
  // A new look this frame, and why (the meters: a track's own, or one chosen, does not count against the budget).
  type Look = NonNullable<FrameStats['look']>;
  let lookCame: FrameStats['look'] = null;
  // The rating page (?rate=1, RateMode.svelte): its trial drawn in place of the settings while it runs.
  const rating = typeof location !== 'undefined' && new URLSearchParams(location.search).has('rate');
  // The dice: a random preset into the look's settings not pinned.
  function rollAnother() {
    loadAnother(Math.floor(Math.random() * 0x100000000), true);
    showChrome();
  }

  // The saved looks (snapshots: on the server, every device's) in their sheet (SnapshotsSheet.svelte), from the
  // bookmark. Recalling one puts it back, every look setting pinned (the next track leaves it), and to the group;
  // Standard lets every pin go. A picture for a new one is taken from the canvas in the frame it is drawn (the canvas
  // keeps nothing after it shows).
  let snapshotsOpen = $state(false);
  let pictureWanted: ((url: string | null) => void) | null = null;
  function capturePicture(): Promise<string | null> {
    if (unsupported) return Promise.resolve(null);
    return new Promise((done) => {
      pictureWanted = done;
      setTimeout(() => {
        if (pictureWanted !== done) return;
        pictureWanted = null;
        done(null);
      }, 1000);
    });
  }
  /** The middle of the picture at 16:9, 160 x 90, as a JPEG. */
  function pictureOf(from: HTMLCanvasElement): string | null {
    try {
      const out = document.createElement('canvas');
      out.width = 160;
      out.height = 90;
      const ctx = out.getContext('2d');
      if (!ctx) return null;
      const r = Math.min(from.width / 16, from.height / 9);
      ctx.fillStyle = '#000';
      ctx.fillRect(0, 0, 160, 90);
      ctx.drawImage(from, (from.width - 16 * r) / 2, (from.height - 9 * r) / 2, 16 * r, 9 * r, 0, 0, 160, 90);
      return out.toDataURL('image/jpeg', 0.7);
    } catch {
      return null;
    }
  }
  function recall(snap: VisualsSnapshot) {
    const settings: Record<string, unknown> = { ...snap.settings };
    // One saved in 63: its Fluid on the finer scale since.
    if ((settings.v ?? 1) === 1 && typeof settings.fluid === 'number') settings.fluid = fluidFrom63(settings.fluid);
    delete settings.v;
    // This device's own sliders, on this device only.
    applyDevice(settings);
    // Its routes whole: any route it does not have (one set since, or a default added since) off.
    if (settings.routes && typeof settings.routes === 'object') {
      const off = Object.fromEntries([...Object.keys(DEFAULTS.routes), ...Object.keys(s.routes)].map((k) => [k, 0]));
      const theirs = settings.routes as Record<string, number>;
      // One saved before the Track row: a new track brought its own look then.
      const track = routeKey('track', null, 'drift');
      settings.routes = { ...off, ...(track in theirs ? {} : { [track]: -1 }), ...theirs };
    }
    drawingNow = null;
    nextBlend = ANOTHER_MS;
    lookCame = 'track';
    if (snap.id === STANDARD.id) {
      // Each track's own look again: nothing pinned, the playing track's drawn anew (and sent to the group, as after
      // the panel's Reset).
      applyShared({ ...settings, pins: [] });
      share(Object.fromEntries(SHARED_KEYS.filter((k) => !isLook(k)).map((k) => [k, $state.snapshot(s[k])])));
      resets++;
    } else {
      applyShared({ ...settings, pins: [...PRESET_KEYS] });
      share(sharedNow());
    }
    showChrome();
  }
    // The settings' rows: the drawing's cost first (ceilings: the device's own pace may keep it lower), then how the show
  // behaves, then the look (the preset: pinned where moved).
  type One = Exclude<keyof VisualSettings, 'lighting' | 'evolution' | 'continuity' | 'routes' | 'pins'>;
  const MIRROR = ['Off', 'On'];
  const FAMILIES = ['Spectrum', 'Arc', 'Two tones', 'Three'];
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
  const SHOW: Row[] = [
    { key: 'intensity', label: 'Intensity', step: 0.05, text: percent },
    { key: 'flash', label: 'Flash', step: 0.05, text: percent },
    { key: 'speed', label: 'Speed', step: 0.05, text: percent },
    { key: 'brightness', label: 'Brightness', step: 0.05, text: percent },
    { key: 'colours', label: 'Colours', step: 1, text: (v) => COLOURS[v] ?? 'Rainbow' },
    { key: 'smoothing', label: 'Smoothing', step: 0.05, text: percent },
    { key: 'floor', label: 'Floor', step: 0.05, text: percent },
    { key: 'depth', label: 'Depth', step: 0.05, text: percent },
    { key: 'fluid', label: 'Fluid', step: 0.05, text: percent },
  ];
  const LOOK: Row[] = [
    { key: 'form', label: 'Form', step: 1, text: (v) => `${v}` },
    { key: 'detail', label: 'Detail', step: 0.01, text: percent },
    { key: 'twist', label: 'Twist', step: 0.01, text: percent },
    { key: 'warp', label: 'Warp', step: 0.01, text: percent },
    { key: 'symmetry', label: 'Symmetry', step: 1, text: (v) => `${v}` },
    { key: 'mirror', label: 'Mirror', step: 1, text: (v) => MIRROR[v] ?? 'Off' },
    { key: 'final', label: 'Lens', step: 1, text: (v) => (v ? (LENSES[v - 1] ?? '') : 'Off') },
    { key: 'spread', label: 'Spread', step: 0.05, text: percent },
    { key: 'family', label: 'Family', step: 1, text: (v) => FAMILIES[v] ?? '' },
    { key: 'hue', label: 'Hue', step: 0.01, text: (v) => `${Math.round(v * 360)}°` },
    { key: 'lightness', label: 'Lightness', step: 0.01, text: percent },
    { key: 'vividness', label: 'Vividness', step: 0.01, text: percent },
    { key: 'purity', label: 'Purity', step: 0.05, text: percent },
  ];
  const isLook = (key: string) => (PRESET_KEYS as readonly string[]).includes(key);
  const lookNow = () => Object.fromEntries(PRESET_KEYS.map((k) => [k, s[k]]));
  // A look's setting let go of: back to the preset's value.
  function unpin(key: One) {
    s.pins = s.pins.filter((k) => k !== key);
    if (current && isLook(key)) (s as Record<string, unknown>)[key] = current[key as keyof Preset];
    saveVisualSettings();
    share({ pins: [...s.pins], [key]: s[key] });
    showChrome();
  }
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
    share({ [key]: [...s[key]] });
    showChrome();
  }
  function setContinuity(band: number, v: number) {
    s.continuity[band] = v;
    saveVisualSettings();
    share({ continuity: [...s.continuity] });
    showChrome();
  }
  const devicePixels = () => innerWidth * innerHeight * Math.min(devicePixelRatio || 1, 2) ** 2;
  const valueOf = (key: One) => (key === 'resolution' ? (s.resolution ?? autoResolution(devicePixels())) : s[key]) as number;
  function set(key: One, v: number) {
    (s as Record<One, number | null>)[key] = v;
    // The look's settings moved are pinned: a track's or the dice's preset leaves them as they are.
    if (isLook(key) && !s.pins.includes(key)) s.pins = [...s.pins, key];
    saveVisualSettings();
    // To the sync group: all but what is this device's own (its cost, its brightness).
    if ((SHARED_KEYS as readonly string[]).includes(key)) share({ [key]: v, ...(isLook(key) ? { pins: [...s.pins] } : {}) });
    showChrome();
  }
  // Full screen, where the browser can and the visuals are not already (the app's view may be): a button for it.
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function';
  let fullscreen = $state(typeof document !== 'undefined' && !!document.fullscreenElement);
  function goFullscreen() {
    document.documentElement.requestFullscreen({ navigationUI: 'hide' }).catch(() => {});
    if (docked) setVisualsSide(false);
    showChrome();
  }
  // Beside the page, where the player goes (it moves there too), or over it again; as the full player's own buttons.
  function beside() {
    prefs.sidebar = true;
    player.min = false;
    player.full = false;
    sidePanel.playerTop = false;
    setVisualsSide(true);
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
    const changed: Record<string, number> = {};
    for (const k of keysOf(signal, aspect)) {
      // A 0 is kept only where it turns a default off (to the group, a 0 either way).
      if (v === 0 && !DEFAULTS.routes[k]) delete s.routes[k];
      else s.routes[k] = v;
      changed[k] = v;
    }
    saveVisualSettings();
    share({ routes: changed });
  }
  // The routes back to the defaults; to the group, each route set before as nothing.
  function routesReset() {
    const before = Object.keys(s.routes);
    resetRoutes();
    share({ routes: { ...Object.fromEntries(before.map((k) => [k, 0])), ...$state.snapshot(s.routes) } });
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
  // The pace this device settled at (`cratarr-visuals-pace`): its own, not shared, not a setting.
  type Pace = { points: number; passes: number; fit: number; ceiling: { cost: number; until: number } | null };
  const PACE = 'cratarr-visuals-pace';
  function keptPace(): Pace | null {
    try {
      const p = JSON.parse(localStorage.getItem(PACE) ?? 'null');
      return p && typeof p.points === 'number' && typeof p.passes === 'number' && typeof p.fit === 'number' ? p : null;
    } catch {
      return null;
    }
  }
  function keepPace(p: Pace) {
    try {
      localStorage.setItem(PACE, JSON.stringify(p));
    } catch {
      /* this visit only */
    }
  }
  // The fewest points the device's own pace steps down to.
  const MIN_POINTS = 1 << 14;
  // A new track's flame blends in over this long; one asked for with the dice, quicker.
  const BLEND_MS = 6000;
  const ANOTHER_MS = 1500;

  onMount(() => {
    showChrome();
    // Moved beside the page or over it, the visuals are the same step of the history.
    if (!history.state?.visuals) history.pushState({ ...history.state, visuals: true }, '');
    const popped = () => {
      if (!closed && !history.state?.visuals) {
        closed = true;
        onclose();
      }
    };
    addEventListener('popstate', popped);
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && close();
    addEventListener('keydown', onKey);
    if (!docked) document.documentElement.requestFullscreen?.({ navigationUI: 'hide' }).catch(() => {});

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
    const fullscreenChanged = () => (fullscreen = !!document.fullscreenElement);
    document.addEventListener('fullscreenchange', fullscreenChanged);
    // The sync group this device plays in, if any: its visuals followed, this one's changes sent to it.
    const stopGroup = followGroup();

    // Where this device's pace settled last time (points, steps, resolution's share; and a level it could not hold
    // lately, not tried again until then): the visuals open there, not at the settings' most, and do not hunt for it
    // again (2026-10-18: a tablet opening at the most stepped down and up five times in eleven seconds, every step
    // starting the picture over).
    const pace = keptPace();
    let renderer: FlameRenderer | null = null;
    try {
      renderer = new FlameRenderer(canvas, Math.min(Math.round(2 ** s.points), pace?.points ?? Infinity));
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
    let resetsSeen = 0;
    // The flame the look's settings make (Spread and Purity aside: `evolved` takes them as it draws), and the settings
    // and colours it was made from. A change blends in: as `nextBlend` asks after a preset, else over a second and a
    // half; one of the settings that move it by degrees (Detail, Twist, Warp, Hue, Lightness, Vividness) dragged shows at
    // once.
    let base: Flame | null = null;
    let made: Record<string, number> | null = null;
    let madeColours = '';
    const SHAPING = PRESET_KEYS.filter((k) => k !== 'spread' && k !== 'purity');
    const BY_DEGREES = new Set<string>(['detail', 'twist', 'warp', 'hue', 'lightness', 'vividness']);
    // The Colours setting's Cover: the playing track's cover's colours, asked for as it starts (and the next one's, so
    // they are in as it starts); null until they are, and where it has no cover. Fire: how much of its heat shows,
    // followed over a second and a half.
    let cover: { key: string; colours: PaletteColour[] | null } | null = null;
    let fire = 0;
    // The next track's preset, drawn ahead at idle time (with the pins as they are then), so its start waits for none
    // of it; and how to stop the idle work drawing it.
    let ahead: { key: string; pins: string; draws: Generator<void, Drawn>; drawn: Drawn | null } | null = null;
    let stopIdle: (() => void) | null = null;
    const drawAhead = (deadline?: IdleDeadline) => {
      stopIdle = null;
      if (!ahead || ahead.drawn) return;
      const until = performance.now() + Math.min(8, deadline ? deadline.timeRemaining() : 8);
      let next = ahead.draws.next();
      while (!next.done && performance.now() < until) next = ahead.draws.next();
      if (next.done) ahead.drawn = next.value;
      else idleSoon();
    };
    const idleSoon = () => {
      if (stopIdle) return;
      if ('requestIdleCallback' in window) {
        const id = requestIdleCallback(drawAhead, { timeout: 2000 });
        stopIdle = () => cancelIdleCallback(id);
      } else {
        const id = setTimeout(drawAhead, 100);
        stopIdle = () => clearTimeout(id);
      }
    };
    // The mapping from the music and the settings to what is drawn (aspects.ts), and the moments' gap.
    const aspects = newAspects();
    // What the visuals do, measured (meters.ts), and when the rating page last had them.
    const metering = new Meters();
    let meteredAt = 0;
    // The look's walk (drift.ts): when it last walked, and the time since. A track's own look goes into the settings
    // as far as the Track row's route into Drift says: at -100 % all of it (as before Drift), at -50 % half way, at 0
    // none, above 0 away from it. After the panel's Reset (`whole`), all of it.
    const drifting = newDrift();
    let drifted = 0;
    let driftDt = 0;
    const trackLook = (drawn: Drawn, blend: number, whole: boolean) => {
      nextBlend = blend;
      lookCame = 'track';
      if (whole) {
        loadPreset(drawn.preset);
        return;
      }
      const how = trial.settings ?? s;
      const out = stepDrift(drifting, {
        amount: 0,
        trackEvent: how.routes[routeKey('track', null, 'drift')] ?? 0,
        dt: 0,
        seed: seedOf(key ?? 'cratarr'),
        bar: 0,
        pins: s.pins,
        own: drawn.own,
        now: lookNow() as Preset,
      });
      Object.assign(s, out);
      saveVisualSettings();
    };
    // The fluid (decision 0047): where each band's part lies on the screen (measured four times a second while the light
    // flows), and which way each part turns (its pushes curl with it).
    let partsWhere: ([number, number] | null)[] = [];
    let partsMeasured = 0;
    let metered = 0;
    let passes = Math.min(s.steps, pace?.passes ?? s.steps);
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
    let fit = Math.min(1, Math.max(0.6, pace?.fit ?? 1));
    // Seconds in a row with no late frame needed before a step back up; doubled each time a step up was soon taken
    // back, so the load settles instead of swinging (back to 3 after a minute with nothing taken back).
    let calm = 0;
    let wait = 6;
    // The cost of a level (points times steps times pixels) that failed right after a step up to it, and until when
    // no step up reaches it again; the cost of the last step up's level.
    let ceiling = pace?.ceiling && pace.ceiling.until > Date.now() ? pace.ceiling : null;
    let upTo = 0;
    const costOf = (points: number, steps: number, share: number) => points * steps * share * share;
    // Frames not to judge the pace by: while new points settle, and the one a resize made.
    let settlingFrames = 0;
    let steppedUp = 0;
    let steppedDown = 0;
    // The framing and exposure, measured from the flame twice a second (a quarter of the measure each frame, so no one
    // frame waits for all of it) and followed over a second and a half.
    let frame: { center: [number, number]; size: number; coverage: number } | null = null;
    let measured = 0;
    let measure: ReturnType<typeof measuring> | null = null;
    let counted = { since: performance.now(), frames: 0 };


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

      // How it behaves: as the settings say, or as the rating page's trial does (the look stays the settings').
      const how = trial.settings ?? s;
      const m = musicNow(now, how.lighting);
      if (m.key !== key || resets !== resetsSeen) {
        // A track's preset into the look's settings not pinned (after the panel's Reset, the playing one's again): the
        // first shows at once, each next blends in. Drawn ahead where it was (the next track's, at idle time, with the
        // same pins), else drawn now.
        const blend = key === undefined ? 0 : m.key !== key ? BLEND_MS : ANOTHER_MS;
        // After a Reset, the track's preset goes to the group too.
        const toGroup = resets !== resetsSeen;
        key = m.key;
        resetsSeen = resets;
        showing(m.key);
        // In a sync group that already set a look for this track (a device further on sent it), that look; the track's
        // own is still drawn, for a pin let go of.
        const theirs = toGroup ? null : groupLook(m.key);
        if (theirs) {
          nextBlend = blend;
          lookCame = 'track';
          loadPreset({ ...lookNow(), ...theirs } as Preset);
        }
        const fixed = pinned();
        if (ahead?.drawn && ahead.key === m.key && ahead.pins === JSON.stringify(fixed)) {
          current = ahead.drawn.own;
          drawingNow = null;
          if (!theirs) {
            trackLook(ahead.drawn, blend, toGroup);
            if (toGroup) share(lookNow());
          }
        } else drawingNow = { draws: presetDraws(seedOf(m.key ?? 'cratarr'), fixed), blend, share: toGroup, load: !theirs, why: 'track' };
      }
      // A preset being drawn: as many draws as fit in six milliseconds of this frame, then into the settings.
      if (drawingNow) {
        const until = performance.now() + 6;
        let next = drawingNow.draws.next();
        while (!next.done && performance.now() < until) next = drawingNow.draws.next();
        if (next.done) {
          current = next.value.own;
          const { blend, share: toGroup, load, why } = drawingNow;
          drawingNow = null;
          if (load && why === 'track') trackLook(next.value, blend, toGroup);
          else if (load) {
            nextBlend = blend;
            lookCame = why;
            loadPreset(next.value.preset);
          }
          if (load && toGroup) share(lookNow());
        }
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
      // The next track's preset drawn ahead, anew when the next track or the pins change.
      const upcoming = playingNow?.upcoming[0] ? `${playingNow.source}/${playingNow.upcoming[0].id}` : null;
      if (upcoming) {
        const fixed = pinned();
        const pins = JSON.stringify(fixed);
        if (ahead?.key !== upcoming || ahead.pins !== pins) {
          ahead = { key: upcoming, pins, draws: presetDraws(seedOf(upcoming), fixed), drawn: null };
          idleSoon();
        }
      }
      const covered = mode === 'cover' && cover?.key === m.key ? cover.colours : null;
      fire += ((mode === 'fire' ? 1 : 0) - fire) * Math.min(1, dt / 1.5);
      // The flame as the look's settings make it, coloured as the Colours setting asks. After a preset, it blends in as
      // asked; a choice (Form, Symmetry, Mirror, Lens, Family) or another colour mode over a second and a half; a
      // setting dragged by degrees at once; the cover's colours coming in while a flame blends in join that blend.
      const colours = `${mode}/${covered ? m.key : ''}`;
      const changed = !made ? SHAPING : SHAPING.filter((k) => made![k] !== s[k]);
      if (changed.length || colours !== madeColours) {
        // Unframed: the framing below measures what is drawn anyway.
        if (changed.length) base = flameOf(Object.fromEntries(PRESET_KEYS.map((k) => [k, s[k]])) as Preset, false);
        const coverCame = !changed.length && madeColours.startsWith('cover/') && colours.startsWith('cover/') && from;
        const blend = nextBlend ?? (changed.every((k) => BY_DEGREES.has(k)) && colours === madeColours ? 0 : coverCame ? null : ANOTHER_MS);
        if (blend && shown) {
          from = shown;
          blendStart = now;
          blendMs = blend;
        }
        made = Object.fromEntries(SHAPING.map((k) => [k, s[k]]));
        madeColours = colours;
        to = coloured(base!, mode, covered);
        if (!from) shown = to;
      }
      // A preset that changed nothing (all of it pinned) leaves nothing to blend.
      nextBlend = null;
      // What is drawn, as the music and the settings give it (aspects.ts). Reduced motion: a quarter of the turning, all
      // of it steady, half the beats' and hits' share, no moments.
      const still = reducedMotion();
      const d = stepAspects(aspects, m, how, dt, { still, now, matrix: trial.settings ? undefined : matrix, fluidFull: FLUID_FULL });
      const { turn, hits, parts, track } = d;
      if (panel && now - metered > 100) {
        metered = now;
        meters = { track: Array.from(track), bands: d.bands.map((v) => Array.from(v)) };
      }
      // Drift: the look walking with the music (drift.ts), each bar another way, never far from where the track set it;
      // ten steps a second (each makes the flame anew), none with reduced motion. Not saved, nor sent to the group:
      // every device playing the track together walks alike.
      driftDt += dt;
      if (now - drifted >= 100) {
        drifted = now;
        if (!still) {
          const bar = Math.floor((m.beat ? m.beat.index : m.position * 2) / 4);
          Object.assign(s, stepDrift(drifting, { amount: d.drift, trackEvent: 0, dt: driftDt, seed: seedOf(m.key ?? 'cratarr'), bar, pins: s.pins, own: current ?? (lookNow() as Preset), now: lookNow() as Preset }));
        }
        driftDt = 0;
      }
      // The moments: another symmetry; a pinned setting stays as it is. Seeded from the track and the beat it comes on
      // (else the half second), so every device playing it together makes the same choice.
      const at = `${m.key}|${m.beat ? m.beat.index : Math.floor(m.position * 2)}`;
      if (d.moments.symmetry && !(s.pins.includes('symmetry') && s.pins.includes('mirror'))) {
        const rnd = randomFrom(seedOf(`symmetry|${at}`));
        if (!s.pins.includes('symmetry')) s.symmetry = ((s.symmetry + Math.floor(rnd() * 7)) % 8) + 1;
        if (!s.pins.includes('mirror')) s.mirror = rnd() < 0.5 ? 1 : 0;
        nextBlend = ANOTHER_MS;
        saveVisualSettings();
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
      const scaleNow = d.scale;

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
      const bw = Math.max(1, Math.round(w * scale));
      const bh = Math.max(1, Math.round(h * scale));
      const resized = renderer.width !== bw || renderer.height !== bh;
      renderer.resize(bw, bh);
      // The fluid: each band's part pushed where it lies by its lights (its hits, by default), outward from the middle
      // and round the way the part turns; a beat pushes out from the middle; the loudness keeps the flow going, the
      // swells add whirls. Where the picture is in the buffer: as the renderer's camera puts it.
      const fluidNow = d.fluid;
      const forces: Force[] = [];
      if (fluidNow > 0) {
        if (now - partsMeasured > 250) {
          partsMeasured = now;
          const cam = frame ?? drawnFlame;
          const aspect = w / h;
          const cos = Math.cos(turn);
          const sin = Math.sin(turn);
          partsWhere = partsAt(drawnFlame).map((p) => {
            if (!p) return null;
            let x = ((p[0] - cam.center[0]) * scaleNow) / cam.size;
            let y = ((p[1] - cam.center[1]) * scaleNow) / cam.size;
            [x, y] = [cos * x - sin * y, sin * x + cos * y];
            if (aspect > 1) x /= aspect;
            else y *= aspect;
            return [x * 0.5 + 0.5, y * 0.5 + 0.5];
          });
        }
        for (let i = 0; i < PARTS; i++) {
          const at = partsWhere[i];
          const light = Math.max(0, hits[i] ?? 0);
          if (!at || light < 0.02) continue;
          const out = Math.hypot(at[0] - 0.5, at[1] - 0.5) || 1;
          const ox = (at[0] - 0.5) / out;
          const oy = (at[1] - 0.5) / out;
          const round = 0.6 * Math.sign(d.rates[i]);
          forces.push({ x: at[0], y: at[1], dx: 0.5 * (ox - oy * round), dy: 0.5 * (oy + ox * round), radius: 0.12, strength: light * fluidNow });
        }
        const beat = still ? 0 : track[T.beat];
        if (beat > 0.05)
          for (let k = 0; k < 4; k++) {
            const a = turn + (k * Math.PI) / 2;
            forces.push({ x: 0.5 + 0.12 * Math.cos(a), y: 0.5 + 0.12 * Math.sin(a), dx: 0.4 * Math.cos(a), dy: 0.4 * Math.sin(a), radius: 0.1, strength: beat * fluidNow });
          }
      }
      renderer.frame(
        passes,
        {
          brightness: 1,
          keep: d.keep,
          gamma: d.gamma,
          vibrancy: 1,
          turn,
          scale: scaleNow,
          zoom: 1,
          flash: d.flash,
          hits,
          ceiling: s.brightness,
          saturation: d.saturation,
          // The whole picture as bright however much of its frame the flame covers: a flame spread over most of it is
          // shown dimmer, one gathered into a few cells brighter (against one covering a third of its frame), so its
          // shape's changes are not flares.
          exposure: frame ? Math.min(1.5, Math.max(0.4, 0.33 / Math.max(0.05, frame.coverage))) : 1,
          smoothing: d.smoothing,
          floor: d.floor,
          fire,
          fluid: fluidNow,
          forces,
          flow: { energy: d.lit, swirl: 1 + 2 * track[T.swell] },
          // At the Speed setting's pace, as the turning (a quarter with reduced motion; none at 0). The fluid takes at most
          // 0.05 s a step: at 30 frames a second, Speed past 150 % flows no faster.
          dt: dt * how.speed * (still ? 0.25 : 1),
          warp: d.warp,
        },
        w,
        h,
      );
      // What this frame did, measured (meters.ts): the screen read into tiles (back a frame or two later), the parts'
      // motion and change, a new look and why. The meters' values for the rating page four times a second.
      renderer.measure(w, h, now / 1000);
      const stats: FrameStats = {
        t: now / 1000,
        turnRates: d.rates,
        shape: d.shape,
        hue: d.parts.map((p) => p.hue),
        spread: d.parts.map((p) => p.spread ?? 1),
        scale: d.scale,
        saturation: d.saturation,
        look: lookCame,
        symmetry: d.moments.symmetry,
        tiles: renderer.tiles(),
      };
      lookCame = null;
      metering.add(stats);
      if (now - meteredAt >= 250) {
        meteredAt = now;
        trial.meters = metering.now();
      }
      // A picture for a snapshot, while this frame is still in the canvas.
      if (pictureWanted) {
        const done = pictureWanted;
        pictureWanted = null;
        done(pictureOf(canvas));
      }

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
      // Not judged: frames while new points settle, or just after a resize (each costs more than the pace it leads to).
      if (renderer.settling || resized) settlingFrames = 2;
      else if (settlingFrames > 0) settlingFrames--;
      else {
        drawn++;
        if (spent > frameMs * 1.08 + 2) late++;
        worst = Math.max(worst, spent);
      }
      if (now - judged >= 1000) {
        let stepped = false;
        if (drawn >= 5 && late * 5 > drawn) {
          calm = 0;
          // A step up that failed at once: its level is the ceiling for five minutes.
          if (now - steppedUp < 5000 && upTo) {
            wait = Math.min(60, wait * 2);
            ceiling = { cost: upTo, until: Date.now() + 5 * 60_000 };
          }
          upTo = 0;
          steppedDown = now;
          // One level a second at most, however late (two at once overshot, and the step back up failed again).
          if (passes > 1) passes--;
          else if (fit > 0.6) fit = Math.max(0.6, fit * 0.85);
          else if (renderer.pointCount > MIN_POINTS) renderer.setPoints(Math.max(MIN_POINTS, renderer.pointCount >> 1));
          stepped = true;
        } else if (drawn >= 5 && late === 0 && ++calm >= wait) {
          calm = 0;
          const points = renderer.pointCount;
          const next =
            points < most
              ? { points: Math.min(most, points * 2), passes, fit }
              : fit < 1
                ? { points, passes, fit: Math.min(1, fit / 0.85) }
                : passes < s.steps
                  ? { points, passes: passes + 1, fit }
                  : null;
          if (ceiling && ceiling.until < Date.now()) ceiling = null;
          const cost = next ? costOf(next.points, next.passes, next.fit) : 0;
          if (next && (!ceiling || cost < ceiling.cost)) {
            steppedUp = now;
            upTo = cost;
            if (next.points !== points) renderer.setPoints(next.points);
            fit = next.fit;
            passes = next.passes;
            stepped = true;
          }
        }
        if (stepped) keepPace({ points: renderer.pointCount, passes, fit, ceiling });
        if (now - steppedDown > 60_000) wait = 6;
        late = 0;
        drawn = 0;
        worst = 0;
        judged = now;
      }
    };
    raf = requestAnimationFrame(loop);

    return () => {
      // Closed some other way: its step of the history goes too. Moved beside the page or over it (still open), the
      // step stays for the visuals where they went: going back here would close them there (2026-10-18: the docked
      // visuals' Full screen left the page full screen with the player, the visuals closed).
      if (!closed && !visuals.open && history.state?.visuals) history.back();
      cancelAnimationFrame(raf);
      clearTimeout(hideTimer);
      removeEventListener('popstate', popped);
      removeEventListener('keydown', onKey);
      document.removeEventListener('visibilitychange', wake);
      document.removeEventListener('fullscreenchange', fullscreenChanged);
      lock?.release().catch(() => {});
      // The full screen the visuals over the page went to ends with them; beside it, they made none.
      if (!docked && document.fullscreenElement) document.exitFullscreen().catch(() => {});
      renderer?.dispose();
      stopGroup();
      stopIdle?.();
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
  class:docked
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
        <button class="round" aria-label="Another look" use:held={'another'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={rollAnother}
          ><Icon name="visuals" size={24} /></button
        >
        <button
          class="round"
          aria-label="Snapshots"
          aria-haspopup="dialog"
          use:held={'snapshots'}
          onpointerdown={(e) => e.stopPropagation()}
          onpointerup={(e) => e.stopPropagation()}
          onclick={() => {
            snapshotsOpen = true;
            showChrome();
          }}><Icon name="bookmark" size={24} /></button
        >
        {#if docked}
          <button class="round" aria-label="Over the page" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => setVisualsSide(false)}
            ><Icon name="expand" size={22} /></button
          >
        {:else if player.roomy}
          <button class="round" aria-label="Beside the page" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={beside}
            ><Icon name="sidebar" size={24} /></button
          >
        {/if}
        {#if canFullscreen && !fullscreen}
          <button class="round" aria-label="Full screen" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={goFullscreen}
            ><Icon name="fullscreen" size={24} /></button
          >
        {/if}
      </div>
      {#if panel}
        <div class="panel" role="group" aria-label="Settings" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()}>
          {#each [COMPUTE, SHOW, LOOK] as rows, g (g)}
            <div class="rows" class:look={g > 0}>
              {#if g === 2}<h3 use:held={'look'}>Look</h3>{/if}
              {#each rows as row (row.key)}
                <label class="row" class:pinnable={g === 2}>
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
                  {#if g === 2}
                    {#if s.pins.includes(row.key)}
                      <button
                        class="pin"
                        aria-label="Unpin {row.label}"
                        use:held={'pin'}
                        onclick={(e) => {
                          e.preventDefault();
                          unpin(row.key);
                        }}><Icon name="pin" size={16} /></button
                      >
                    {:else}
                      <span></span>
                    {/if}
                  {/if}
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
                  routesReset();
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
                  <button class="chip" class:on={pick === b} role="radio" aria-checked={pick === b} use:held={`band:${b ?? 'all'}`} onclick={() => (pick = b)}
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
              <h3 use:held={`band:${b}`}>{band}</h3>
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
                resets++;
                // To the group: all but the look now (the track's preset follows once drawn), every pin let go.
                share(Object.fromEntries(SHARED_KEYS.filter((k) => !isLook(k)).map((k) => [k, $state.snapshot(s[k])])));
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
        <div class="transport">
          <button class="round" aria-label="Previous track" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('previous')}
            ><Icon name="previous" size={26} /></button
          >
          <button class="round" aria-label="Next track" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('next')}
            ><Icon name="next" size={26} /></button
          >
        </div>
      {/if}
    </div>
  {/if}
  {#if snapshotsOpen}
    <SnapshotsSheet onclose={() => (snapshotsOpen = false)} capture={capturePicture} trackName={np?.name ?? null} {recall} over={!docked} />
  {/if}
  <Help />
  {#if rating}<RateMode />{/if}
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

  /* Beside the page: in the panel at the right, under the page's own layers. */
  .visuals.docked {
    position: absolute;
    z-index: 1;
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

  /* The look's rows: their values may be names (a lens, a family), and a pin where moved. */
  .row.pinnable {
    grid-template-columns: 5.4em 1fr 6em 1.4em;
    gap: 8px;
  }

  .pin {
    display: grid;
    place-items: center;
    width: 1.4em;
    height: 28px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
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
    right: 136px;
    bottom: max(24px, env(safe-area-inset-bottom));
    display: grid;
    gap: 2px;
    color: #fff;
    text-shadow: 0 1px 6px rgb(0 0 0 / 0.6);
  }

  .transport {
    position: absolute;
    right: max(16px, env(safe-area-inset-right));
    bottom: max(16px, env(safe-area-inset-bottom));
    display: flex;
    gap: 8px;
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
