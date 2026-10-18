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
  import { paused, playing, seekBy, send } from '../remote.svelte';
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
  import { newBehaviourWalk, newDrift, stepBehaviourWalk, stepDrift } from './drift';
  import { curveOf, endOf, heightAt, tableOf, valueOf as kept, walksOf, type Curve, type RouteValue } from './curves';
  import CurveEditor from './CurveEditor.svelte';
  import { densityOf, newHistory, remember } from './history';
  import { budgetOf, BUDGETED, holding, LEVERS, newGovernor, STANDING, stepGovernor, type Budgeted, type Levers } from './governor';
  import { drifted, fresh, limitsOf, thumbed, type Learned } from './learning';
  import { forgetVisualsLimits, getVisualsLimits, postVisualsThumb, putVisualsLimits } from '../api';
  import { toast } from '../toast.svelte';
  import type { MeterValues } from './meters';
  import type { TrialLoop } from './trial.svelte';
  import { beatsOf } from './beats';
  import { DEFAULT_LIMITS, prechecking, type Checked } from './precheck';
  import { recipeFor, settingsOf, STIRRING, type Recipe, type Scope } from './recipe';
  import { signalsOf } from './signals';
  import { Meters, type FrameStats } from './meters';
  import RateMode from './RateMode.svelte';
  import { rating, trial } from './trial.svelte';
  import { appInfo, inApp } from '../app.svelte';
  import { coverPalette, type PaletteColour } from './palette';
  import { setVisualsSide, visuals } from './shown.svelte';
  import { applyDevice, applyShared, followGroup, groupLook, share, SHARED_KEYS, sharedNow, showing, STANDARD } from './shared.svelte';
  import SnapshotsSheet from './SnapshotsSheet.svelte';
  import { held } from './held.svelte';
  import Help from './Help.svelte';
  import { newStirring, stir } from './stirring';
  import { FlameRenderer } from './render';
  import { DIVE_MOST, TUNNEL_MOST } from './zoom';
  import { effectOf, reportOf } from './effects';
  import {
    A,
    ASPECT_LABEL,
    ASPECTS,
    B,
    BAND_SIGNALS,
    COLUMN_SCALES,
    compile,
    heightOn,
    LEVEL_ASPECTS,
    routeKey,
    SCALE_KEYS,
    SIGNAL_LABEL,
    T,
    TRACK_SIGNALS,
    type Aspect,
    type Signal,
  } from './routes';
  import { ceilingOf } from './quality';
  import {
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
    BEHAVIOUR_PINS,
    BAND_PINS,
    bandPin,
    isPinnable,
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
    else if (Math.hypot(dx, dy) < 10) tapped(e);
  }

  // Taps, as video players take them (the owner, 2026-10-18): one shows or hides the controls, once it is clear no
  // second follows; two or more in a row on one side seek, on the right forward and on the left back, each as it
  // comes: two 10 s, three 30 s, and each one after 30 s more ("each subsequent tap after the 30 seconds should be 30
  // seconds per tap"). How far, on that side for a moment.
  const TAP_MS = 300;
  let taps: { n: number; side: 1 | -1; at: number; timer: ReturnType<typeof setTimeout> } | null = null;
  let sought = $state<{ side: 1 | -1; secs: number } | null>(null);
  let soughtTimer: ReturnType<typeof setTimeout> | undefined;
  function tapped(e: PointerEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const side = e.clientX - box.left > box.width / 2 ? 1 : -1;
    const now = performance.now();
    if (taps) clearTimeout(taps.timer);
    if (!(taps && taps.side === side && now - taps.at < TAP_MS)) {
      taps = { n: 1, side, at: now, timer: setTimeout(toggleChrome, TAP_MS) };
      return;
    }
    taps.n++;
    taps.at = now;
    taps.timer = setTimeout(() => (taps = null), TAP_MS);
    const step = taps.n === 2 ? 10 : taps.n === 3 ? 20 : 30;
    void seekBy(side * step);
    sought = { side, secs: (taps.n === 2 ? 0 : (sought?.secs ?? 0)) + step };
    clearTimeout(soughtTimer);
    soughtTimer = setTimeout(() => (sought = null), 900);
  }
  function toggleChrome() {
    taps = null;
    if (chrome) {
      chrome = false;
      panel = false;
    } else showChrome();
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
  // The rating page (RateMode.svelte): from the panel's Rate (or ?rate=1 in the address, taken out of it as it starts,
  // so neither going back nor opening the visuals again starts another), its trial drawn in place of the settings while
  // it runs; each start a new run of it (a series stopped is taken up where it was left). It goes on where the visuals
  // move (`rating.run` outlives this mount) and ends as they close. While it runs, the controls that would change the
  // look or how it behaves are away: only Close, where it shows, Full screen and the transport stay (a track change, a
  // pause or a seek the page itself waits out or starts the pair over for).
  if (typeof location !== 'undefined' && new URLSearchParams(location.search).has('rate')) {
    const url = new URL(location.href);
    url.searchParams.delete('rate');
    history.replaceState(history.state, '', url);
    rating.run++;
  }
  function startRating() {
    rating.run++;
    panel = false;
    chrome = false;
  }
  // Another look: a random preset into the look's settings not pinned.
  function rollAnother() {
    loadAnother(Math.floor(Math.random() * 0x100000000), true);
    showChrome();
  }

  // The governor (governor.ts), stepped with the meters four times a second in the loop; and the limits it holds the
  // picture within, as learned from the thumbs (learning.ts): kept per user on the server, drifted back by the time
  // since, applied at once.
  const governor = newGovernor();
  let learned = $state<Learned>(fresh(STANDING));
  let learnedVersion = 0;
  const limitsNow = $derived(limitsOf(learned));
  const status = (e: unknown) => (e && typeof e === 'object' && 'status' in e ? (e as { status?: number }).status : undefined);
  async function loadLearned() {
    try {
      const got = await getVisualsLimits();
      learnedVersion = got.version;
      learned = drifted({ ...fresh(STANDING), ...got.limits } as Learned, STANDING, (Date.now() - got.at) / 86_400_000);
    } catch (e) {
      // None learned yet (or not to be had now): where they start.
      if (status(e) === 404) {
        learnedVersion = 0;
        learned = fresh(STANDING);
      }
    }
  }
  // The last 20 seconds, four readings a second: the meters and what the loop knew beside them (for the thumbs).
  const WINDOW = 80;
  // `relative`: each budgeted meter against its budget at a limit of 1, as Wild and the music's activity had it then.
  let recent: { meters: MeterValues; loop: TrialLoop; relative: Record<Budgeted, number> }[] = [];
  /** The budget the governor holds the picture back by most now, if it holds any back (the flash rule is not one). */
  function heldMost(): Budgeted | null {
    let best: Budgeted | null = null;
    let far = 0;
    // How far each budget's lever holds back: 0 open, 1 as far as it goes.
    for (const [k, { lever, most }] of Object.entries(LEVERS) as [Budgeted, { lever: keyof Levers; most: number }][]) {
      if (!holding(governor.levers, lever)) continue;
      const v = governor.levers[lever];
      const by = most > 1 ? (v - 1) / (most - 1) : (1 - v) / (1 - most);
      if (by > far) {
        far = by;
        best = k;
      }
    }
    return best;
  }
  // Thumbs up and down on the visuals as they are now (the owner: "a general thumbs up or thumbs down for any
  // visualization that I currently look at, and it could then decide to tune"): learning.ts decides what, if anything,
  // to change; each is kept, with the 20 seconds before it, for the periodic fit.
  let thumbShown = $state<'up' | 'down' | null>(null);
  let thumbTimer: ReturnType<typeof setTimeout> | undefined;
  async function thumb(t: 'up' | 'down') {
    thumbShown = t;
    clearTimeout(thumbTimer);
    thumbTimer = setTimeout(() => (thumbShown = null), 600);
    showChrome();
    const list = recent.slice();
    const mean = (pick: (r: (typeof list)[number]) => number) => (list.length ? list.reduce((s, r) => s + pick(r), 0) / list.length : 0);
    const relative = Object.fromEntries(BUDGETED.map((k) => [k, mean((r) => r.relative[k])])) as Record<Budgeted, number>;
    const window = { relative: list.length ? relative : {} };
    const held = heldMost();
    let { learned: next, changed } = thumbed(learned, STANDING, t, window, held);
    if (changed) {
      learned = next;
      for (let tries = 0; tries < 2; tries++) {
        try {
          learnedVersion = (await putVisualsLimits({ base: learnedVersion, limits: learned, at: Date.now() })).version;
          break;
        } catch (e) {
          if (status(e) !== 409 || tries) {
            toast.fail(e, 'Not saved');
            break;
          }
          // Another device learned meanwhile: its limits, this thumb on them.
          await loadLearned();
          ({ learned: next, changed } = thumbed(learned, STANDING, t, window, held));
          if (!changed) break;
          learned = next;
        }
      }
    }
    const summary = <T,>(pick: (r: (typeof list)[number]) => T) => list.map(pick);
    const meterStats = Object.fromEntries(
      Object.keys(list[0]?.meters ?? {}).map((k) => {
        const v = summary((r) => Number(r.meters[k as keyof MeterValues]));
        return [k, { mean: v.reduce((a, b) => a + b, 0) / v.length, most: Math.max(...v) }];
      }),
    );
    const loopStats = Object.fromEntries(
      (['intensity', 'bps', 'hitsPerSecond', 'swing', 'points', 'steps', 'fps'] as const).map((k) => {
        const v = summary((r) => r.loop[k]);
        return [k, v.length ? { mean: v.reduce((a, b) => a + b, 0) / v.length, least: Math.min(...v) } : null];
      }),
    );
    const now = $state.snapshot(s) as VisualSettings;
    const playingNow = playing();
    postVisualsThumb({
      thumb: t,
      meters: meterStats,
      loop: { ...loopStats, device: list[list.length - 1]?.loop.device ?? null },
      settings: { ...Object.fromEntries(BEHAVIOUR.map((k) => [k, now[k]])), ...lookNow(), wild: now.wild },
      track: playingNow ? `${playingNow.source}/${playingNow.id}` : null,
      relative,
      held,
      changed,
      at: Date.now(),
    }).catch(() => {});
  }
  async function forget() {
    try {
      await forgetVisualsLimits();
      learnedVersion = 0;
      learned = fresh(STANDING);
    } catch (e) {
      toast.fail(e, 'Not forgotten');
    }
    showChrome();
  }

  // The dice: everything at random, a look and how it behaves (a recipe: recipe.ts), both to the group. Each recipe is
  // first run against the playing track from where it is (precheck.ts), a slice a frame: the first within the limits
  // (the governor's, as Calm ↔ Wild has them; three flashes a second whatever it says) is taken, after eight the one
  // over them by least; where the track's signals are not known yet the first, the governor holding it as it plays.
  // The undo puts back the look and behaviour from before (until the next roll).
  //
  // A double tap rolls how it moves only (the owner, 64.1: "double tap becomes this randomize"; `MOVEMENT` in
  // recipe.ts), its look and the rest as they are; and it may go "twice the limits": checked, and governed while it
  // stands, as at Wild's most (`loosened`), till the next roll, an undo (back to what stood before), Reset or a recall.
  // Three flashes a second over a quarter of the screen stay the rule. A single tap waits out the double's window (a
  // roll takes about as long anyway).
  const BEHAVIOUR = ['routes', 'flash', 'speed', 'intensity', 'fluid', 'smoothing', 'haze', 'tunnel', 'dive', ...SCALE_KEYS, 'lighting', 'evolution', 'continuity'] as const;
  const TRIES = 8;
  const DOUBLE_MS = 250;
  let rollWanted: Scope | null = null;
  let rolling: { run: Generator<void, Recipe>; scope: Scope } | null = null;
  let loosened = false;
  const wildNow = (wild: number) => (loosened ? Math.max(wild, 1) : wild);
  let undo = $state<{ look: Record<string, number>; pins: string[]; behaviour: Partial<VisualSettings>; loosened: boolean } | null>(null);
  let diceWait = 0;
  function diceTapped() {
    if (diceWait) {
      clearTimeout(diceWait);
      diceWait = 0;
      roll('movement');
    } else
      diceWait = window.setTimeout(() => {
        diceWait = 0;
        roll('all');
      }, DOUBLE_MS);
  }
  function roll(scope: Scope) {
    rollWanted = scope;
    showChrome();
  }
  function* rollingFor(base: VisualSettings, key: string | null, from: number, scope: Scope): Generator<void, Recipe> {
    const [source, id] = key ? [key.slice(0, key.indexOf('/')), key.slice(key.indexOf('/') + 1)] : ['', ''];
    const signals = key ? signalsOf(source, id) : null;
    const beats = key ? (beatsOf(source, id) ?? null) : null;
    const wild = 2 ** (2 * ((scope === 'movement' ? Math.max(base.wild, 1) : base.wild) - 0.5));
    const limits: Record<string, number> = { ...Object.fromEntries(Object.entries(limitsNow).map(([k, v]) => [k, v * wild])), flash: DEFAULT_LIMITS.flash };
    let best: { recipe: Recipe; by: number } | null = null;
    for (let k = 0; k < TRIES; k++) {
      const recipe = recipeFor(Math.floor(Math.random() * 0x100000000), ASPECTS);
      if (!signals) return recipe;
      const checked: Checked = yield* prechecking(recipe, base, signals, beats, from, limits, undefined, scope);
      if (checked.ok) return recipe;
      const by = checked.over.reduce((sum, m) => sum + (checked.worst[m as keyof typeof checked.worst] as number) / (limits[m] ?? 1), 0);
      if (!best || by < best.by) best = { recipe, by };
    }
    return best!.recipe;
  }
  function rolled(recipe: Recipe, scope: Scope) {
    const now = $state.snapshot(s) as VisualSettings;
    undo = { look: lookNow(), pins: [...now.pins], behaviour: Object.fromEntries(BEHAVIOUR.map((k) => [k, now[k]])) as Partial<VisualSettings>, loosened };
    const next = settingsOf(recipe, now, scope);
    for (const k of BEHAVIOUR) if (!s.pins.includes(k)) (s as Record<string, unknown>)[k] = next[k];
    loosened = scope === 'movement';
    saveVisualSettings();
    share(Object.fromEntries(BEHAVIOUR.map((k) => [k, $state.snapshot(s[k])])));
    if (scope === 'all') loadAnother(Math.floor(Math.random() * 0x100000000), true);
  }
  function undoRoll() {
    if (!undo) return;
    const was = undo;
    undo = null;
    drawingNow = null;
    rolling = null;
    nextBlend = ANOTHER_MS;
    lookCame = 'dice';
    Object.assign(s, was.behaviour, was.look, { pins: was.pins });
    loosened = was.loosened;
    saveVisualSettings();
    share({ ...Object.fromEntries(BEHAVIOUR.map((k) => [k, $state.snapshot(s[k])])), ...lookNow(), pins: [...s.pins] });
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
    loosened = false;
    const settings: Record<string, unknown> = { ...snap.settings };
    // One saved in 63: its Fluid on the finer scale since.
    if ((settings.v ?? 1) === 1 && typeof settings.fluid === 'number') settings.fluid = fluidFrom63(settings.fluid);
    // One saved before the columns' scales (64.2): all of each.
    for (const k of SCALE_KEYS) if (!(k in settings)) settings[k] = 1;
    // One saved before the zooms (65): neither.
    for (const k of ['tunnel', 'dive'] as const) if (!(k in settings)) settings[k] = 0;
    // One saved before the effects (65): none, and all of the Effects setting.
    for (const k of ['fx1', 'fx1Amount', 'fx2', 'fx2Amount', 'fx3', 'fx3Amount', 'fxLoop', 'fxLoopAmount'] as const) if (!(k in settings)) settings[k] = 0;
    if (!('effects' in settings)) settings.effects = 1;
    delete settings.v;
    // This device's own sliders, on this device only.
    applyDevice(settings);
    // Its routes whole: any route it does not have (one set since, or a default added since) off.
    if (settings.routes && typeof settings.routes === 'object') {
      const off = Object.fromEntries([...Object.keys(DEFAULTS.routes), ...Object.keys(s.routes)].map((k) => [k, 0]));
      const theirs = settings.routes as Record<string, number>;
      // One saved before the Track row, or before the Behaviour column (64.1): a new track brought its own look then,
      // and the behaviour stayed as set.
      const defaults: Record<string, number> = {};
      if (!(routeKey('track', null, 'look') in theirs) && !('track|-|drift' in theirs)) defaults[routeKey('track', null, 'look')] = -1;
      if (!(routeKey('track', null, 'behaviour') in theirs)) defaults[routeKey('track', null, 'behaviour')] = -1;
      // One saved before the fluid's routes (64.1): it stirred as their defaults do.
      const fluidRoute = (k: string) => /[|](stir|flow|swirl)$/.test(k);
      if (!Object.keys(theirs).some(fluidRoute)) for (const [k, v] of Object.entries(DEFAULTS.routes)) if (fluidRoute(k)) defaults[k] = v as number;
      settings.routes = { ...off, ...defaults, ...theirs };
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
      applyShared({ ...settings, pins: [...PRESET_KEYS, ...BEHAVIOUR_PINS, ...BAND_PINS] });
      share(sharedNow());
    }
    showChrome();
  }
    // The settings' rows: the drawing's cost first (ceilings: the device's own pace may keep it lower), then how the show
  // behaves, then the look (the preset: pinned where moved).
  type One = Exclude<keyof VisualSettings, 'lighting' | 'evolution' | 'continuity' | 'routes' | 'pins'>;
  const MIRROR = ['Off', 'On'];
  // The Colours (65: Family folded in, the owner: "YES Family into Colours"): the look's four families, which a track's
  // look or the dice draw, then the cover's and a fire's, which only a hand chooses (and so pins).
  const COLOURS = ['Spectrum', 'Arc', 'Two tones', 'Three', 'Cover', 'Fire'];
  const modeOf = (family: number): ColourMode => (family >= 5 ? 'fire' : family >= 4 ? 'cover' : 'rainbow');
  type Row = { key: One; label: string; step: number; text: (v: number) => string };
  // The settings the governor holds back now (governor.ts), marked by their label: Flash's swing, Speed's pace, and on
  // Wild whatever it holds back (the evolving, the hues, the size, the moments too).
  let governed = $state<Partial<Record<One, boolean>>>({});
  // Each behaviour slider's value in effect now (its walk's share of what is set, and for the four levels what their
  // routes let through of that), marked under the slider where it differs.
  let live = $state<Partial<Record<string, number>>>({});
  const liveAt = (key: One) => {
    const v = live[key];
    if (v === undefined || Math.abs(v - (valueOf(key) ?? 0)) < 0.005) return null;
    const [lo, hi] = RANGES[key];
    return Math.min(1, Math.max(0, (v - lo) / (hi - lo)));
  };
  const percent = (v: number) => `${Math.round(v * 100)}%`;
  const signedPercent = (v: number) => `${v > 0.004 ? '+' : v < -0.004 ? '−' : ''}${Math.abs(Math.round(v * 100))}%`;
  const thousands = (log2: number) => {
    const n = 2 ** log2;
    return n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)}M` : `${Math.round(n / 1024)}k`;
  };
  // The panel's groups (the owner, 2026-10-18: "We should get a header for the behavior section"): first the limits
  // (what the device can afford, how bright, how lively the governor lets it be), then how the visuals behave with the
  // music, then the look.
  const COMPUTE: Row[] = [
    { key: 'quality', label: 'Quality', step: 0.05, text: percent },
    { key: 'fps', label: 'FPS', step: 5, text: (v) => `${v}` },
    { key: 'brightness', label: 'Brightness', step: 0.05, text: percent },
    { key: 'wild', label: 'Wild', step: 0.05, text: percent },
  ];
  // Then each column of the routes, in the grid's order (64.2; the owner: "each column their own slider"): its scale,
  // or for the four levels the most their routes give.
  const column = (key: One, aspect: Aspect): Row => ({ key, label: ASPECT_LABEL[aspect], step: 0.05, text: key === 'haze' ? signedPercent : percent });
  const SHOW: Row[] = [
    { key: 'intensity', label: 'Intensity', step: 0.05, text: percent },
    { key: 'flash', label: 'Flash', step: 0.05, text: percent },
    { key: 'speed', label: 'Speed', step: 0.05, text: percent },
    ...ASPECTS.flatMap((a) => {
      const key = (COLUMN_SCALES as Partial<Record<Aspect, One>>)[a] ?? ((LEVEL_ASPECTS as readonly Aspect[]).includes(a) ? (a as One) : null);
      return key ? [column(key, a)] : [];
    }),
  ];
  // An effect setting's text: its effect's name, or none.
  const effectName = (v: number) => {
    const e = effectOf(Math.round(v));
    return e ? e.name[0].toUpperCase() + e.name.slice(1) : 'None';
  };
  // The look's effects for the renderer (render.ts `Shown.effects`), each amount times what the Effects setting's
  // routes let through; and what they add to the meters.
  const effectsNow = (how: VisualSettings, level: number, t: number) => ({
    ids: [how.fx1, how.fx2, how.fx3].map(Math.round),
    amounts: [how.fx1Amount, how.fx2Amount, how.fx3Amount].map((a) => a * level),
    loop: Math.round(how.fxLoop),
    loopAmount: how.fxLoopAmount * level,
    t,
  });
  const effectsReport = (how: VisualSettings, level: number) => {
    const fx = effectsNow(how, level, 0);
    const picture = reportOf(fx.ids, fx.amounts, 'picture');
    const loop = reportOf([fx.loop], [fx.loopAmount], 'loop');
    const out: Record<string, number> = { ...picture };
    for (const [k, v] of Object.entries(loop)) out[k] = (out[k] ?? 0) + (v ?? 0);
    return out;
  };
  const LOOK: Row[] = [
    { key: 'family', label: 'Colours', step: 1, text: (v) => COLOURS[v] ?? '' },
    { key: 'form', label: 'Form', step: 1, text: (v) => `${v}` },
    { key: 'detail', label: 'Detail', step: 0.01, text: percent },
    { key: 'twist', label: 'Twist', step: 0.01, text: percent },
    { key: 'warp', label: 'Warp', step: 0.01, text: percent },
    { key: 'symmetry', label: 'Symmetry', step: 1, text: (v) => `${v}` },
    { key: 'mirror', label: 'Mirror', step: 1, text: (v) => MIRROR[v] ?? 'Off' },
    { key: 'final', label: 'Lens', step: 1, text: (v) => (v ? (LENSES[v - 1] ?? '') : 'Off') },
    { key: 'spread', label: 'Spread', step: 0.05, text: percent },
    { key: 'hue', label: 'Hue', step: 0.01, text: (v) => `${Math.round(v * 360)}°` },
    { key: 'lightness', label: 'Lightness', step: 0.01, text: percent },
    { key: 'vividness', label: 'Vividness', step: 0.01, text: percent },
    { key: 'purity', label: 'Purity', step: 0.05, text: percent },
    // The look's effects (65): three on the picture, one on the loop, each with its amount.
    ...([1, 2, 3] as const).flatMap((n) => [
      { key: `fx${n}` as One, label: `Effect ${n}`, step: 1, text: effectName },
      { key: `fx${n}Amount` as One, label: `Amount ${n}`, step: 0.05, text: percent },
    ]),
    { key: 'fxLoop', label: 'Loop', step: 1, text: effectName },
    { key: 'fxLoopAmount', label: 'Loop amount', step: 0.05, text: percent },
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
  // A band's time moved is pinned, as the other sliders are: the dice leave it as it is.
  function pinBand(key: 'lighting' | 'evolution' | 'continuity', band: number) {
    const pin = bandPin(key, band);
    if (!s.pins.includes(pin)) s.pins = [...s.pins, pin];
  }
  function unpinBand(key: 'lighting' | 'evolution' | 'continuity', band: number) {
    const pin = bandPin(key, band);
    s.pins = s.pins.filter((k) => k !== pin);
    saveVisualSettings();
    share({ pins: [...s.pins] });
    showChrome();
  }
  function setTime(key: 'lighting' | 'evolution', band: number, log2: number) {
    s[key][band] = 2 ** log2;
    pinBand(key, band);
    saveVisualSettings();
    share({ [key]: [...s[key]], pins: [...s.pins] });
    showChrome();
  }
  function setContinuity(band: number, v: number) {
    s.continuity[band] = v;
    pinBand('continuity', band);
    saveVisualSettings();
    share({ continuity: [...s.continuity], pins: [...s.pins] });
    showChrome();
  }
  const devicePixels = () => innerWidth * innerHeight * Math.min(devicePixelRatio || 1, 2) ** 2;
  const valueOf = (key: One) => s[key] as number;
  function set(key: One, v: number) {
    (s as Record<One, number | null>)[key] = v;
    // The look's settings and the behaviour's sliders moved are pinned: a track's or the dice's preset, and Drift,
    // leave them as they are.
    if (isPinnable(key) && !s.pins.includes(key)) s.pins = [...s.pins, key];
    saveVisualSettings();
    // To the sync group: all but what is this device's own (its cost, its brightness).
    if ((SHARED_KEYS as readonly string[]).includes(key)) share({ [key]: v, ...(isPinnable(key) ? { pins: [...s.pins] } : {}) });
    showChrome();
  }
  // Full screen, where the browser can: a button for it, which leaves it again (64.2: it went away, and only the
  // browser's Esc or back left).
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function';
  let fullscreen = $state(typeof document !== 'undefined' && !!document.fullscreenElement);
  function goFullscreen() {
    document.documentElement.requestFullscreen({ navigationUI: 'hide' }).catch(() => {});
    if (docked) setVisualsSide(false);
    showChrome();
  }
  function leaveFullscreen() {
    document.exitFullscreen().catch(() => {});
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
  // The grid wider than the panel scrolls across on its own: the aspects' names over it follow it, and its visible
  // width keeps the bands' chips in sight.
  let gridHead = $state<HTMLDivElement>();
  let gridView = $state(0);
  let cell = $state<{ signal: Signal; aspect: Aspect } | null>(null);
  const isBandSignal = (signal: Signal) => (BAND_SIGNALS as readonly string[]).includes(signal);
  const keysOf = (signal: Signal, aspect: Aspect) =>
    !isBandSignal(signal)
      ? [routeKey(signal, null, aspect)]
      : pick !== null
        ? [routeKey(signal, pick, aspect)]
        : Array.from({ length: BANDS }, (_, b) => routeKey(signal, b, aspect));
  // Each walking route's walk now (`route:` and its key: its share of itself), for its cell, four times a second while
  // the panel shows.
  let routeWalkNow = $state<Record<string, number>>({});
  // A cell: its routes' end (the bands' mean), whether they differ, whether they are all pinned, the curve it shows
  // (the bands' mean, scaled to the cell: `pathOf`), whether its routes walk with the behaviour, and what they give now:
  // each its curve at where its signal is now, times its walk, through the loudness (as the routes act: the Loudness
  // row and the stirring's columns not), the bands' mean; -1 to 1.
  function cellOf(signal: Signal, aspect: Aspect) {
    const keys = keysOf(signal, aspect);
    const vs = keys.map((k) => s.routes[k] ?? 0);
    const ends = vs.map(endOf);
    const cs = vs.map(curveOf);
    const mean = (f: (c: Curve) => number) => cs.reduce((a, c) => a + f(c), 0) / cs.length;
    const shown = { h1: [mean((c) => c.h1[0]), mean((c) => c.h1[1])], h2: [mean((c) => c.h2[0]), mean((c) => c.h2[1])], h: mean((c) => c.h) } as Curve;
    const e = signal === 'loudness' || STIRRING.includes(aspect as (typeof STIRRING)[number]) ? 1 : meters.lit;
    const band = isBandSignal(signal);
    const now = keys.reduce((a, k, i) => {
      const b = band ? (pick ?? i) : null;
      const x = b === null ? (meters.track[T[signal as keyof typeof T]] ?? 0) : (meters.bands[b]?.[B[signal as keyof typeof B]] ?? 0);
      const y = typeof vs[i] === 'number' ? (vs[i] as number) * Math.min(1, Math.max(0, x)) : heightAt(tableFor(cs[i]), x);
      return a + y * (routeWalkNow[`route:${k}`] ?? 1) * e;
    }, 0) / keys.length;
    return {
      v: ends.reduce((a, b) => a + b, 0) / ends.length,
      mixed: ends.some((x) => Math.abs(x - ends[0]) > 1e-6),
      pinned: keys.every((k) => s.pins.includes(`route:${k}`)),
      path: pathOf(shown),
      walks: keys.some((_, i) => walksOf(vs[i])),
      now: Math.min(1, Math.max(-1, now)),
    };
  }
  // Each curve's table, kept by the curve (a table is 513 points).
  const tables = new Map<string, Float32Array>();
  function tableFor(c: Curve): Float32Array {
    const key = `${c.h1}|${c.h2}|${c.h}`;
    let t = tables.get(key);
    if (!t) {
      if (tables.size > 2000) tables.clear();
      t = tableOf(c).table;
      tables.set(key, t);
    }
    return t;
  }
  // A curve as the cell draws it (viewBox 0 0 32 24): left to right as the row rises, about the middle line, scaled so
  // its largest swing reaches the cell's edge (the amount is the cell's colour); none for a route that does nothing.
  // Kept by the curve, so a grid of them costs little each time it is drawn.
  const paths = new Map<string, string | null>();
  function pathOf(c: Curve): string | null {
    const key = `${c.h1}|${c.h2}|${c.h}`;
    const had = paths.get(key);
    if (had !== undefined) return had;
    const all = tableFor(c);
    const ys = Array.from({ length: 33 }, (_, i) => all[Math.round((i / 32) * (all.length - 1))]);
    const most = Math.max(...ys.map(Math.abs));
    const path =
      most < 1e-4
        ? null
        : ys.map((y, i) => `${i ? 'L' : 'M'}${((i / (ys.length - 1)) * 28 + 2).toFixed(2)},${(12 - (y / most) * 9).toFixed(2)}`).join('');
    if (paths.size > 2000) paths.clear();
    paths.set(key, path);
    return path;
  }
  // The picked cell's curve for the editor: its route's, or for all six bands the mean of their handles (dashed where
  // they differ); whether it walks (all of them), and its pins.
  function editedOf(signal: Signal, aspect: Aspect) {
    const keys = keysOf(signal, aspect);
    const cs = keys.map((k) => curveOf(s.routes[k] ?? 0));
    const mean = (f: (c: Curve) => number) => cs.reduce((a, c) => a + f(c), 0) / cs.length;
    const flat = (c: Curve) => [c.h1[0], c.h1[1], c.h2[0], c.h2[1], c.h];
    return {
      curve: { h1: [mean((c) => c.h1[0]), mean((c) => c.h1[1])], h2: [mean((c) => c.h2[0]), mean((c) => c.h2[1])], h: mean((c) => c.h) } as Curve,
      mixed: cs.some((c) => flat(c).some((v, i) => Math.abs(v - flat(cs[0])[i]) > 1e-6)),
      walks: cs.every((c) => !!c.walks),
      pinned: keys.every((k) => s.pins.includes(`route:${k}`)),
    };
  }
  const edited = $derived(cell ? editedOf(cell.signal, cell.aspect) : null);
  const editedTable = $derived(edited ? tableOf(edited.curve).table : null);
  // Where the picked cell's signal is now on its curve, and where it has been (four times a second while it shows):
  // each signal's last half minute, kept while the music plays (not named history: that is the browser's).
  const heard = newHistory(TRACK_SIGNALS.length, BAND_SIGNALS.length, BANDS);
  let density = $state<Float32Array | null>(null);
  const liveOnCurve = $derived.by(() => {
    if (!cell || !editedTable || !meters.track.length) return null;
    const x = Math.min(1, Math.max(0, meterOf(cell.signal)));
    return { x, y: heightAt(editedTable, x) };
  });
  // Each of a cell's routes made anew (`make`: from what it is, its curve), kept as a number where straight and not
  // walking; a 0 kept only where it turns a default off (to the group, a 0 either way). The routes edited are pinned:
  // the dice leave them as they are. Saved and sent to the group when the change is `done` (a drag let go).
  function remake(signal: Signal, aspect: Aspect, make: (c: Curve) => Curve, done: boolean) {
    const changed: Record<string, RouteValue> = {};
    const pins = new Set(s.pins);
    for (const k of keysOf(signal, aspect)) {
      const v = kept(make(curveOf(s.routes[k] ?? 0)));
      if (v === 0 && !DEFAULTS.routes[k]) delete s.routes[k];
      else s.routes[k] = v;
      changed[k] = v;
      pins.add(`route:${k}`);
    }
    if (pins.size !== s.pins.length) s.pins = [...pins];
    if (!done) return;
    saveVisualSettings();
    share({ routes: changed, pins: [...s.pins] });
    showChrome();
  }
  // A cell's curve: its shape, each route's walk as it was.
  const setCurve = (signal: Signal, aspect: Aspect, c: Curve, done: boolean) =>
    remake(signal, aspect, (was) => ({ h1: [...c.h1], h2: [...c.h2], h: c.h, ...(was.walks ? { walks: true } : {}) }), done);
  // Whether a cell's routes walk with the behaviour (each its own curve kept).
  const setWalks = (signal: Signal, aspect: Aspect, on: boolean) =>
    remake(signal, aspect, (c) => ({ h1: [...c.h1], h2: [...c.h2], h: c.h, ...(on ? { walks: true } : {}) }), true);
  // Pinning: a cell's routes, or a column's.
  function setPins(keys: string[], on: boolean) {
    s.pins = on ? [...new Set([...s.pins, ...keys])] : s.pins.filter((p) => !keys.includes(p));
    saveVisualSettings();
    share({ pins: [...s.pins] });
    showChrome();
  }
  // The routes back to the defaults; to the group, each route set before as nothing.
  function routesReset() {
    loosened = false;
    const before = Object.keys(s.routes);
    resetRoutes();
    share({ routes: { ...Object.fromEntries(before.map((k) => [k, 0])), ...$state.snapshot(s.routes) } });
  }
  const signed = (v: number) => `${v > 0 ? '+' : ''}${Math.round(v * 100)}%`;
  const cellLabel = (signal: Signal, aspect: Aspect) => `${SIGNAL_LABEL[signal]}${isBandSignal(signal) ? ` (${pick === null ? 'All' : BAND_ROWS[pick]})` : ''} → ${ASPECT_LABEL[aspect]}`;
  // Each signal now, for the meters by the rows' names (the picked band's, or the bands' mean), ten times a second
  // while the panel shows.
  let meters = $state<{ track: number[]; bands: number[][]; lit: number }>({ track: [], bands: [], lit: 1 });
  // The routes' box, and how far the walking cells' borders have marched (px, one dash and gap is 12): on at the pace of
  // the behaviour's walk, standing still while it does.
  let routesBox = $state<HTMLElement | null>(null);
  let marched = 0;
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
    void loadLearned();

    // Where this device's pace settled last time (points, steps, resolution's share; and a level it could not hold
    // lately, not tried again until then): the visuals open there, not at the settings' most, and do not hunt for it
    // again (2026-10-18: a tablet opening at the most stepped down and up five times in eleven seconds, every step
    // starting the picture over).
    const pace = keptPace();
    let renderer: FlameRenderer | null = null;
    try {
      renderer = new FlameRenderer(canvas, Math.min(Math.round(2 ** ceilingOf(s.quality, devicePixels()).points), pace?.points ?? Infinity));
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
    // How the music stirs the fluid (stirring.ts).
    const stirring = newStirring();
    // What the visuals do, measured (meters.ts), and when the rating page last had them.
    let metering = new Meters();
    // The trial the meters and the governor last measured: each new one (the rating page's A, then B) starts them anew,
    // so one segment's flashes and motion are not counted, nor held back, in the next.
    let trialSeen: VisualSettings | null = null;
    let meteredAt = 0;
    // The hits seen since the rating page last had the loop's numbers (each band's last one, and how many came).
    const hitsSeen = Array<number>(BANDS).fill(-1);
    let hitsCounted = 0;
    // Which device this is, for the rating page: the app's own player, else the browser's platform.
    const device = () => (inApp && appInfo.player) || (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform || navigator.platform || 'browser';
    // The look's walk (drift.ts): when it last walked, and the time since. A track's own look goes into the settings
    // as far as the Track row's route into Drift says: at -100 % all of it (as before Drift), at -50 % half way, at 0
    // none, above 0 away from it. After the panel's Reset (`whole`), all of it.
    const drifting = newDrift();
    // The behaviour's walk (drift.ts): each walking slider's factor on its set value now (none: as set).
    const walking = newBehaviourWalk();
    let walkFactor: Record<string, number> = {};
    // The walking routes' factors (`Matrix.walks` order) and the matrix they are for (a route changed: none till the
    // next step).
    let routeWalk: Float32Array | undefined;
    let routeWalkFor: typeof matrix | null = null;
    let drifted = 0;
    let driftDt = 0;
    // For the rating page: the look's settings Drift's walk and the Symmetry moments moved since the last frame (how it
    // behaves), so a change of the look from anywhere else is told apart (counted, and while it blends in, settling).
    const behaviourMoved = new Set<string>();
    let looksOutside = 0;
    let blendOutside = false;
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
        trackEvent: endOf(how.routes[routeKey('track', null, 'look')] ?? 0),
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
    let passes = Math.min(ceilingOf(s.quality, devicePixels()).steps, pace?.passes ?? Infinity);
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
      // The dice: the recipes run against the track, as many slices as fit in four milliseconds of this frame.
      if (rollWanted) {
        rolling = { run: rollingFor($state.snapshot(s) as VisualSettings, m.key, m.position, rollWanted), scope: rollWanted };
        rollWanted = null;
      }
      if (rolling) {
        const until = performance.now() + 4;
        let next = rolling.run.next();
        while (!next.done && performance.now() < until) next = rolling.run.next();
        if (next.done) {
          const { scope } = rolling;
          rolling = null;
          rolled(next.value, scope);
        }
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
      const mode = modeOf(s.family);
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
        const outside = colours !== madeColours || changed.some((k) => !behaviourMoved.has(k));
        if (outside) looksOutside++;
        if (blend && shown) {
          from = shown;
          blendStart = now;
          blendMs = blend;
          blendOutside = outside;
        }
        made = Object.fromEntries(SHAPING.map((k) => [k, s[k]]));
        madeColours = colours;
        to = coloured(base!, mode, covered);
        if (!from) shown = to;
      }
      // A preset that changed nothing (all of it pinned) leaves nothing to blend.
      nextBlend = null;
      behaviourMoved.clear();
      // What is drawn, as the music and the settings give it (aspects.ts). Reduced motion: a quarter of the turning, all
      // of it steady, half the beats' and hits' share, no moments.
      const still = reducedMotion();
      // How it behaves now: as set, each walking slider at its walk's share of it (not with the rating page's trial).
      const behaving = trial.settings ? how : ({ ...how, ...Object.fromEntries(BEHAVIOUR_PINS.map((k) => [k, how[k] * (walkFactor[k] ?? 1)])) } as VisualSettings);
      const d = stepAspects(aspects, m, behaving, dt, {
        still,
        now,
        matrix: trial.settings ? undefined : matrix,
        fluidFull: FLUID_FULL,
        levers: governor.levers,
        routeWalk: routeWalkFor === matrix ? routeWalk : undefined,
      });
      const { turn, hits, parts, track } = d;
      if (m.playing && m.hits)
        m.hits.forEach((h, b) => {
          if (h.index > hitsSeen[b] && hitsSeen[b] >= 0 && h.index - hitsSeen[b] <= 3) hitsCounted += h.index - hitsSeen[b];
          hitsSeen[b] = h.index;
        });
      if (panel && routesBox) {
        marched = (marched + 40 * Math.abs(d.behaviour) * dt) % 12;
        routesBox.style.setProperty('--march', `${marched.toFixed(2)}px`);
      }
      if (panel && now - metered > 100) {
        metered = now;
        meters = { track: Array.from(track), bands: d.bands.map((v) => Array.from(v)), lit: d.lit };
      }
      // Drift: the look walking with the music (drift.ts), each bar another way, never far from where the track set it;
      // ten steps a second (each makes the flame anew), none with reduced motion. Not saved, nor sent to the group:
      // every device playing the track together walks alike.
      driftDt += dt;
      if (now - drifted >= 100) {
        drifted = now;
        if (!still) {
          const bar = Math.floor((m.beat ? m.beat.index : m.position * 2) / 4);
          const walked = stepDrift(drifting, { amount: d.look, trackEvent: 0, dt: driftDt, seed: seedOf(m.key ?? 'cratarr'), bar, pins: s.pins, own: current ?? (lookNow() as Preset), now: lookNow() as Preset });
          for (const k of Object.keys(walked)) behaviourMoved.add(k);
          Object.assign(s, walked);
        }
        // The behaviour's: its Track row once a track, its walk (none with reduced motion), under the set values; the
        // routes that walk with it too. A pinned slider stays as set; a pinned route walks if it is set to (its pin
        // is for the dice).
        walkFactor = stepBehaviourWalk(walking, {
          amount: still ? 0 : d.behaviour,
          trackEvent: endOf(how.routes[routeKey('track', null, 'behaviour')] ?? 0),
          dt: driftDt,
          seed: seedOf(m.key ?? 'cratarr'),
          bar: Math.floor((m.beat ? m.beat.index : m.position * 2) / 4),
          keys: [...BEHAVIOUR_PINS, ...matrix.walks.map((k) => `route:${k}`)],
          pins: s.pins.filter((p) => !p.startsWith('route:')),
        });
        routeWalk = Float32Array.from(matrix.walks, (k) => walkFactor[`route:${k}`] ?? 1);
        routeWalkFor = matrix;
        if (m.playing) remember(heard, track, d.bands);
        driftDt = 0;
      }
      // The moments: another symmetry; a pinned setting stays as it is. Seeded from the track and the beat it comes on
      // (else the half second), so every device playing it together makes the same choice.
      const at = `${m.key}|${m.beat ? m.beat.index : Math.floor(m.position * 2)}`;
      if (d.moments.symmetry && !(s.pins.includes('symmetry') && s.pins.includes('mirror'))) {
        const rnd = randomFrom(seedOf(`symmetry|${at}`));
        if (!s.pins.includes('symmetry')) s.symmetry = ((s.symmetry + Math.floor(rnd() * 7)) % 8) + 1;
        if (!s.pins.includes('mirror')) s.mirror = rnd() < 0.5 ? 1 : 0;
        behaviourMoved.add('symmetry').add('mirror');
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

      // What Quality lets the device draw at most (quality.ts `ceilingOf`): its resolution of the screen's own pixels
      // (at 60 % about a megapixel; the screen scales it up), and below, its points and steps.
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const top = ceilingOf(s.quality, canvas.clientWidth * canvas.clientHeight * dpr * dpr);
      const r = top.resolution * fit;
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
      // The fluid, stirred in time (stirring.ts): kicks on each band's hits at its part (ahead of them where they are
      // known) and on the beats, the downbeats and the sections from the middle, steady bands stirring with their level,
      // the flow breathing with the pulse. With reduced motion: no kicks, only the steady stirring. Where each part is in
      // the buffer: as the renderer's camera puts it, measured four times a second.
      const fluidNow = d.fluid;
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
      }
      const beatNow = m.playing && !still ? m.beat : null;
      const stirred = stir(stirring, {
        t: now / 1000,
        dt,
        amount: fluidNow,
        parts: partsWhere,
        turns: d.rates,
        lights: hits,
        level: d.bands.map((v) => v[B.level]),
        continuity: still ? how.continuity.map(() => 1) : how.continuity,
        hits: m.playing && m.hits ? m.hits.map((h) => (h.index < 0 ? null : { index: h.index, strength: h.weight, nextIn: h.nextIn })) : [],
        beat: beatNow ? { ...beatNow, nextIn: beatNow.length - beatNow.since } : null,
        downbeat: still ? 0 : track[T.downbeat],
        section: still ? 0 : track[T.section],
        loudness: d.lit,
        swell: track[T.swell],
        bps: m.bps,
        waves: still ? 0 : d.given[A.waves],
        position: m.position,
        center: [0.5, 0.5],
        // What each push is worth: its route into Stir, Flow or Swirl (the trial's own routes with the rating page's),
        // times that column's scale.
        gain: (signal, band, aspect, x) =>
          behaving[aspect] *
          (trial.settings ? (aspects.matrix ? heightOn(aspects.matrix, signal, band, aspect, x) : x) : heightOn(matrix, signal, band, aspect, x, routeWalkFor === matrix ? routeWalk : undefined)),
      });
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
          // The zooms (zoom.ts): as fast as their routes let their sliders, held back with the pace; none with reduced
          // motion.
          tunnel: still ? 0 : TUNNEL_MOST * d.tunnel * governor.levers.pace,
          // The look's effects, as far as the Effects setting and its routes let them, moving with the track's time.
          effects: effectsNow(how, d.effects, m.position),
          dive: still ? 0 : DIVE_MOST * d.dive * governor.levers.pace,
          forces: stirred.forces,
          flow: stirred.flow,
          // At the Speed setting's pace, as the turning (a quarter with reduced motion; none at 0). The fluid takes at most
          // 0.05 s a step: at 30 frames a second, Speed past 150 % flows no faster.
          dt: dt * behaving.speed * governor.levers.pace * (still ? 0.25 : 1),
          warp: d.warp,
        },
        w,
        h,
      );
      // What this frame did, measured (meters.ts): the screen read into tiles (back a frame or two later), the parts'
      // motion and change, a new look and why. The meters' values for the rating page four times a second.
      if (trial.settings !== trialSeen) {
        trialSeen = trial.settings;
        metering = new Meters();
        Object.assign(governor, newGovernor());
        // Nor is a thumb just after the rating page judged on its trials.
        recent = [];
      }
      renderer.measure(w, h, now / 1000);
      const stats: FrameStats = {
        t: now / 1000,
        // What the effects add to the meters (effects.ts), beside the flame's own.
        extra: effectsReport(how, d.effects),
        // The dive's turn counts as each part's.
        turnRates: renderer?.diveTurnRate ? d.rates.map((r) => r + renderer!.diveTurnRate) : d.rates,
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
        const values = metering.now();
        // With the trial of the rating page, only the flashes' rule: its budgets are what the ratings are to tell.
        const l = stepGovernor(governor, values, { dt: Math.min(1, (now - meteredAt) / 1000), limits: limitsNow, wild: wildNow(how.wild), activity: d.lit, budgets: !trial.settings });
        const since = Math.max(0.05, (now - meteredAt) / 1000);
        meteredAt = now;
        trial.meters = values;
        if (panel) {
          live = Object.fromEntries(BEHAVIOUR_PINS.map((k) => [k, behaving[k] * (k in d.levels ? d.levels[k as keyof typeof d.levels] : 1)]));
          routeWalkNow = Object.fromEntries(matrix.walks.map((k) => [`route:${k}`, walkFactor[`route:${k}`] ?? 1]));
          density = cell
            ? densityOf(heard, isBandSignal(cell.signal) ? { signal: B[cell.signal as keyof typeof B], band: pick } : { track: T[cell.signal as keyof typeof T] })
            : null;
        }
        trial.loop = {
          intensity: m.intensity,
          bps: m.bps,
          hitsPerSecond: hitsCounted / since,
          activity: d.lit,
          swing: l.swing,
          looks: looksOutside,
          settling: !!from && blendOutside,
          view: [canvas.clientWidth, canvas.clientHeight],
          points: renderer.pointCount,
          steps: passes,
          fps,
          device: device(),
        };
        const factor = budgetOf(1, wildNow(how.wild), d.lit);
        recent.push({ meters: values, loop: trial.loop, relative: Object.fromEntries(BUDGETED.map((k) => [k, values[k] / factor])) as Record<Budgeted, number> });
        if (recent.length > WINDOW) recent.shift();
        hitsCounted = 0;
        const marks = { flash: holding(l, 'swing'), speed: holding(l, 'pace'), wild: (['swing', 'pace', 'evolution', 'hue', 'size', 'gap'] as const).some((k) => holding(l, k)) };
        if (marks.flash !== !!governed.flash || marks.speed !== !!governed.speed || marks.wild !== !!governed.wild) governed = marks;
      }
      // A picture for a snapshot, while this frame is still in the canvas.
      if (pictureWanted) {
        const done = pictureWanted;
        pictureWanted = null;
        done(pictureOf(canvas));
      }

      // Adapting, under what Quality allows, to hold the FPS setting rather than settle under it.
      // Each second: if more than a fifth of its frames came late for their slot, a step down (steps, then resolution to
      // 60 % of the setting's, then points, halved; several at once where frames took over twice their slot). A step
      // back up (points, then resolution, then steps) only after `wait` seconds in a row with no late frame; a step up
      // taken back within five seconds doubles the wait (up to a minute), so the load settles instead of swinging. The
      // step-down judged its window of frames once per frame after it filled until 2026-10-17 (from one step to the most
      // within a second, behind again, over and over).
      const most = Math.round(2 ** top.points);
      if (renderer.pointCount > most) renderer.setPoints(most);
      passes = Math.min(passes, top.steps);
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
                : passes < top.steps
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
      // Closed, a rating run ends with them (moved, it goes on where they went).
      if (!visuals.open) rating.run = 0;
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

{#snippet bandPinOf(key: 'lighting' | 'evolution' | 'continuity', label: string, b: number)}
  {#if s.pins.includes(bandPin(key, b))}
    <button
      class="pin"
      aria-label="Unpin {label}"
      use:held={'pin'}
      onclick={(e) => {
        e.preventDefault();
        unpinBand(key, b);
      }}><Icon name="pin" size={16} /></button
    >
  {:else}
    <span></span>
  {/if}
{/snippet}

{#snippet routeRow(signal: Signal)}
  <span class="signal" use:held={`signal:${signal}`}>{SIGNAL_LABEL[signal]}<i style:width="{Math.round(Math.min(1, Math.max(0, meterOf(signal))) * 100)}%"></i></span>
  {#each ASPECTS as aspect (aspect)}
    {@const a = cellOf(signal, aspect)}
    <button
      class="cell"
      class:negative={a.now < 0}
      class:mixed={a.mixed}
      class:pinned={a.pinned}
      class:walks={a.walks}
      class:picked={cell?.signal === signal && cell?.aspect === aspect}
      style:--now={Math.abs(a.now)}
      aria-label="{cellLabel(signal, aspect)}, {signed(a.v)}"
      onclick={() => (cell = { signal, aspect })}
      >{#if a.path}<svg class="curve" viewBox="0 0 32 24" preserveAspectRatio="none" aria-hidden="true"><path d={a.path} /></svg>{/if}</button
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
        {#if !rating.run}
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
        <button class="round" aria-label="Randomise" use:held={'randomise'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={diceTapped}
          ><Icon name="dice" size={24} /></button
        >
        {#if undo}
          <button class="round" aria-label="Undo" use:held={'undo'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={undoRoll}
            ><Icon name="undo" size={24} /></button
          >
        {/if}
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
        {/if}
        {#if docked}
          <button class="round" aria-label="Over the page" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => setVisualsSide(false)}
            ><Icon name="expand" size={22} /></button
          >
        {:else if player.roomy}
          <button class="round" aria-label="Beside the page" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={beside}
            ><Icon name="sidebar" size={24} /></button
          >
        {/if}
        {#if canFullscreen}
          <button
            class="round"
            aria-label={fullscreen ? 'Exit full screen' : 'Full screen'}
            onpointerdown={(e) => e.stopPropagation()}
            onpointerup={(e) => e.stopPropagation()}
            onclick={fullscreen ? leaveFullscreen : goFullscreen}><Icon name={fullscreen ? 'exitFullscreen' : 'fullscreen'} size={24} /></button
          >
        {/if}
      </div>
      {#if panel && !rating.run}
        <div class="panel" role="group" aria-label="Settings" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()}>
          {#each [COMPUTE, SHOW, LOOK] as rows, g (g)}
            <div class="rows" class:look={g > 0}>
              {#if g === 1}<h3 use:held={'behaviour'}>Behaviour</h3>{:else if g === 2}<h3 use:held={'look'}>Look</h3>{/if}
              {#each rows as row (row.key)}
                <label class="row" class:pinnable={g > 0}>
                  <span use:held={row.key}>{row.label}{#if governed[row.key]}<span class="governed" aria-label="Held back" use:held={'held'}></span>{/if}</span>
                  <span class="slider">
                    <input
                      type="range"
                      min={RANGES[row.key][0]}
                      max={RANGES[row.key][1]}
                      step={row.step}
                      value={valueOf(row.key)}
                      oninput={(e) => set(row.key, Number(e.currentTarget.value))}
                    />
                    {#if liveAt(row.key) !== null}<i class="live" style:left="calc(8px + {liveAt(row.key)} * (100% - 16px))"></i>{/if}
                  </span>
                  <output>{row.text(valueOf(row.key))}</output>
                  {#if g > 0}
                    {#if s.pins.includes(row.key) && isPinnable(row.key)}
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
          <div class="routes" role="group" aria-label="Routes" bind:this={routesBox}>
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
            <div class="grid-head" bind:this={gridHead}>
              <div class="grid" style:--columns={ASPECTS.length}>
                <span class="head-corner"></span>
                {#each ASPECTS as aspect (aspect)}
                  <span class="aspect" use:held={`aspect:${aspect}`}>{ASPECT_LABEL[aspect]}</span>
                {/each}
              </div>
            </div>
            <div
              class="grid-scroll"
              bind:clientWidth={gridView}
              onscroll={(e) => {
                if (gridHead) gridHead.scrollLeft = e.currentTarget.scrollLeft;
              }}
            >
              <div class="grid" style:--columns={ASPECTS.length}>
                {#each TRACK_SIGNALS as signal (signal)}
                  {@render routeRow(signal)}
                {/each}
                <div class="bands" role="radiogroup" aria-label="Band" style:width="{gridView}px">
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
            </div>
            {#if cell && edited}
              {@const { signal, aspect } = cell}
              <div class="route">
                <span use:held={'route:curve'}>{cellLabel(signal, aspect)}</span>
                <CurveEditor
                  curve={edited.curve}
                  mixed={edited.mixed}
                  live={liveOnCurve}
                  {density}
                  walks={edited.walks}
                  pinned={edited.pinned}
                  onchange={(c, done) => setCurve(signal, aspect, c, done)}
                  onwalks={(on) => setWalks(signal, aspect, on)}
                  onpin={(on) => setPins(keysOf(signal, aspect).map((k) => `route:${k}`), on)}
                />
              </div>
            {/if}
          </div>
          {#each BAND_ROWS as band, b (band)}
            <div class="rows band">
              <h3 use:held={`band:${b}`}>{band}</h3>
              {#each TIMES as time (time.key)}
                <label class="row pinnable">
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
                  {@render bandPinOf(time.key, time.label, b)}
                </label>
              {/each}
              <label class="row pinnable">
                <span use:held={'continuity'}>Continuity</span>
                <input type="range" min={RANGES.continuity[0]} max={RANGES.continuity[1]} step="0.05" value={s.continuity[b]} oninput={(e) => setContinuity(b, Number(e.currentTarget.value))} />
                <output>{percent(s.continuity[b])}</output>
                {@render bandPinOf('continuity', 'Continuity', b)}
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
                loosened = false;
                resets++;
                // To the group: all but the look now (the track's preset follows once drawn), every pin let go.
                share(Object.fromEntries(SHARED_KEYS.filter((k) => !isLook(k)).map((k) => [k, $state.snapshot(s[k])])));
                showChrome();
              }}>Reset</button
            >
            <button class="reset" use:held={'rate'} onclick={startRating}>Rate</button>
            <button class="reset" use:held={'forget'} onclick={forget}>Forget</button>
          </div>
        </div>
      {/if}
      {#if np && !panel}
        <div class="track">
          {#if !rating.run}
            <div class="thumbs">
              <button class="round" class:on={thumbShown === 'up'} aria-label="Thumbs up" use:held={'thumbs-up'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => thumb('up')}
                ><Icon name="thumbsUp" size={22} /></button
              >
              <button class="round" class:on={thumbShown === 'down'} aria-label="Thumbs down" use:held={'thumbs-down'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => thumb('down')}
                ><Icon name="thumbsDown" size={22} /></button
              >
            </div>
          {/if}
          <strong>{np.name}</strong>
          {#if np.artist}<span>{np.artist}</span>{/if}
        </div>
        <div class="transport">
          <button class="round" aria-label="Previous track" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('previous')}
            ><Icon name="previous" size={26} /></button
          >
          <button class="round" aria-label={paused() ? 'Play' : 'Pause'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('play-pause')}
            ><Icon name={paused() ? 'play' : 'pause'} size={26} /></button
          >
          <button class="round" aria-label="Next track" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('next')}
            ><Icon name="next" size={26} /></button
          >
        </div>
      {/if}
    </div>
  {/if}
  {#if sought}
    <div class="sought" class:back={sought.side < 0} aria-live="polite">{sought.side > 0 ? '+' : '−'}{sought.secs} s</div>
  {/if}
  {#if snapshotsOpen}
    <SnapshotsSheet onclose={() => (snapshotsOpen = false)} capture={capturePicture} trackName={np?.name ?? null} {recall} over={!docked} />
  {/if}
  <Help />
  {#if rating.run}{#key rating.run}<RateMode />{/key}{/if}
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
  .slider {
    position: relative;
    display: grid;
  }

  .slider .live {
    position: absolute;
    bottom: -3px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #3fd18f;
    transform: translateX(-50%);
    pointer-events: none;
  }

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

  /* The grid: its names' column, then a cell an aspect, at least 28 px (a finger's tap, its number readable) and at most
     2.1em, the panel's width shared out between them where it fits; else it scrolls across on its own, the signals'
     names kept at its left and the aspects' (a grid of their own over it, the same columns, following its scroll) at
     the top of the panel. Sized from the routes' width (cqw), not by their content, so both grids line up exactly. */
  .grid {
    --cell: clamp(28px, calc((100cqw - 4.6em - var(--columns) * 2px) / var(--columns)), 2.1em);
    display: grid;
    grid-template-columns: 4.6em repeat(var(--columns), var(--cell));
    gap: 2px;
    width: max-content;
  }

  .grid-head {
    position: sticky;
    /* Up to the panel's edge (its padding), so nothing shows over it. */
    top: -8px;
    z-index: 2;
    margin-top: 4px;
    padding-top: 2px;
    overflow: hidden;
    background: rgb(20 20 24);
  }

  .grid-scroll {
    padding-top: 2px;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scrollbar-width: thin;
    scrollbar-color: rgb(255 255 255 / 0.25) transparent;
    touch-action: pan-x pan-y;
  }

  /* The routes' header's first cell, over the signals' names (its own name: .corner is the top controls'). */
  .head-corner {
    position: sticky;
    left: 0;
    z-index: 1;
    background: rgb(20 20 24);
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

  /* A signal's name, kept at the grid's left over the cells scrolled under it. */
  .signal {
    position: sticky;
    left: 0;
    z-index: 1;
    padding-right: 4px;
    overflow: hidden;
    background: rgb(20 20 24);
    box-shadow: 4px 0 6px -4px rgb(0 0 0 / 0.7);
    font-size: 0.8rem;
    line-height: 28px;
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

  /* What the route gives now: the cell filled from the bottom that far, green the one way, pink the other. */
  .cell::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: calc(var(--now) * 100%);
    border-radius: 0 0 4px 4px;
    background: var(--accent);
    opacity: 0.85;
  }

  .cell.negative::before {
    background: #e8708f;
  }

  /* Pinned: a solid line round it (walking, its marching border is drawn as heavy instead). */
  .cell.pinned:not(.walks) {
    outline: 2px solid rgb(255 255 255 / 0.85);
    outline-offset: -2px;
  }

  /* Walking with the behaviour: long dashes and short gaps (10 and 2 px; the picked square's are short and even)
     marching round it, at the walk's pace. */
  .cell.walks::after {
    --line: 1.5px;
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    pointer-events: none;
    background:
      linear-gradient(90deg, #fff 10px, transparent 0) var(--march, 0px) 0 / 12px var(--line) repeat-x,
      linear-gradient(90deg, #fff 10px, transparent 0) calc(-1 * var(--march, 0px)) 100% / 12px var(--line) repeat-x,
      linear-gradient(0deg, #fff 10px, transparent 0) 0 calc(-1 * var(--march, 0px)) / var(--line) 12px repeat-y,
      linear-gradient(0deg, #fff 10px, transparent 0) 100% var(--march, 0px) / var(--line) 12px repeat-y;
  }

  .cell.walks.pinned::after {
    --line: 2.5px;
  }

  /* Picked too: the march inside the picked outline. */
  .cell.walks.picked::after {
    inset: 3px;
  }

  /* Its curve, scaled to the cell. */
  .cell .curve {
    position: absolute;
    inset: 2px;
    width: calc(100% - 4px);
    height: calc(100% - 4px);
    fill: none;
    stroke: rgb(255 255 255 / 0.9);
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }

  .cell.mixed {
    outline: 1px dashed rgb(255 255 255 / 0.5);
    outline-offset: -2px;
  }

  .cell.picked {
    outline: 2px dashed #fff;
    outline-offset: -1px;
  }

  /* The bands' chips: as wide as the grid shows, kept in sight as it scrolls. */
  .bands {
    position: sticky;
    left: 0;
    grid-column: 1 / -1;
    justify-self: start;
    box-sizing: border-box;
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

  /* The picked route's curve, kept in sight under the grid. */
  .route {
    position: sticky;
    /* Down to the panel's edge (its padding), so nothing shows under it; over the grid's sticky names and header. */
    bottom: -10px;
    z-index: 3;
    display: grid;
    gap: 4px;
    margin: 8px -8px 0;
    padding: 8px 10px 10px;
    border-radius: 10px 10px 0 0;
    background: rgb(36 36 42);
    box-shadow: 0 -6px 12px rgb(0 0 0 / 0.35);
    font-size: 0.85rem;
  }

  .route span {
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

  .thumbs {
    display: flex;
    gap: 4px;
    margin: 0 0 6px -12px;
  }

  .track {
    position: absolute;
    left: max(24px, env(safe-area-inset-left));
    right: 192px;
    bottom: max(24px, env(safe-area-inset-bottom));
    display: grid;
    gap: 2px;
    color: #fff;
    text-shadow: 0 1px 6px rgb(0 0 0 / 0.6);
  }

  .governed {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 6px;
    border-radius: 50%;
    background: #f0b44c;
    vertical-align: middle;
  }

  .sought {
    position: absolute;
    top: 50%;
    right: 12%;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgb(0 0 0 / 0.45);
    color: #fff;
    font-size: 1.1rem;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
    transform: translateY(-50%);
  }

  .sought.back {
    right: auto;
    left: 12%;
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
