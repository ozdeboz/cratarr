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
  import { coverUrl, deleteVisualsSnapshot, getVisualsSnapshots, renameVisualsSnapshot, saveVisualsSnapshot, type VisualsSnapshot } from '../api';
  import Icon from '../Icon.svelte';
  import { reducedMotion } from '../motion';
  import { toast } from '../toast.svelte';
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
  import { longPress } from './longPress';
  import { coverPalette, type PaletteColour } from './palette';
  import { setVisualsSide } from './shown.svelte';
  import { applyShared, followGroup, groupLook, share, SHARED_KEYS, SHARED_V, sharedNow, showing, STANDARD } from './shared.svelte';
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
  let drawingNow: { draws: Generator<void, Drawn>; blend: number; share: boolean; load: boolean } | null = null;
  function loadAnother(seed: number, toGroup = false) {
    drawingNow = { draws: presetDraws(seed, pinned()), blend: ANOTHER_MS, share: toGroup, load: true };
  }
  // The dice: a random preset into the look's settings not pinned.
  function rollAnother() {
    loadAnother(Math.floor(Math.random() * 0x100000000), true);
    showChrome();
  }

  // The saved looks (snapshots: on the server, every device's), newest first: read as the panel opens. Saving takes
  // everything shared in a sync group, named after the track playing; recalling one puts it back, every look setting
  // pinned (the next track leaves it), and to the group. One held is renamed or deleted.
  let snapshots = $state<VisualsSnapshot[]>([]);
  let editing = $state<string | null>(null);
  let editName = $state('');
  function readSnapshots() {
    getVisualsSnapshots().then(
      (list) => (snapshots = list),
      () => {},
    );
  }
  async function saveSnapshot() {
    showChrome();
    try {
      const made = await saveVisualsSnapshot(np?.name ?? 'Visuals', { ...sharedNow(), v: SHARED_V });
      snapshots = [made, ...snapshots.filter((x) => x.id !== made.id)];
    } catch (e) {
      toast.fail(e, 'Not saved');
    }
  }
  function recall(snap: VisualsSnapshot) {
    const settings: Record<string, unknown> = { ...snap.settings };
    delete settings.v;
    // Its routes whole: any route it does not have (one set since, or a default added since) off.
    if (settings.routes && typeof settings.routes === 'object') {
      const off = Object.fromEntries([...Object.keys(DEFAULTS.routes), ...Object.keys(s.routes)].map((k) => [k, 0]));
      settings.routes = { ...off, ...(settings.routes as Record<string, number>) };
    }
    drawingNow = null;
    nextBlend = ANOTHER_MS;
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
  function edit(snap: VisualsSnapshot) {
    editName = snap.name;
    editing = snap.id;
    showChrome();
  }
  async function rename(snap: VisualsSnapshot) {
    if (editing !== snap.id) return;
    editing = null;
    const name = editName.trim();
    if (!name || name === snap.name) return;
    try {
      const done = await renameVisualsSnapshot(snap.id, name);
      snapshots = snapshots.map((x) => (x.id === snap.id ? done : x));
    } catch (e) {
      toast.fail(e, 'Not renamed');
    }
  }
  async function remove(snap: VisualsSnapshot) {
    editing = null;
    try {
      await deleteVisualsSnapshot(snap.id);
      snapshots = snapshots.filter((x) => x.id !== snap.id);
    } catch (e) {
      toast.fail(e, 'Not deleted');
    }
  }
  const focusNow = (node: HTMLInputElement) => {
    node.focus();
    node.select();
  };

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
    if (panel) readSnapshots();
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
  // The fewest points the device's own pace steps down to.
  const MIN_POINTS = 1 << 14;
  // A new track's flame blends in over this long; one asked for with the dice, quicker.
  const BLEND_MS = 6000;
  const ANOTHER_MS = 1500;
  // A beat's turn of the transforms: once around in 32 beats.
  const STEP = (2 * Math.PI) / 32;

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
          loadPreset({ ...lookNow(), ...theirs } as Preset);
        }
        const fixed = pinned();
        if (ahead?.drawn && ahead.key === m.key && ahead.pins === JSON.stringify(fixed)) {
          current = ahead.drawn.own;
          drawingNow = null;
          if (!theirs) {
            nextBlend = blend;
            loadPreset(ahead.drawn.preset);
            if (toGroup) share(lookNow());
          }
        } else drawingNow = { draws: presetDraws(seedOf(m.key ?? 'cratarr'), fixed), blend, share: toGroup, load: !theirs };
      }
      // A preset being drawn: as many draws as fit in six milliseconds of this frame, then into the settings.
      if (drawingNow) {
        const until = performance.now() + 6;
        let next = drawingNow.draws.next();
        while (!next.done && performance.now() < until) next = drawingNow.draws.next();
        if (next.done) {
          current = next.value.own;
          const { blend, share: toGroup, load } = drawingNow;
          drawingNow = null;
          if (load) {
            nextBlend = blend;
            loadPreset(next.value.preset);
            if (toGroup) share(lookNow());
          }
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
      // The moments: another preset (as the dice give), another symmetry; a pinned setting stays as it is.
      // Seeded from the track and the beat they come on (else the half second), so every device playing it together
      // makes the same choice.
      const at = `${m.key}|${m.beat ? m.beat.index : Math.floor(m.position * 2)}`;
      if (!still && moment(moments.reseed, W[A.reseed], now)) loadAnother(seedOf(`reseed|${at}`));
      if (!still && moment(moments.symmetry, W[A.symmetry], now) && !(s.pins.includes('symmetry') && s.pins.includes('mirror'))) {
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
      document.removeEventListener('fullscreenchange', fullscreenChanged);
      lock?.release().catch(() => {});
      if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
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
        <button class="round" aria-label="Another look" use:held={'dice'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={rollAnother}
          ><Icon name="dice" size={26} /></button
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
          <div class="snapshots" role="group" aria-label="Snapshots">
            <h3 use:held={'snapshots'}>Snapshots</h3>
            <div class="chips">
              <button class="chip save" use:held={'snapshots:save'} onclick={saveSnapshot}><Icon name="plus" size={14} /> Save</button>
              <button class="chip" use:held={'snapshots:standard'} onclick={() => recall(STANDARD as unknown as VisualsSnapshot)}>{STANDARD.name}</button>
              {#each snapshots as snap (snap.id)}
                {#if editing === snap.id}
                  <span class="editing">
                    <input
                      class="name"
                      aria-label="Name"
                      maxlength="60"
                      bind:value={editName}
                      use:focusNow
                      onkeydown={(e) => {
                        if (e.key === 'Enter') rename(snap);
                        else if (e.key === 'Escape') editing = null;
                      }}
                      onblur={() => rename(snap)}
                    />
                    <button class="delete" aria-label="Delete {snap.name}" onpointerdown={(e) => e.preventDefault()} onclick={() => remove(snap)}
                      ><Icon name="trash" size={16} /></button
                    >
                  </span>
                {:else}
                  <button class="chip" use:longPress={() => edit(snap)} onclick={() => recall(snap)}>{snap.name}</button>
                {/if}
              {/each}
            </div>
          </div>
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

  .snapshots {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgb(255 255 255 / 0.12);
  }

  .snapshots h3 {
    margin: 2px 0 0;
    font-size: 0.8rem;
    font-weight: 600;
    color: rgb(255 255 255 / 0.6);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 8px 0 4px;
  }

  .chip.save {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .editing {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .editing .name {
    width: 11em;
    min-height: 30px;
    padding: 0 10px;
    border: 1px solid rgb(255 255 255 / 0.5);
    border-radius: 15px;
    background: rgb(255 255 255 / 0.08);
    color: #fff;
    font: inherit;
    font-size: 0.8rem;
  }

  .editing .delete {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 15px;
    background: rgb(232 112 143 / 0.25);
    color: #fff;
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
