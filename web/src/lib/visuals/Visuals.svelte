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
  import { newAspects, stepAspects, wavesMotion, zoomsMotion } from './aspects';
  import { newBehaviourWalk, newDrift, stepBehaviourWalk, stepDrift } from './drift';
  import { curveOf, endOf, heightAt, straight, tableOf, valueOf as kept, walksOf, type Curve, type RouteValue } from './curves';
  import { ASPECT_KNOB, emptyExtra, SETTING_KNOB } from './patch';
  import CurveEditor from './CurveEditor.svelte';
  import { densityOf, newHistory, remember } from './history';
  import { budgetOf, BUDGETED, holding, LEVERS, newGovernor, STANDING, stepGovernor, type Budgeted, type Levers } from './governor';
  import { fresh, limitsOf, type Learned } from './learning';
  import { getVisualsLimits } from '../api';
  import { toast } from '../toast.svelte';
  import type { MeterValues, Tiles } from './meters';
  import type { TrialLoop } from './trial.svelte';
  import { beatsOf } from './beats';
  import { DEFAULT_LIMITS, prechecking, type Checked } from './precheck';
  import { recipeFor, settingsOf, STIRRING, type Recipe, type Scope } from './recipe';
  import { signalsOf } from './signals';
  import { Meters, type FrameStats } from './meters';
  import { trial } from './trial.svelte';
  import { appInfo, inApp } from '../app.svelte';
  import { coverPalette, type PaletteColour } from './palette';
  import { setVisualsSide, visuals } from './shown.svelte';
  import { applyDevice, applyShared, followGroup, groupLook, share, SHARED_KEYS, sharedNow, showing, STANDARD } from './shared.svelte';
  import SnapshotsSheet from './SnapshotsSheet.svelte';
  import { held } from './held.svelte';
  import Help from './Help.svelte';
  import { newStirring, stir } from './stirring';
  import { FlameRenderer } from './render';
  import { Compositor } from './compose';
  import { recordPictures } from './recorder';
  import { chainNow as nodesNow } from './chain-params';
  import { newNormalState, signalsFor } from './normal';
  import { cabledSettings, cableGain, cableSignals, giveCables, type Cabled } from './chain-cables';
  import { shapeOf } from './evaluate';
  import { evaluate, newEvalState, type Evaluated } from './evaluate';
  import { askStep, morphTo, newMorph, stepMorph } from './morph';
  import { chainRoutes, chainShape, CHAINS, EXAMPLES, SCRATCH, type Chain } from './chains';
  import { cableOf, freeCableId, isRouteId, knobLabel as cableKnobLabel, newCable, removeCables, reroute, sourceLabel, type Cables, type Source } from './cable-actions';
  import CableMenu from './CableMenu.svelte';
  import CableSheet from './CableSheet.svelte';
  import KnobCables from './KnobCables.svelte';
  import SourcePicker from './SourcePicker.svelte';
  import { leavingIds, morphedPatch, morphModules, rolledExtra, sameModules, withLeaving } from './chain-morph';
  import type { Cable, Knob, Module, PatchExtra, SignalRef } from './patch';
  import { ENGINE_TYPES, EXTRA_TYPES, extraType, PICTURE_LOOP_TYPES } from './registry';
  import PatchGraph from './PatchGraph.svelte';
  import { BLENDS, cratarrPatch, cratarrTypes, modulesOf, type ModuleType } from './patch';
  import { SOURCE_TYPES } from './sources/index';
  import { MODULATORS } from './modulators/modulators';
  import { ENGINE_EFFECTS, ENGINES, knobLabel, LOOP_EFFECTS, settingsReport, type Effect } from './effects';
  import {
    A,
    ASPECT_LABEL,
    ASPECTS,
    B,
    BAND_SIGNALS,
    COLUMN_SCALES,
    compile,
    heightOn,
    AMOUNT_ASPECTS,
    KNOB_ASPECTS,
    LEVEL_ASPECTS,
    MOMENT_ASPECTS,
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
  // Another look: a random preset into the look's settings not pinned.
  function rollAnother() {
    loadAnother(Math.floor(Math.random() * 0x100000000), true);
    showChrome();
  }

  // The governor (governor.ts), stepped with the meters four times a second in the loop; and the limits it holds the
  // picture within, as the thumbs had learned them (learning.ts), frozen: no thumbs or learning in this version
  // (decision 0057: "we drop taste altogether"); the server keeps them as they are.
  const governor = newGovernor();
  let learned = $state<Learned>(fresh(STANDING));
  let learnedVersion = 0;
  const limitsNow = $derived(limitsOf(learned));
  const status = (e: unknown) => (e && typeof e === 'object' && 'status' in e ? (e as { status?: number }).status : undefined);
  async function loadLearned() {
    try {
      const got = await getVisualsLimits();
      learnedVersion = got.version;
      learned = { ...fresh(STANDING), ...got.limits } as Learned;
    } catch (e) {
      // None learned yet (or not to be had now): where they start.
      if (status(e) === 404) {
        learnedVersion = 0;
        learned = fresh(STANDING);
      }
    }
  }

  // The dice: everything at random, a look and how it behaves (a recipe: recipe.ts), both to the group. Each try draws
  // its look first (65: a look brings its effects, which count against the limits) and runs the recipe with it against
  // the playing track from where it is (precheck.ts), a slice a frame: the first within the limits
  // (the governor's, as Calm ↔ Wild has them; three flashes a second whatever it says) is taken, after eight the one
  // over them by least; where the track's signals are not known yet the first, the governor holding it as it plays.
  // The undo puts back the look and behaviour from before (until the next roll).
  //
  // A double tap rolls how it moves only (the owner, 64.1: "double tap becomes this randomize"; `MOVEMENT` in
  // recipe.ts), its look and the rest as they are; and it may go "twice the limits": checked, and governed while it
  // stands, as at Wild's most (the shared `loose`: every device of the group alike), till the next roll, an undo (back to
  // what stood before), Reset or a recall on any of them.
  // Three flashes a second over a quarter of the screen stay the rule. A single tap waits out the double's window (a
  // roll takes about as long anyway).
  const BEHAVIOUR = ['routes', 'flash', 'speed', 'intensity', 'fluid', 'smoothing', 'haze', 'tunnel', 'dive', ...SCALE_KEYS, 'lighting', 'evolution', 'continuity'] as const;
  const TRIES = 8;
  const DOUBLE_MS = 250;
  let rollWanted: Scope | null = null;
  let rolling: { run: Generator<void, Rolled>; scope: Scope } | null = null;
  // A roll's recipe and, for a full one, the look drawn with it (and checked with it).
  type Rolled = { recipe: Recipe; look: Drawn | null };
  const wildNow = (wild: number) => (s.loose ? Math.max(wild, 1) : wild);
  let undo = $state<{ look: Record<string, number>; pins: string[]; behaviour: Partial<VisualSettings>; loose: number; extra: PatchExtra } | null>(null);
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
  function* rollingFor(base: VisualSettings, key: string | null, from: number, scope: Scope): Generator<void, Rolled> {
    const [source, id] = key ? [key.slice(0, key.indexOf('/')), key.slice(key.indexOf('/') + 1)] : ['', ''];
    const signals = key ? signalsOf(source, id) : null;
    const beats = key ? (beatsOf(source, id) ?? null) : null;
    const wild = 2 ** (2 * ((scope === 'movement' ? Math.max(base.wild, 1) : base.wild) - 0.5));
    const limits: Record<string, number> = { ...Object.fromEntries(Object.entries(limitsNow).map(([k, v]) => [k, v * wild])), flash: DEFAULT_LIMITS.flash };
    let best: (Rolled & { by: number }) | null = null;
    for (let k = 0; k < TRIES; k++) {
      const recipe = recipeFor(Math.floor(Math.random() * 0x100000000), ASPECTS);
      // A full roll's look, drawn as Another look draws it (what is pinned kept), before the check, so the check is of
      // what will show; a double tap keeps the look.
      const look = scope === 'all' ? yield* presetDraws(Math.floor(Math.random() * 0x100000000), pinned()) : null;
      if (!signals) return { recipe, look };
      const checked: Checked = yield* prechecking(recipe, look ? { ...base, ...look.preset } : base, signals, beats, from, limits, undefined, scope);
      if (checked.ok) return { recipe, look };
      const by = checked.over.reduce((sum, m) => sum + (checked.worst[m as keyof typeof checked.worst] as number) / (limits[m] ?? 1), 0);
      if (!best || by < best.by) best = { recipe, look, by };
    }
    return best!;
  }
  function rolled({ recipe, look }: Rolled, scope: Scope) {
    const now = $state.snapshot(s) as VisualSettings;
    undo = { look: lookNow(), pins: [...now.pins], behaviour: Object.fromEntries(BEHAVIOUR.map((k) => [k, now[k]])) as Partial<VisualSettings>, loose: now.loose, extra: now.extra };
    const next = settingsOf(recipe, now, scope);
    // Within the chain (the owner: "chains become presets, then within that we can freely evolve"): the cables there
    // are get new amounts; none is added (one the recipe leaves out keeps its own).
    next.routes = Object.fromEntries(Object.entries(now.routes).map(([k, v]) => [k, endOf(v) !== 0 && k in next.routes && endOf(next.routes[k]) !== 0 ? next.routes[k] : v]));
    for (const k of BEHAVIOUR) if (!s.pins.includes(k)) (s as Record<string, unknown>)[k] = next[k];
    s.loose = scope === 'movement' ? 1 : 0;
    // A full roll evolves the extras within their chain too (decision 0056): their knobs anew, morphed to as a look
    // blends in; the chain itself (modules, cables) as it is.
    if (scope === 'all') s.extra = rolledExtra(now.extra, GRAPH_TYPES, Math.random);
    saveVisualSettings();
    share({ ...Object.fromEntries(BEHAVIOUR.map((k) => [k, $state.snapshot(s[k])])), loose: s.loose, extra: $state.snapshot(s.extra) });
    // The look the roll was checked with, into the settings as a drawn one is (blended in, to the group).
    if (look) drawingNow = { draws: (function* (): Generator<void, Drawn> { return look; })(), blend: ANOTHER_MS, share: true, load: true, why: 'dice' };
  }
  function undoRoll() {
    if (!undo) return;
    const was = undo;
    undo = null;
    drawingNow = null;
    rolling = null;
    nextBlend = ANOTHER_MS;
    lookCame = 'dice';
    Object.assign(s, was.behaviour, was.look, { pins: was.pins, extra: was.extra });
    s.loose = was.loose;
    saveVisualSettings();
    share({ ...Object.fromEntries(BEHAVIOUR.map((k) => [k, $state.snapshot(s[k])])), ...lookNow(), pins: [...s.pins], loose: s.loose, extra: $state.snapshot(s.extra) });
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
    // A recall ends a double tap's room, for the group too (shared with the rest below).
    const settings: Record<string, unknown> = { ...snap.settings, loose: 0 };
    // One saved in 63: its Fluid on the finer scale since.
    if ((settings.v ?? 1) === 1 && typeof settings.fluid === 'number') settings.fluid = fluidFrom63(settings.fluid);
    // One saved before the columns' scales (64.2): all of each.
    for (const k of SCALE_KEYS) if (!(k in settings)) settings[k] = 1;
    // One saved before the zooms (65): neither.
    for (const k of ['tunnel', 'dive'] as const) if (!(k in settings)) settings[k] = 0;
    // One saved before the effects (65): none, and all of the Effects setting.
    for (const k of ['fx1', 'fx1Amount', 'fx2', 'fx2Amount', 'fx3', 'fx3Amount', 'fxLoop', 'fxLoopAmount', 'facets', 'lattice'] as const) if (!(k in settings)) settings[k] = 0;
    for (const k of ['fx1K1','fx1K2','fx2K1','fx2K2','fx3K1','fx3K2','fxLoopK1','fxLoopK2'] as const) if (!(k in settings)) settings[k] = 0.5;
    if (!('effects' in settings)) settings.effects = 1;
    // One saved before the flame could be off (70): it showed.
    if (!('flameLayer' in settings)) settings.flameLayer = 1;
    // One saved before the patch (70): the chain as it was then, today's alone. A recall sets the chain whole.
    if (!('extra' in settings)) settings.extra = emptyExtra();
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
    // ---- The chains to start from (the owner on 70: "a few sensible chains as presets, the ability to start from
  // scratch"; claude-5f's chains.ts) ---------------------------------------------------------------------------------
  // Applied as a recall is, to the group too: its routes whole, its extras whole, none muted, its settings over the
  // rest and pinned (a track's preset leaves them), morphed to as a look blends in. The chain shown is the one the
  // patch still is (its cables as it set them, no extras), else the owner's own.
  function applyChain(c: Chain) {
    const settings = (c.settings ?? {}) as Record<string, unknown>;
    const keys = Object.keys(settings);
    undo = null;
    applyShared({
      ...settings,
      routes: chainRoutes(c, BANDS),
      extra: JSON.parse(JSON.stringify(c.extra)),
      muted: [],
      loose: 0,
      pins: [...s.pins.filter((p) => !keys.includes(p)), ...keys.filter((k) => isPinnable(k))],
    });
    share(sharedNow());
    path = [];
    showChrome();
  }
  // A chain is its modules and its cables (which go from where into what), not their amounts: the dice and the knobs
  // move those within it (the owner: "chains become presets, then within that we can freely evolve").
  const wiring = (routes: Record<string, RouteValue>, extra: PatchExtra) =>
    [...Object.entries(routes).filter(([, v]) => endOf(v) !== 0).map(([k]) => k), ...extra.cables.map((c) => `${JSON.stringify(c.from)}>${c.to.module}.${c.to.knob}`)].sort().join(',');
  // Every cable by the knob it moves: its page (a column's routes, or the extras' cables into a knob).
  const cableGroups = $derived.by(() => {
    const by = new Map<string, number>();
    for (const c of patchNow.cables) if (endOf(c.value) !== 0) by.set(`${c.to.module}.${c.to.knob}`, (by.get(`${c.to.module}.${c.to.knob}`) ?? 0) + 1);
    return [...by].map(([mk, n]) => {
      const [mid, kid] = splitKnob(mk);
      const module = extraModule(mid);
      const label = `${module ? moduleName(module) : (MODULE_LABEL[mid] ?? mid)} · ${ROW_OF.get(SETTING_OF.get(mk) as One)?.label ?? anyKnob(mid, kid)?.label ?? kid}`;
      return { mk, n, label, page: `moves:${mk}` };
    });
  });
  const chainNow = $derived(
    [SCRATCH, ...CHAINS, ...EXAMPLES].find((c) => wiring(s.routes, s.extra) === wiring(c.routes, c.extra) && sameModules(s.extra, c.extra) && chainShape(s as never) === chainShape({ ...s, ...c.settings } as never)) ?? null,
  );

  // The settings' rows: the drawing's cost first (ceilings: the device's own pace may keep it lower), then how the show
  // behaves, then the look (the preset: pinned where moved).
  type One = Exclude<keyof VisualSettings, 'lighting' | 'evolution' | 'continuity' | 'routes' | 'pins' | 'extra'>;
  const MIRROR = ['Off', 'On'];
  // The Colours (65: Family folded in, the owner: "YES Family into Colours"): the look's four families, which a track's
  // look or the dice draw, then the cover's and a fire's, which only a hand chooses (and so pins).
  const COLOURS = ['Spectrum', 'Arc', 'Two tones', 'Three', 'Cover', 'Fire'];
  const modeOf = (family: number): ColourMode => (family >= 5 ? 'fire' : family >= 4 ? 'cover' : 'rainbow');
  // Each engine's choice and its controls (the moments reset them).
  const ENGINE_KNOBS = { fx1: ['fx1K1', 'fx1K2'], fx2: ['fx2K1', 'fx2K2'], fx3: ['fx3K1', 'fx3K2'], fxLoop: ['fxLoopK1', 'fxLoopK2'] } as const;
  // What a moment (65.1) moves a discrete setting to: another of its values, drawn with `rnd` (the same on every device
  // of a group). Symmetry another of its eight; Mirror and Lattice the other; a Lens another (or none); a Form another;
  // the Colours another of the look's four families (Cover and Fire, chosen by hand, stay); an engine another of its
  // effects where one is chosen (None stays none: its Amount's routes bring it in).
  function momentOf(a: (typeof MOMENT_ASPECTS)[number], v: number, rnd: () => number): number {
    const other = (lo: number, hi: number) => (hi <= lo ? v : lo + ((Math.round(v) - lo + 1 + Math.floor(rnd() * (hi - lo))) % (hi - lo + 1)));
    if (a === 'symmetry') return ((v + Math.floor(rnd() * 7)) % 8) + 1;
    if (a === 'mirror' || a === 'lattice') return v ? 0 : 1;
    if (a === 'family') return v > 3 ? v : other(0, 3);
    if (a === 'fx1' || a === 'fx2' || a === 'fx3' || a === 'fxLoop') return v > 0 ? other(1, RANGES[a][1]) : v;
    return other(RANGES[a][0], RANGES[a][1]);
  }
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
      // A level of the look (Facets) has its row in the Look group.
      const key = (COLUMN_SCALES as Partial<Record<Aspect, One>>)[a] ?? ((LEVEL_ASPECTS as readonly Aspect[]).includes(a) && !(PRESET_KEYS as readonly string[]).includes(a) ? (a as One) : null);
      return key ? [column(key, a)] : [];
    }),
  ];
  // An effect setting's text: its effect's name, or none.
  // An engine's (or the loop's) sub-effect by its place: its name, or none.
  const subName = (list: readonly { name: string }[]) => (v: number) => {
    const e = list[Math.round(v) - 1];
    return e ? e.name[0].toUpperCase() + e.name.slice(1) : 'None';
  };
  // Under each engine (and the loop), the chosen sub-effect's own knobs, labelled by it (65; the owner: "one or two of
  // the most relevant controls per chosen subeffect").
  const knobRows = (prefix: 'fx1' | 'fx2' | 'fx3' | 'fxLoop', effect: Effect | undefined): Row[] =>
    (effect?.knobs ?? []).map((knob, i) => ({ key: `${prefix}K${i + 1}` as One, label: knob.label, step: 0.05, text: knob.text }));
  const LOOK: Row[] = $derived([
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
    // Facets (65): where the light flows, each cell of a lattice one light.
    { key: 'facets', label: 'Facets', step: 0.05, text: percent },
    { key: 'lattice', label: 'Lattice', step: 1, text: (v) => (v ? 'Hex' : 'Square') },
    // The look's effects (65): three on the picture, one on the loop, each with its amount.
    ...([1, 2, 3] as const).flatMap((n) => [
      { key: `fx${n}` as One, label: ENGINES[n - 1].label, step: 1, text: subName(ENGINE_EFFECTS[ENGINES[n - 1].key]) },
      { key: `fx${n}Amount` as One, label: 'Amount', step: 0.05, text: percent },
      ...knobRows(`fx${n}`, ENGINE_EFFECTS[ENGINES[n - 1].key][Math.round(s[`fx${n}`]) - 1]),
    ]),
    { key: 'fxLoop', label: 'Loop', step: 1, text: subName(LOOP_EFFECTS) },
    { key: 'fxLoopAmount', label: 'Loop amount', step: 0.05, text: percent },
    ...knobRows('fxLoop', LOOP_EFFECTS[Math.round(s.fxLoop) - 1]),
  ]);
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
  // The fifth band was Presence until 65's Presence column took the name (the owner chose "Upper mid").
  const BAND_ROWS = ['Sub', 'Bass', 'Low mid', 'Mid', 'Upper mid', 'Air'];
  const TIMES = (['lighting', 'evolution'] as const).map((key) => ({
    key,
    // Lighting is Linger since 65 (decision 0050): how long a part's light rings on, and its envelope's release.
    label: key === 'lighting' ? 'Linger' : 'Evolution',
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
  // ---- The nested panel (decision 0056, step 2) ----------------------------------------------------------------------
  // The patch's things as pages, over today's settings (patch.ts: the Cratarr chain is exactly them): the top (the chain,
  // the whole frame, the layers, finish, all the cables, this device); a layer; a module and its knobs; a knob's cables
  // (what moves it, each opened in the curve's editor) and adding one; the flame's bands. `path` is where it is.
  let path = $state<string[]>([]);
  let panelBox = $state<HTMLElement | null>(null);
  const here = $derived(path.at(-1) ?? '');
  function go(page: string) {
    path = [...path, page];
    cell = null;
    panelBox?.scrollTo(0, 0);
    showChrome();
  }
  function back(steps = 1) {
    path = path.slice(0, Math.max(0, path.length - steps));
    cell = null;
    panelBox?.scrollTo(0, 0);
    showChrome();
  }
  // A knob's setting today, and its route column where it has one; each setting's row (its label, step and text).
  const SETTING_OF = new Map(Object.entries(SETTING_KNOB).map(([k, [m, id]]) => [`${m}.${id}`, k as One]));
  const ASPECT_OF = new Map(ASPECTS.map((a) => [ASPECT_KNOB[a].join('.'), a]));
  const ROW_OF = $derived(new Map([...COMPUTE, ...SHOW, ...LOOK].map((r) => [r.key, r])));
  const MODULE_LABEL: Record<string, string> = {
    flame: 'Flame',
    show: 'Show',
    fluid: 'Fluid',
    tunnel: 'Tunnel',
    dive: 'Dive',
    trails: 'Trails',
    'loop-effect': 'Loop effect',
    fx1: ENGINES[0].label,
    fx2: ENGINES[1].label,
    fx3: ENGINES[2].label,
  };
  // Each module's knobs (`module.knob`) as its page groups them.
  const effectKnobs = (m: string) => [{ title: null, knobs: ['effect', 'amount', 'k1', 'k2'].map((k) => `${m}.${k}`) }];
  const GROUPS: Record<string, { title: string | null; knobs: string[] }[]> = {
    flame: [
      { title: 'Look', knobs: ['family', 'form', 'detail', 'twist', 'warp', 'symmetry', 'mirror', 'final', 'spread', 'hue', 'lightness', 'vividness', 'purity'].map((k) => `flame.${k}`) },
      { title: 'Moved by the music', knobs: ['shape', 'rotation', 'lights', 'presence'].map((k) => `flame.${k}`) },
    ],
    show: [{ title: null, knobs: ['size', 'glow', 'saturation', 'smoothing', 'haze', 'waves'].map((k) => `show.${k}`) }],
    fluid: [{ title: null, knobs: ['fluid', 'stir', 'flow', 'swirl', 'facets', 'lattice'].map((k) => `fluid.${k}`) }],
    tunnel: [{ title: null, knobs: ['tunnel.tunnel'] }],
    dive: [{ title: null, knobs: ['dive.dive'] }],
    trails: [{ title: null, knobs: ['trails.trails'] }],
    'loop-effect': effectKnobs('loop-effect'),
    fx1: effectKnobs('fx1'),
    fx2: effectKnobs('fx2'),
    fx3: effectKnobs('fx3'),
  };
  // The flame layer's own loop, in its order.
  const FLAME_LOOP = ['fluid', 'tunnel', 'dive', 'trails', 'loop-effect'];
  // A module's line under its name: what it is set to now.
  function moduleLine(m: string): string {
    const off = (v: number) => (v > 0 ? percent(v) : 'Off');
    if (m === 'flame') return 'Look, moved by the music, bands';
    if (m === 'show') return `Size, glow, haze ${signedPercent(s.haze)}`;
    if (m === 'fluid') return off(s.fluid);
    if (m === 'tunnel') return off(s.tunnel);
    if (m === 'dive') return off(s.dive);
    if (m === 'trails') return percent(s.trails);
    if (m === 'loop-effect') return subName(LOOP_EFFECTS)(s.fxLoop);
    const n = Number(m.slice(2));
    return subName(ENGINE_EFFECTS[ENGINES[n - 1].key])(s[`fx${n}` as One] as number);
  }
  // What moves a column, a row each: a whole-track signal; a band signal as one row where every band's is the same,
  // else one a band.
  type CableRow = { id: string; signal: Signal; pick: number | null; label: string; sub: string; v: number };
  function cableRows(aspect: Aspect): CableRow[] {
    const out: CableRow[] = [];
    for (const t of TRACK_SIGNALS) {
      const v = endOf(s.routes[routeKey(t, null, aspect)] ?? 0);
      if (Math.abs(v) > 1e-6) out.push({ id: t, signal: t, pick: null, label: SIGNAL_LABEL[t], sub: '', v });
    }
    for (const b of BAND_SIGNALS) {
      const vs = Array.from({ length: BANDS }, (_, i) => endOf(s.routes[routeKey(b, i, aspect)] ?? 0));
      if (vs.every((v) => Math.abs(v) < 1e-6)) continue;
      if (vs.every((v) => Math.abs(v - vs[0]) < 1e-6)) out.push({ id: b, signal: b, pick: null, label: SIGNAL_LABEL[b], sub: 'Every band', v: vs[0] });
      else vs.forEach((v, i) => Math.abs(v) > 1e-6 && out.push({ id: `${b}:${i}`, signal: b, pick: i, label: SIGNAL_LABEL[b], sub: BAND_ROWS[i], v }));
    }
    return out;
  }
  const isOpen = (c: CableRow, aspect: Aspect) => cell?.aspect === aspect && cell.signal === c.signal && pick === c.pick;
  function openCable(c: CableRow, aspect: Aspect) {
    pick = c.pick;
    cell = isOpen(c, aspect) ? null : { signal: c.signal, aspect };
    showChrome();
  }
  // ---- Building with the patch (decision 0056): the layers past the flame, the picture's own loop, the modulators ----
  const MODULATOR_TYPES = Object.values(MODULATORS);
  /** A module of the extras by its id, and what holds it. */
  function extraModule(id: string): Module | undefined {
    const e = s.extra;
    for (const l of e.layers) for (const m of [l.source, ...l.modules, ...l.loop]) if (m.id === id) return m;
    return [...e.loop, ...e.modulators].find((m) => m.id === id);
  }
  const moduleName = (m: Module) => {
    const t = extraType(m.type);
    const same = [...s.extra.layers.flatMap((l) => [l.source, ...l.modules, ...l.loop]), ...s.extra.loop, ...s.extra.modulators].filter((x) => x.type === m.type);
    return `${t?.label ?? m.type}${same.length > 1 ? ` ${same.indexOf(m) + 1}` : ''}`;
  };
  // A change to the extras: kept, and to the group, as every setting is.
  function changeExtra(f: (e: PatchExtra) => void) {
    f(s.extra);
    saveVisualSettings();
    share({ extra: $state.snapshot(s.extra) });
    showChrome();
  }
  const takenIds = () => {
    const e = s.extra;
    return new Set([
      ...['frame', 'flame', 'show', 'fluid', 'tunnel', 'dive', 'trails', 'loop-effect', 'fx1', 'fx2', 'fx3', 'loop', 'finish'],
      ...e.layers.flatMap((l) => [l.id, l.source.id, ...l.modules.map((m) => m.id), ...l.loop.map((m) => m.id)]),
      ...e.loop.map((m) => m.id),
      ...e.modulators.map((m) => m.id),
    ]);
  };
  function newId(prefix: string): string {
    const taken = takenIds();
    for (let n = 1; ; n++) if (!taken.has(`${prefix}-${n}`)) return `${prefix}-${n}`;
  }
  const freshModule = (type: string, id: string): Module => ({ id, type, knobs: Object.fromEntries((extraType(type)?.knobs ?? []).map((k) => [k.id, k.default])) });
  function addLayer(type: string) {
    const id = newId(type);
    changeExtra((e) => e.layers.push({ id, source: freshModule(type, `${id}-source`), modules: [], loop: [], blend: { mode: 'add', amount: 1 } }));
    path = [...path.slice(0, -1), `xlayer:${id}`];
  }
  function addTo(where: 'loop' | 'modulators' | string, type: string) {
    const id = newId(type);
    changeExtra((e) => {
      if (where === 'loop') e.loop.push(freshModule(type, id));
      else if (where === 'modulators') e.modulators.push(freshModule(type, id));
      else e.layers.find((l) => l.id === where)?.modules.push(freshModule(type, id));
    });
    path = [...path.slice(0, -1), `xmodule:${id}`];
  }
  // A module, a layer, or a modulator removed, with every cable to or from it and what macros it was in.
  function removeExtra(id: string) {
    changeExtra((e) => {
      const gone = new Set([id]);
      const layer = e.layers.find((l) => l.id === id);
      if (layer) for (const m of [layer.source, ...layer.modules, ...layer.loop]) gone.add(m.id);
      e.layers = e.layers.filter((l) => l.id !== id).map((l) => ({ ...l, modules: l.modules.filter((m) => m.id !== id), loop: l.loop.filter((m) => m.id !== id) }));
      e.loop = e.loop.filter((m) => m.id !== id);
      e.modulators = e.modulators.filter((m) => m.id !== id);
      e.cables = e.cables.filter((c) => !gone.has(c.to.module) && !('modulator' in c.from && gone.has(c.from.modulator)));
      e.macros = e.macros.map((m) => ({ ...m, targets: m.targets.filter((t) => !gone.has(t.module)) }));
    });
    back();
  }
  function setKnob(id: string, knob: string, v: number) {
    const m = extraModule(id);
    if (!m) return;
    m.knobs[knob] = v;
    morphNext = KNOB_MORPH;
    saveVisualSettings();
    share({ extra: $state.snapshot(s.extra) });
    showChrome();
  }
  const knobText = (k: Knob, v: number) => (k.kind === 'step' ? `${Math.round(v)}` : k.min === 0 && k.max === 1 ? percent(v) : k.min === -1 && k.max === 1 ? signedPercent(v) : v.toFixed(2));
  const BLEND_NAMES = { add: 'Add', screen: 'Screen', over: 'Over', multiply: 'Multiply', window: 'Window' } as const;
  // The cables into one knob of the extras (or a modulator's port), and adding one: from a signal (the whole track's,
  // a band's, or every band's mean) or a modulator.
  const cablesTo = (mid: string, kid: string) => s.extra.cables.filter((c) => c.to.module === mid && c.to.knob === kid);
  let openCableId = $state<string | null>(null);
  const cableFromName = (c: Cable) =>
    'modulator' in c.from
      ? (() => {
          const m = extraModule((c.from as { modulator: string }).modulator);
          return m ? moduleName(m) : 'A modulator';
        })()
      : `${SIGNAL_LABEL[c.from.signal as Signal] ?? c.from.signal}${c.from.band !== undefined ? ` · ${BAND_ROWS[c.from.band]}` : ''}`;
  // A knob of any module, the extras' or today's chain's (claude-5f's chain-cables.ts moves those by the extras' cables).
  const anyKnob = (mid: string, kid: string): Knob | undefined =>
    GRAPH_TYPES[extraModule(mid)?.type ?? modulesOf(patchNow).get(mid)?.type ?? '']?.knobs.find((k) => k.id === kid);
  // `module.knob` (a band's knob has its own dot: `flame.lighting.2`).
  const splitKnob = (mk: string): [string, string] => [mk.slice(0, mk.indexOf('.')), mk.slice(mk.indexOf('.') + 1)];
  const cableable = (mk: string) => {
    const k = anyKnob(...splitKnob(mk));
    return !!k && k.kind !== 'step' && !k.device;
  };
  function addExtraCable(mid: string, kid: string, from: Cable['from'], to: number) {
    const kind = anyKnob(mid, kid)?.kind;
    const id = newCableId();
    changeExtra((e) => e.cables.push({ id, from, to: { module: mid, knob: kid }, value: 0.5, mode: kind === 'level' ? 'level' : 'swing' }));
    path = path.slice(0, to);
    openCableId = id;
  }
  function newCableId(): string {
    const taken = new Set(s.extra.cables.map((c) => c.id));
    for (let n = 1; ; n++) if (!taken.has(`cable-${n}`)) return `cable-${n}`;
  }
  function setExtraCable(id: string, c: Curve, done: boolean) {
    const cable = s.extra.cables.find((x) => x.id === id);
    if (!cable) return;
    cable.value = kept({ h1: [...c.h1], h2: [...c.h2], h: c.h });
    if (!done) return;
    saveVisualSettings();
    share({ extra: $state.snapshot(s.extra) });
    showChrome();
  }
  function removeExtraCable(id: string) {
    changeExtra((e) => (e.cables = e.cables.filter((c) => c.id !== id)));
    openCableId = null;
  }

  // ---- The graph builder (decision 0056, step 3: claude-632's PatchGraph) beside the panel where there is room ------
  // The same patch as the panel's, drawn; every edit made the way the panel makes it. Where its nodes stand is this
  // device's own (kept here, not shared).
  const patchNow = $derived(cratarrPatch(s));
  // The extras as set, a new value on any change of them: the frame loop morphs to each (decision 0056, step 6).
  const extraNow = $derived($state.snapshot(s.extra) as PatchExtra);
  // The next change's morph (seconds): a knob moved here, over a moment (`KNOB_MORPH`); anything else (a module added or
  // removed, a recall, the dice, the group's) as a look blends in.
  const KNOB_MORPH = 0.15;
  let morphNext: number | null = null;
  const GRAPH_TYPES: Record<string, ModuleType> = Object.fromEntries([...EXTRA_TYPES, ...cratarrTypes(RANGES, DEFAULTS)].map((t) => [t.type, t]));
  // ---- Cables in the rack (the owner on 70: "each line is clickable, removable"; a long press "to get the option to
  // delete or reroute"): claude-5f's KnobCables, CableSheet, CableMenu and SourcePicker over cable-actions.ts ----------
  const cablesNow = (): Cables => ({ routes: $state.snapshot(s.routes) as Record<string, RouteValue>, extra: $state.snapshot(s.extra) as PatchExtra });
  // The cables as they are to be: kept, and to the group once a change is done (a curve being dragged goes when let go).
  function setCables(next: Cables, done = true) {
    s.routes = next.routes;
    s.extra = next.extra;
    saveVisualSettings();
    if (done) share({ routes: $state.snapshot(s.routes), extra: $state.snapshot(s.extra) });
    showChrome();
  }
  const kindOf = (to: { module: string; knob: string }) => anyKnob(to.module, to.knob)?.kind ?? 'swing';
  // The cables into a knob (`module.knob`), the routes' that are on and the extras'.
  const cablesInto = (mk: string) => patchNow.cables.filter((c) => `${c.to.module}.${c.to.knob}` === mk && (!isRouteId(c.id) || endOf(c.value) !== 0));
  const cableName = (c: Cable) => `${sourceLabel(c.from, s.extra.modulators, GRAPH_TYPES)} → ${cableKnobLabel(c.to, GRAPH_TYPES, morphModules(s.extra))}`;
  // Every knob a cable may go into, by name (a Reroute's target).
  const cableKnobs = $derived(
    [...modulesOf(patchNow).values()].flatMap((m) =>
      (GRAPH_TYPES[m.type]?.knobs ?? [])
        .filter((k) => !k.device && k.kind !== 'step')
        .map((k) => ({ module: m.id, knob: k.id, label: cableKnobLabel({ module: m.id, knob: k.id }, GRAPH_TYPES, morphModules(s.extra)) })),
    ),
  );
  let menuCable = $state<string | null>(null);
  // A picked source as a cable's: a new modulator made first.
  function sourceFrom(pick: Source, cs: Cables): { from: Cable['from']; cables: Cables } {
    if (!('newModulator' in pick)) return { from: pick as Cable['from'], cables: cs };
    const id = newId(pick.newModulator);
    return { from: { modulator: id }, cables: { ...cs, extra: { ...cs.extra, modulators: [...cs.extra.modulators, freshModule(pick.newModulator, id)] } } };
  }
  // "+ Add" on a knob's page: a cable from the picked source at half, opened.
  function addCable(mk: string, pick: Source) {
    const [mid, kid] = splitKnob(mk);
    const made = sourceFrom(pick, cablesNow());
    const c = newCable(made.from as never, { module: mid, knob: kid }, kindOf({ module: mid, knob: kid }), freeCableId(made.cables));
    const cs = made.cables;
    setCables(isRouteId(c.id) ? { ...cs, routes: { ...cs.routes, [c.id]: c.value } } : { ...cs, extra: { ...cs.extra, cables: [...cs.extra.cables, c] } });
    path = [...path.slice(0, -1), `cable:${c.id}`];
  }
  // A cable as its page changed it: its source (with its reference and speed: a route then becomes an extra cable),
  // its value, its mode. A new id (a route's key) follows in the trail.
  function changeCable(id: string, next: Cable, done: boolean) {
    let cs = cablesNow();
    const was = cableOf(cs, id);
    if (!was) return;
    let now = id;
    if (JSON.stringify(was.from) !== JSON.stringify(next.from)) {
      const r = reroute(cs, id, 'from', next.from as never, kindOf(next.to));
      if (!r) return;
      cs = r.cables;
      now = r.id;
    }
    cs = isRouteId(now)
      ? { ...cs, routes: { ...cs.routes, [now]: next.value } }
      : { ...cs, extra: { ...cs.extra, cables: cs.extra.cables.map((c) => (c.id === now ? { ...c, value: next.value, mode: next.mode } : c)) } };
    setCables(cs, done);
    if (now !== id) path = path.map((p) => (p === `cable:${id}` ? `cable:${now}` : p));
  }
  function removeCable(id: string) {
    setCables(removeCables(cablesNow(), [id]));
    path = path.filter((p) => p !== `cable:${id}`);
  }
  // One end moved (a Reroute): to the picked source or knob; the cable's page, if open, follows it.
  function rerouteCable(id: string, end: 'from' | 'to', pick: Source | { module: string; knob: string }) {
    const cs = cablesNow();
    const was = cableOf(cs, id);
    if (!was) return;
    const made = end === 'from' ? sourceFrom(pick as Source, cs) : { from: was.from, cables: cs };
    const to = end === 'to' ? (pick as { module: string; knob: string }) : was.to;
    const r = reroute(made.cables, id, end, (end === 'from' ? made.from : to) as never, kindOf(to));
    if (!r) return;
    setCables(r.cables);
    path = path.filter((p) => !p.startsWith('reroute:')).map((p) => (p === `cable:${id}` ? `cable:${r.id}` : p));
  }

  const GRAPH_KEY = 'cratarr-visuals-graph';
  let graphAt = $state<Record<string, { x: number; y: number }>>(
    (() => {
      try {
        return JSON.parse(localStorage.getItem(GRAPH_KEY) ?? '{}');
      } catch {
        return {};
      }
    })(),
  );
  let wide = $state(typeof innerWidth === 'number' ? innerWidth >= 1100 : false);
  $effect(() => {
    const resized = () => (wide = innerWidth >= 1100);
    addEventListener('resize', resized);
    return () => removeEventListener('resize', resized);
  });
  const graphShown = $derived(panel && wide && !docked);
  function graphConnect(from: Cable['from'], to: Cable['to']) {
    const aspect = ASPECT_OF.get(`${to.module}.${to.knob}`);
    if ('signal' in from && aspect && !from.reference && !from.speed) {
      // A route can hold it: one band's, or (a band signal without one) every band's.
      pick = from.band ?? null;
      setCurve(from.signal as Signal, aspect, straight(0.5), true);
      return;
    }
    const kind = GRAPH_TYPES[[...extraModulesAll(), patchNow.frame].find((m) => m.id === to.module)?.type ?? '']?.knobs.find((k) => k.id === to.knob)?.kind;
    const id = newCableId();
    changeExtra((e) => e.cables.push({ id, from, to, value: 0.5, mode: kind === 'level' ? 'level' : 'swing' }));
  }
  const extraModulesAll = () => [...s.extra.layers.flatMap((l) => [l.source, ...l.modules, ...l.loop]), ...s.extra.loop, ...s.extra.modulators];
  function graphDisconnect(id: string) {
    if (id in s.routes) {
      const [signal, band, aspect] = id.split('|') as [Signal, string, Aspect];
      pick = band === '-' ? null : Number(band);
      setCurve(signal, aspect, straight(0), true);
    } else removeExtraCable(id);
  }
  function graphKnob(mid: string, kid: string, v: number) {
    const key = SETTING_OF.get(`${mid}.${kid}`);
    if (key) return set(key, v);
    const band = /^(lighting|evolution|continuity)\.(\d)$/.exec(kid);
    if (mid === 'flame' && band) {
      if (band[1] === 'continuity') setContinuity(Number(band[2]), v);
      else setTime(band[1] as 'lighting' | 'evolution', Number(band[2]), Math.log2(v));
      return;
    }
    setKnob(mid, kid, v);
  }
  function graphMove(id: string, x: number, y: number) {
    graphAt = { ...graphAt, [id]: { x, y } };
    try {
      localStorage.setItem(GRAPH_KEY, JSON.stringify(graphAt));
    } catch {
      /* this visit only */
    }
  }
  // A node opened: its page in the panel.
  function graphOpen(id: string) {
    const chainPage: Record<string, string[]> = {
      flame: ['layer:flame', 'module:flame'],
      show: ['layer:flame', 'module:show'],
      ...Object.fromEntries(FLAME_LOOP.map((m) => [m, ['layer:flame', `module:${m}`]])),
      fx1: ['module:fx1'],
      fx2: ['module:fx2'],
      fx3: ['module:fx3'],
      'layer:flame': ['layer:flame'],
    };
    const layer = id.startsWith('layer:') ? s.extra.layers.find((l) => l.id === id.slice('layer:'.length)) : undefined;
    path = chainPage[id] ?? (layer ? [`xlayer:${layer.id}`] : extraModule(id) ? [`xmodule:${id}`] : []);
    cell = null;
    panelBox?.scrollTo(0, 0);
    showChrome();
  }

  // A page's name in the breadcrumb.
  function pageLabel(page: string): string {
    const [kind, a = '', b = ''] = page.split(':');
    if (kind === 'layer' || kind === 'module') return MODULE_LABEL[a] ?? a;
    if (kind === 'band') return BAND_ROWS[Number(a)] ?? a;
    if (kind === 'cables') return ROW_OF.get(SETTING_OF.get(a) as One)?.label ?? a;
    if (kind === 'add' || kind === 'xadd' || kind.startsWith('new')) return 'Add';
    if (kind === 'xlayer') {
      const l = s.extra.layers.find((x) => x.id === a);
      return l ? moduleName(l.source) : a;
    }
    if (kind === 'xmodule') {
      const m = extraModule(a);
      return m ? moduleName(m) : a;
    }
    if (kind === 'xcables') {
      const [mid, kid] = splitKnob(a);
      const m = extraModule(mid);
      return (m ? undefined : ROW_OF.get(SETTING_OF.get(a) as One)?.label) ?? anyKnob(mid, kid)?.label ?? (extraType(m?.type ?? '')?.inputs?.includes(kid) ? 'Input' : kid);
    }
    if (kind === 'xaddband') return SIGNAL_LABEL[a as Signal] ?? b;
    if (kind === 'addband') return SIGNAL_LABEL[a as Signal] ?? b;
    if (kind === 'moves') return anyKnob(...splitKnob(a))?.label ?? ROW_OF.get(SETTING_OF.get(a) as One)?.label ?? a;
    if (kind === 'cable') {
      const c = cableOf({ routes: s.routes, extra: s.extra }, page.slice('cable:'.length));
      return c ? sourceLabel(c.from, s.extra.modulators, GRAPH_TYPES) : 'Cable';
    }
    if (kind === 'cableadd') return 'Add';
    if (kind === 'reroute') return 'Reroute';
    return { bands: 'Bands', chains: 'Chains', cables: 'Cables', device: 'This device' }[kind] ?? kind;
  }
  const devicePixels = () => innerWidth * innerHeight * Math.min(devicePixelRatio || 1, 2) ** 2;
  const valueOf = (key: One) => s[key] as number;
  function set(key: One, v: number) {
    const before = s[key];
    (s as Record<One, number | null>)[key] = v;
    // The look's settings and the behaviour's sliders moved are pinned: a track's or the dice's preset, and Drift,
    // leave them as they are.
    if (isPinnable(key) && !s.pins.includes(key)) s.pins = [...s.pins, key];
    // Another sub-effect chosen for an engine (or the loop): its knobs start from their middles, not where the last
    // one's were, and are not pinned.
    const knobs: Partial<Record<One, number>> = {};
    if ((key === 'fx1' || key === 'fx2' || key === 'fx3' || key === 'fxLoop') && Math.round(v) !== Math.round(Number(before))) {
      for (const k of [`${key}K1`, `${key}K2`] as One[]) {
        (s as Record<One, number | null>)[k] = 0.5;
        knobs[k] = 0.5;
      }
      s.pins = s.pins.filter((p) => !(p in knobs));
    }
    saveVisualSettings();
    // To the sync group: all but what is this device's own (its cost, its brightness).
    if ((SHARED_KEYS as readonly string[]).includes(key)) share({ [key]: v, ...knobs, ...(isPinnable(key) ? { pins: [...s.pins] } : {}) });
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
  const signed = (v: number) => `${v > 0 ? '+' : ''}${Math.round(v * 100)}%`;
  const cellLabel = (signal: Signal, aspect: Aspect) => `${SIGNAL_LABEL[signal]}${isBandSignal(signal) ? ` (${pick === null ? 'All' : BAND_ROWS[pick]})` : ''} → ${columnName(aspect)}`;
  // A column's name (65.1): an effect's control is named for the chosen effect's (Sides, Spin…; — where it has none),
  // with its engine's in front where there is room (the editor's title, a cell's label); an Amount under its engine.
  const KNOB_SLOT = { fx1K1: ['fx1', 0], fx1K2: ['fx1', 1], fx2K1: ['fx2', 0], fx2K2: ['fx2', 1], fx3K1: ['fx3', 0], fx3K2: ['fx3', 1], fxLoopK1: ['fxLoop', 0], fxLoopK2: ['fxLoop', 1] } as const;
  const SLOT_ENGINE = { fx1: 'kaleidoscope', fx2: 'eyedo', fx3: 'hydra', fxLoop: 'loop' } as const;
  function columnName(aspect: Aspect, short = false): string {
    const knob = KNOB_SLOT[aspect as keyof typeof KNOB_SLOT];
    if (knob) {
      const label = knobLabel(SLOT_ENGINE[knob[0]], s[knob[0]], knob[1]) ?? '—';
      return short ? label : `${ASPECT_LABEL[knob[0]]}: ${label}`;
    }
    if (short && (AMOUNT_ASPECTS as readonly string[]).includes(aspect)) return 'Amount';
    return ASPECT_LABEL[aspect];
  }
  // Each signal now, for the meters by the rows' names (the picked band's, or the bands' mean), ten times a second
  // while the panel shows.
  let meters = $state<{ track: number[]; bands: number[][]; lit: number }>({ track: [], bands: [], lit: 1 });
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
    // The patch past the flame (decision 0056): the other layers and the picture's own loop, drawn between the flame's
    // show and finish (compose.ts). Its knobs as set for now; the cables move them once the evaluator is in.
    let compositor: Compositor | null = null;
    try {
      compositor = renderer ? new Compositor(renderer.context) : null;
    } catch (e) {
      console.warn('Visuals: no layers past the flame on this device', e);
    }
    // The picture lab's recorder (development only, where the lab asks): each frame's inputs, for a replay.
    if (import.meta.env.DEV && renderer) recordPictures(renderer, compositor);
    const normal = newNormalState();
    // The patch's cables, modulators and macros made into each knob's value this frame (claude-632's evaluate.ts):
    // what the compositor draws with; today's chain stays with aspects.ts.
    const evalState = newEvalState();
    let evaluated: Evaluated | null = null;
    // The morph (claude-632's morph.ts): the extras' knobs from one state to the next, never a cut; a chain change's
    // modules fading in from neutral and out to it (drawn till then: chain-morph.ts); a step only at its module's
    // neutral, the evaluator's moments too.
    const morph = newMorph(morphModules(extraNow), GRAPH_TYPES);
    let morphedTo = extraNow;
    let drawnExtra = extraNow;
    let goingBefore = 0;
    // The flame layer's share as shown: to its setting over a chain's morph (1.5 s whole), never a cut.
    let flameShown = s.flameLayer ?? 1;
    let shownKnobs: Record<string, Record<string, number>> = {};
    const knobsOf = (m: Module) => evaluated?.knobs[m.id] ?? shownKnobs[m.id] ?? m.knobs;

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
    // What the patch's cables make of today's settings that are no column (chain-cables.ts), as last worked out: the
    // look's as the flame is made, Purity as drawn, the bands' times and the behaviour as the aspects take them.
    let cabled: Cabled = { settings: {}, bands: {} };
    // The settings cables moved when the flame was last made: one let go of (a trial starting, a cable taken out) returns
    // to its setting, not a change from outside.
    let cabledBefore = new Set<string>();
    const lookOf = (k: string) => ((cabled.settings as Record<string, number | undefined>)[k] ?? s[k as keyof VisualSettings]) as number;
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
    // The flash meter's readings, taken first in a frame (flashmeter.ts) and given to the meters with its stats.
    let unread: Tiles[] = [];


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
      unread.push(...renderer.tiles());

      // How it behaves: as the settings say, or as the rating page's trial does (the look stays the settings').
      const how = trial.settings ?? s;
      // The sound's waveform only while a Waves layer draws it (listening costs a tablet much of a frame).
      const m = musicNow(now, cabled.bands.lighting ?? how.lighting, s.extra.layers.some((l) => l.source.type === 'waves' && l.blend.amount > 0));
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
      const changed = !made ? SHAPING : SHAPING.filter((k) => made![k] !== lookOf(k));
      if (changed.length || colours !== madeColours) {
        // Unframed: the framing below measures what is drawn anyway.
        if (changed.length) base = flameOf(Object.fromEntries(PRESET_KEYS.map((k) => [k, lookOf(k)])) as Preset, false);
        const coverCame = !changed.length && madeColours.startsWith('cover/') && colours.startsWith('cover/') && from;
        const blend = nextBlend ?? (changed.every((k) => BY_DEGREES.has(k)) && colours === madeColours ? 0 : coverCame ? null : ANOTHER_MS);
        // A cable's move is the patch's own, not from outside.
        const outside = colours !== madeColours || changed.some((k) => !behaviourMoved.has(k) && !(k in cabled.settings) && !cabledBefore.has(k));
        if (outside) looksOutside++;
        if (blend && shown) {
          from = shown;
          blendStart = now;
          blendMs = blend;
          blendOutside = outside;
        }
        made = Object.fromEntries(SHAPING.map((k) => [k, lookOf(k)]));
        cabledBefore = new Set(Object.keys(cabled.settings));
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
      const walked = trial.settings ? how : ({ ...how, ...Object.fromEntries(BEHAVIOUR_PINS.map((k) => [k, how[k] * (walkFactor[k] ?? 1)])) } as VisualSettings);
      // And as the patch's cables move it (last frame's: they are read once the aspects have the signals).
      const behaving = trial.settings ? how : ({ ...walked, ...cabled.settings, ...cabled.bands } as VisualSettings);
      // The signals, read once a frame (normal.ts), with the Track pulse aspects.ts makes; and the patch's cables into
      // today's columns (chain-cables.ts) given beside the routes, from them and the modulators as last stepped.
      let read: (ref: SignalRef) => number = () => 0;
      const extraCables = s.extra?.cables ?? [];
      const cableWalk = Object.fromEntries(extraCables.map((c) => [c.id, walkFactor[`cable:${c.id}`] ?? 1]));
      const d = stepAspects(aspects, m, behaving, dt, {
        still,
        now,
        matrix: trial.settings ? undefined : matrix,
        fluidFull: FLUID_FULL,
        levers: governor.levers,
        routeWalk: routeWalkFor === matrix ? routeWalk : undefined,
        // The Track pulse goes into Music's own track (which the next frame makes anew) for signalsFor to read; the
        // modulators feeding these cables are last frame's (the evaluator steps after, on the envelope made here).
        extra: (got, trackNow, envelope) => {
          m.track[T.track] = trackNow[T.track];
          read = signalsFor(normal, m, now);
          const src = { signals: cableSignals(read, still), outputs: evalState.values, modulators: s.extra?.modulators ?? [], types: GRAPH_TYPES };
          if (extraCables.length && !trial.settings) giveCables(got, extraCables, src, envelope, cableWalk, BANDS);
          cabled = trial.settings ? { settings: {}, bands: {} } : cabledSettings(walked as unknown as Record<string, unknown>, extraCables, src, envelope, GRAPH_TYPES, cableWalk, BANDS);
        },
      });
      const { turn, hits, parts, track } = d;
      if (m.playing && m.hits)
        m.hits.forEach((h, b) => {
          if (h.index > hitsSeen[b] && hitsSeen[b] >= 0 && h.index - hitsSeen[b] <= 3) hitsCounted += h.index - hitsSeen[b];
          hitsSeen[b] = h.index;
        });
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
          keys: [...BEHAVIOUR_PINS, ...matrix.walks.map((k) => `route:${k}`), ...(s.extra?.cables ?? []).filter((c) => shapeOf(c.value).walks).map((c) => `cable:${c.id}`)],
          pins: s.pins.filter((p) => !p.startsWith('route:')),
        });
        routeWalk = Float32Array.from(matrix.walks, (k) => walkFactor[`route:${k}`] ?? 1);
        routeWalkFor = matrix;
        if (m.playing) remember(heard, track, d.bands);
        driftDt = 0;
      }
      // The moments (routes.ts `MOMENT_ASPECTS`): each moves its own setting to another value, a pinned one staying as
      // it is. Seeded from the track and the beat it comes on (else the half second), so every device playing it
      // together makes the same choice. A moment is a new look: the meters count it (looks a minute), as the governor's
      // gap between moments holds it (65.1: it was never counted).
      const at = `${m.key}|${m.beat ? m.beat.index : Math.floor(m.position * 2)}`;
      let momentCame = false;
      for (const a of MOMENT_ASPECTS) {
        if (!d.moments[a] || s.pins.includes(a)) continue;
        const next = momentOf(a, s[a], randomFrom(seedOf(`${a}|${at}`)));
        if (next === s[a]) continue;
        s[a] = next;
        behaviourMoved.add(a);
        // Another of an engine's effects: its controls back to their middle, as when one is chosen by hand.
        const knobs = ENGINE_KNOBS[a as keyof typeof ENGINE_KNOBS];
        for (const k of knobs ?? []) if (!s.pins.includes(k)) s[k] = 0.5;
        momentCame = true;
      }
      if (momentCame) {
        nextBlend = ANOTHER_MS;
        lookCame = 'moment';
        saveVisualSettings();
      }

      if (!to) return;
      const k = from ? Math.min(1, (now - blendStart) / blendMs) : 1;
      shown = from && k < 1 ? between(from, to, k * k * (3 - 2 * k)) : to;
      if (k >= 1) from = null;
      const drawnFlame = evolved(shown, parts, lookOf('purity'), s.spread);
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
      // The waves' height, held back with the pace as the turning is (their phase stays the track's, so a group waves
      // alike).
      const wavesNow = still ? 0 : d.given[A.waves] * governor.levers.pace;
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
        waves: wavesNow,
        position: m.position,
        center: [0.5, 0.5],
        // What each push is worth: its route into Stir, Flow or Swirl (the trial's own routes with the rating page's),
        // times that column's scale.
        gain: (signal, band, aspect, x) =>
          behaving[aspect] *
          (trial.settings
            ? aspects.matrix
              ? heightOn(aspects.matrix, signal, band, aspect, x)
              : x
            : heightOn(matrix, signal, band, aspect, x, routeWalkFor === matrix ? routeWalk : undefined) + cableGain(extraCables, signal, band, aspect, x, cableWalk)),
      });
      // The patch past the flame, this frame: on when there is anything of it, morphed to the extras as set.
      const extraSet = extraNow;
      const extraChanged = extraSet !== morphedTo;
      if (extraChanged) {
        morphTo(morph, morphModules(extraSet), morphNext ?? ANOTHER_MS / 1000, GRAPH_TYPES);
        morphNext = null;
        morphedTo = extraSet;
      }
      const morphFrame = stepMorph(morph, dt, m.bps > 0 ? 1 / m.bps : null, GRAPH_TYPES);
      shownKnobs = { ...morphFrame.knobs, ...Object.fromEntries(morphFrame.leaving.map((l) => [l.id, l.knobs])) };
      const extra = withLeaving(extraSet, drawnExtra, leavingIds(morphFrame.leaving));
      if (compositor && (extraChanged || morphFrame.leaving.length !== goingBefore)) compositor.prune(extra);
      goingBefore = morphFrame.leaving.length;
      drawnExtra = extra;
      if (compositor && renderer) {
        const p = morphedPatch(patchNow, morphFrame.knobs);
        evaluated = p.layers.length > 1 || p.loop.length || p.modulators.length ? evaluate(p, GRAPH_TYPES, read, evalState, dt, { envelope: d.lit, frame: { beat: m.beat }, walk: cableWalk }) : null;
        // A step its cables move on: taken at its module's next neutral.
        for (const r of evaluated?.moments ?? []) askStep(morph, r.module, r.knob, GRAPH_TYPES);
        const flameTo = s.flameLayer ?? 1;
        flameShown = flameTo > flameShown ? Math.min(flameTo, flameShown + dt / (ANOTHER_MS / 1000)) : Math.max(flameTo, flameShown - dt / (ANOTHER_MS / 1000));
        const flame = flameShown;
        const active = Compositor.active(extra, knobsOf, flame);
        renderer.compose = active
          ? {
              active,
              run: (picture, width, height) =>
                compositor!.run(picture, width, height, extra, knobsOf, { dt, time: now / 1000, beat: m.beat, signals: read, waveform: m.wave ?? null }, flame),
            }
          : null;
      }
      renderer.frame(
        passes,
        nodesNow(d, {
          how,
          levers: governor.levers,
          still,
          ceiling: s.brightness,
          coverage: frame ? frame.coverage : null,
          fire,
          position: m.position,
          beat: beatNow,
          forces: stirred.forces,
          flow: stirred.flow,
          dt,
          speed: behaving.speed,
        }),
        w,
        h,
      );
      // What this frame did, measured (meters.ts): the screen read into tiles (back two or more frames later), the parts'
      // motion and change, a new look and why. The meters' values for the rating page four times a second.
      if (trial.settings !== trialSeen) {
        trialSeen = trial.settings;
        metering = new Meters();
        Object.assign(governor, newGovernor());
      }
      renderer.measure(w, h, now / 1000);
      const stats: FrameStats = {
        t: now / 1000,
        // What the effects add (as the governor lets them), the waves' motion where nothing flows, and the zooms'
        // streaming out at the rates the renderer is given (aspects.ts).
        extra: (() => {
          const fx = settingsReport(how, d.effects, governor.levers, still, d.routed);
          const zooms = still ? 0 : zoomsMotion(d.tunnel * governor.levers.pace, d.dive * governor.levers.pace);
          return { ...fx, motion: (fx.motion ?? 0) + wavesMotion(wavesNow, d.fluid) + zooms };
        })(),
        // The dive's turn counts as each part's.
        turnRates: renderer?.diveTurnRate ? d.rates.map((r) => r + renderer!.diveTurnRate) : d.rates,
        shape: d.shape,
        weights: d.parts.map((p) => p.weight ?? 1),
        hue: d.parts.map((p) => p.hue),
        spread: d.parts.map((p) => p.spread ?? 1),
        scale: d.scale,
        saturation: d.saturation,
        look: lookCame,
        symmetry: d.moments.symmetry,
        tiles: unread.splice(0),
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
          live = {
            ...Object.fromEntries(BEHAVIOUR_PINS.map((k) => [k, behaving[k] * (k in d.levels ? d.levels[k as keyof typeof d.levels] : 1)])),
            // The effects' Amounts and controls as their routes have them now (65.1), marked under their Look rows.
            ...Object.fromEntries(AMOUNT_ASPECTS.map((k, i) => [k, behaving[k] * (d.routed.amounts[i] ?? 1)])),
            ...Object.fromEntries(KNOB_ASPECTS.map((k, i) => [k, Math.min(1, Math.max(0, behaving[k] + (d.routed.knobs[i] ?? 0)))])),
          };
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
      compositor?.dispose();
      renderer?.dispose();
      stopGroup();
      stopIdle?.();
      stopMusic();
    };
  });
</script>

{#snippet settingRow(row: Row | undefined)}
  {#if row}
    <label class="row pinnable">
      <span use:held={row.key}>{row.label}{#if governed[row.key]}<span class="governed" aria-label="Held back" use:held={'held'}></span>{/if}</span>
      <span class="slider">
        <input type="range" min={RANGES[row.key][0]} max={RANGES[row.key][1]} step={row.step} value={valueOf(row.key)} oninput={(e) => set(row.key, Number(e.currentTarget.value))} />
        {#if liveAt(row.key) !== null}<i class="live" style:left="calc(8px + {liveAt(row.key)} * (100% - 16px))"></i>{/if}
      </span>
      <output>{row.text(valueOf(row.key))}</output>
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
    </label>
  {/if}
{/snippet}

<!-- A knob of the patch (decision 0056): its setting's row, and where a route column reaches it, what moves it. -->
{#snippet knobRow(mk: string, cables = true)}
  {@const key = SETTING_OF.get(mk)}
  {@const row = key ? ROW_OF.get(key) : undefined}
  {@const aspect = ASPECT_OF.get(mk)}
  {#if row}
    <div class="knob">
      {@render settingRow(row)}
      {#if cables && aspect}
        {@const n = cablesInto(mk).length}
        <button class="moves" class:on={n > 0} aria-label="What moves {row.label}{n ? ` (${n})` : ''}" use:held={'moves'} onclick={() => go(`moves:${mk}`)}
          >{#if n}<span>{n}</span>{/if}<Icon name="chevronRight" size={16} /></button
        >
      {:else if cables && cableable(mk)}
        <!-- A knob no column reaches: the extras' cables into it (as a module's past the chain). -->
        {@const [mid, kid] = splitKnob(mk)}
        {@const n = cablesInto(`${mid}.${kid}`).length}
        <button class="moves" class:on={n > 0} aria-label="What moves {row.label}{n ? ` (${n})` : ''}" use:held={'moves'} onclick={() => go(`moves:${mk}`)}
          >{#if n}<span>{n}</span>{/if}<Icon name="chevronRight" size={16} /></button
        >
      {:else if cables}
        <span class="moves-gap" aria-hidden="true"></span>
      {/if}
    </div>
  {/if}
{/snippet}

<!-- A knob of a module past today's chain (decision 0056): its value in the extras, and what moves it. -->
{#snippet extraKnobRow(m: Module, k: Knob)}
  {@const n = cablesTo(m.id, k.id).length}
  <div class="knob">
    <label class="row pinnable">
      <span>{k.label}</span>
      <span class="slider">
        <input type="range" min={k.min} max={k.max} step={k.kind === 'step' ? 1 : (k.max - k.min) / 100} value={m.knobs[k.id] ?? k.default} oninput={(e) => setKnob(m.id, k.id, Number(e.currentTarget.value))} />
      </span>
      <output>{knobText(k, m.knobs[k.id] ?? k.default)}</output>
      <span></span>
    </label>
    <button class="moves" class:on={n > 0} aria-label="What moves {k.label}{n ? ` (${n})` : ''}" use:held={'moves'} onclick={() => go(`moves:${m.id}.${k.id}`)}
      >{#if n}<span>{n}</span>{/if}<Icon name="chevronRight" size={16} /></button
    >
  </div>
{/snippet}

{#snippet goRow(label: string, line: string, page: string)}
  <button class="go" onclick={() => go(page)}><span class="name">{label}{#if line}<small>{line}</small>{/if}</span><Icon name="chevronRight" size={18} /></button>
{/snippet}

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
      {#if graphShown}
        <div class="graph" role="group" aria-label="Patch" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()}>
          <PatchGraph
            patch={patchNow}
            types={GRAPH_TYPES}
            positions={graphAt}
            onconnect={graphConnect}
            ondisconnect={graphDisconnect}
            onknob={graphKnob}
            onmove={graphMove}
            onopen={graphOpen}
          />
        </div>
      {/if}
      {#if panel}
        <div class="panel" role="group" aria-label="Settings" bind:this={panelBox} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()}>
          {#if path.length}
            <div class="crumbs">
              <button class="back" aria-label="Back" onclick={() => back()}><Icon name="back" size={20} /></button>
              <span class="trail"
                >{#each path as page, i (i)}{#if i}<span class="sep">›</span>{/if}<span class:here={i === path.length - 1}>{pageLabel(page)}</span>{/each}</span
              >
            </div>
          {/if}
          {#if !path.length}
            <div class="rows">
              <h3>Chain</h3>
              {@render goRow(chainNow?.label ?? 'Your own', chainNow?.line ?? 'Changed since a chain was chosen', 'chains')}
            </div>
            <div class="rows">
              <h3 use:held={'behaviour'}>Whole frame</h3>
              {#each ['intensity', 'flash', 'speed', 'wild'] as k (k)}{@render settingRow(ROW_OF.get(k as One))}{/each}
            </div>
            <div class="rows">
              <h3>Layers</h3>
              {#if s.flameLayer > 0}{@render goRow('Flame', 'Its look, its show, its own loop', 'layer:flame')}{/if}
              {#each s.extra.layers as l (l.id)}{@render goRow(moduleName(l.source), `${BLEND_NAMES[l.blend.mode]} ${percent(l.blend.amount)}${l.modules.length ? ` · ${l.modules.map(moduleName).join(', ')}` : ''}`, `xlayer:${l.id}`)}{/each}
              <button class="go add" onclick={() => go('newlayer')}><span class="name">+ Add layer</span></button>
            </div>
            <div class="rows">
              <h3>Loop</h3>
              {#each s.extra.loop as m (m.id)}{@render goRow(moduleName(m), percent(m.knobs.amount ?? 0), `xmodule:${m.id}`)}{/each}
              <button class="go add" onclick={() => go('newloop')}><span class="name">+ Add to the loop</span></button>
            </div>
            <div class="rows">
              <h3>Finish</h3>
              {@render knobRow('frame.effects')}
              {#each ['fx1', 'fx2', 'fx3'] as m (m)}{@render goRow(MODULE_LABEL[m], moduleLine(m), `module:${m}`)}{/each}
            </div>
            <div class="rows">
              <h3>Modulators</h3>
              {#each s.extra.modulators as m (m.id)}{@render goRow(moduleName(m), `${s.extra.cables.filter((c) => 'modulator' in c.from && c.from.modulator === m.id).length} cables`, `xmodule:${m.id}`)}{/each}
              <button class="go add" onclick={() => go('newmodulator')}><span class="name">+ Add modulator</span></button>
            </div>
            <div class="rows">
              <h3>Patch</h3>
              {@render goRow('Cables', `${cableGroups.reduce((n, g) => n + g.n, 0)} in all`, 'cables')}
              {@render goRow('This device', 'Quality, frame rate, brightness', 'device')}
            </div>
          {:else if here.startsWith('moves:')}
            {@const mk = here.slice('moves:'.length)}
            <KnobCables cables={cablesInto(mk)} label={(c) => sourceLabel(c.from, s.extra.modulators, GRAPH_TYPES)} onopen={(id) => go(`cable:${id}`)} onlongpress={(id) => (menuCable = id)} onadd={() => go(`cableadd:${mk}`)} />
          {:else if here.startsWith('cable:')}
            {@const id = here.slice('cable:'.length)}
            {@const c = cableOf({ routes: s.routes, extra: s.extra }, id)}
            {#if c}
              <CableSheet
                cable={c}
                types={GRAPH_TYPES}
                modulators={s.extra.modulators}
                modules={morphModules(s.extra)}
                onchange={(next, done) => changeCable(id, next, done)}
                onremove={() => removeCable(id)}
                onreroute={(end) => go(`reroute:${end}:${id}`)}
              />
            {/if}
          {:else if here.startsWith('cableadd:')}
            {@const mk = here.slice('cableadd:'.length)}
            <SourcePicker mode="source" types={GRAPH_TYPES} modulators={s.extra.modulators} onpick={(pick) => addCable(mk, pick as Source)} onclose={() => back()} />
          {:else if here.startsWith('reroute:')}
            {@const [, end, id] = here.split(':') as [string, 'from' | 'to', string]}
            <SourcePicker mode={end === 'from' ? 'source' : 'knob'} types={GRAPH_TYPES} modulators={s.extra.modulators} knobs={cableKnobs} onpick={(pick) => rerouteCable(id, end, pick)} onclose={() => back()} />
          {:else if here === 'chains'}
            <div class="rows">
              {#each CHAINS as c (c.id)}<button class="go" class:on={chainNow?.id === c.id} onclick={() => applyChain(c)}><span class="name">{c.label}<small>{c.line}</small></span></button>{/each}
            </div>
            <div class="rows">
              <button class="go" class:on={chainNow?.id === SCRATCH.id} onclick={() => applyChain(SCRATCH)}><span class="name">{SCRATCH.label}<small>{SCRATCH.line}</small></span></button>
            </div>
            <div class="rows">
              <h3>Examples</h3>
              {#each EXAMPLES as c (c.id)}<button class="go" class:on={chainNow?.id === c.id} onclick={() => applyChain(c)}><span class="name">{c.label}<small>{c.line}</small></span></button>{/each}
            </div>
          {:else if here === 'layer:flame'}
            <div class="rows">
              <h3>Source</h3>
              {@render goRow('Flame', moduleLine('flame'), 'module:flame')}
            </div>
            <div class="rows">
              <h3>On this layer</h3>
              {@render goRow('Show', moduleLine('show'), 'module:show')}
            </div>
            <div class="rows">
              <h3>Its own loop</h3>
              {#each FLAME_LOOP as m (m)}{@render goRow(MODULE_LABEL[m], moduleLine(m), `module:${m}`)}{/each}
            </div>
            <div class="rows">
              <h3>Blend</h3>
              <label class="row pinnable">
                <span>Amount</span>
                <span class="slider"><input type="range" min="0" max="1" step="0.01" value={s.flameLayer} oninput={(e) => set('flameLayer', Number(e.currentTarget.value))} /></span>
                <output>{percent(s.flameLayer)}</output>
                <span></span>
              </label>
            </div>
            <div class="rows"><button class="go danger" onclick={() => (set('flameLayer', 0), (path = []))}><span class="name">Remove this layer</span></button></div>
          {:else if here.startsWith('module:')}
            {@const m = here.slice('module:'.length)}
            {#each GROUPS[m] ?? [] as g, i (i)}
              <div class="rows">
                {#if g.title}<h3>{g.title}</h3>{/if}
                {#each g.knobs as mk (mk)}{@render knobRow(mk)}{/each}
              </div>
            {/each}
            {#if m === 'flame'}
              <div class="rows">
                <h3>Each band's part</h3>
                {@render goRow('Bands', 'Linger, evolution, continuity', 'bands')}
              </div>
            {/if}
          {:else if here === 'bands'}
            <div class="rows">
              {#each BAND_ROWS as band, b (band)}{@render goRow(band, `${seconds(s.lighting[b])} · ${seconds(s.evolution[b])} · ${percent(s.continuity[b])}`, `band:${b}`)}{/each}
            </div>
          {:else if here.startsWith('band:')}
            {@const b = Number(here.slice('band:'.length))}
            <div class="rows">
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
          {:else if here === 'newlayer' || here === 'newloop' || here === 'newmodulator' || here.startsWith('newmodule:')}
            <div class="rows">
              {#if here === 'newlayer'}
                {#if s.flameLayer <= 0}<button class="go" onclick={() => (set('flameLayer', 1), (path = ['layer:flame']))}><span class="name">Flame</span></button>{/if}
                {#each SOURCE_TYPES as t (t.type)}<button class="go" onclick={() => addLayer(t.type)}><span class="name">{t.label}</span></button>{/each}
              {:else if here === 'newloop'}
                {#each PICTURE_LOOP_TYPES as t (t.type)}<button class="go" onclick={() => addTo('loop', t.type)}><span class="name">{t.label}</span></button>{/each}
              {:else if here === 'newmodulator'}
                {#each MODULATOR_TYPES as t (t.type)}<button class="go" onclick={() => addTo('modulators', t.type)}><span class="name">{t.label}</span></button>{/each}
              {:else}
                {#each ENGINE_TYPES as t (t.type)}<button class="go" onclick={() => addTo(here.slice('newmodule:'.length), t.type)}><span class="name">{t.label}</span></button>{/each}
              {/if}
            </div>
          {:else if here.startsWith('xlayer:')}
            {@const l = s.extra.layers.find((x) => x.id === here.slice('xlayer:'.length))}
            {#if l}
              <div class="rows">
                <h3>Source</h3>
                {@render goRow(moduleName(l.source), '', `xmodule:${l.source.id}`)}
              </div>
              <div class="rows">
                <h3>On this layer</h3>
                {#each l.modules as m (m.id)}{@render goRow(moduleName(m), percent(m.knobs.amount ?? 0), `xmodule:${m.id}`)}{/each}
                <button class="go add" onclick={() => go(`newmodule:${l.id}`)}><span class="name">+ Add module</span></button>
              </div>
              <div class="rows">
                <h3>Blend</h3>
                <button
                  class="go"
                  onclick={() => changeExtra(() => (l.blend.mode = BLENDS[(BLENDS.indexOf(l.blend.mode) + 1) % BLENDS.length]))}
                  ><span class="name">Mode</span><output>{BLEND_NAMES[l.blend.mode]}</output></button
                >
                <label class="row pinnable">
                  <span>Amount</span>
                  <span class="slider"><input type="range" min="0" max="1" step="0.01" value={l.blend.amount} oninput={(e) => changeExtra(() => (l.blend.amount = Number(e.currentTarget.value)))} /></span>
                  <output>{percent(l.blend.amount)}</output>
                  <span></span>
                </label>
              </div>
              <div class="rows"><button class="go danger" onclick={() => removeExtra(l.id)}><span class="name">Remove this layer</span></button></div>
            {/if}
          {:else if here.startsWith('xmodule:')}
            {@const m = extraModule(here.slice('xmodule:'.length))}
            {@const t = m ? extraType(m.type) : undefined}
            {#if m && t}
              <div class="rows">
                {#each t.knobs as k (k.id)}{@render extraKnobRow(m, k)}{/each}
              </div>
              {#if t.inputs?.length}
                <div class="rows">
                  <h3>Inputs</h3>
                  {#each t.inputs as port (port)}{@render goRow(port === 'in' ? 'Input' : port[0].toUpperCase() + port.slice(1), `${cablesTo(m.id, port).length} cables`, `moves:${m.id}.${port}`)}{/each}
                </div>
              {/if}
              {#if !s.extra.layers.some((l) => l.source.id === m.id)}
                <div class="rows"><button class="go danger" onclick={() => removeExtra(m.id)}><span class="name">Remove</span></button></div>
              {/if}
            {/if}
          {:else if here === 'device'}
            <div class="rows">
              {#each ['quality', 'fps', 'brightness'] as k (k)}{@render settingRow(ROW_OF.get(k as One))}{/each}
            </div>
          {:else if here === 'cables'}
            <!-- Every cable, by the knob it moves (the matrix gone, 70.1: the graph and these rows are where cables are). -->
            {#each cableGroups as g (g.mk)}
              {@render goRow(g.label, `${g.n} ${g.n === 1 ? 'cable' : 'cables'}`, g.page)}
            {:else}
              <div class="rows"><div class="go still"><span class="name">No cables</span></div></div>
            {/each}
          {/if}
          {#if !path.length}
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
          {/if}
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
  {#if menuCable}
    {@const c = cableOf({ routes: s.routes, extra: s.extra }, menuCable)}
    {@const id = menuCable}
    {#if c}
      <CableMenu title={cableName(c)} onremove={() => removeCable(id)} onreroute={() => go(`reroute:to:${id}`)} onclose={() => (menuCable = null)} />
    {/if}
  {/if}
  {#if snapshotsOpen}
    <SnapshotsSheet onclose={() => (snapshotsOpen = false)} capture={capturePicture} trackName={np?.name ?? null} {recall} over={!docked} />
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

  /* The graph builder (decision 0056): beside the panel where there is room, the same height. */
  .graph {
    position: absolute;
    top: calc(max(12px, env(safe-area-inset-top)) + 56px);
    left: max(12px, env(safe-area-inset-left));
    right: calc(max(12px, env(safe-area-inset-right)) + 512px);
    bottom: max(12px, env(safe-area-inset-bottom));
    overflow: hidden;
    border-radius: 14px;
    background: rgb(14 16 21 / 0.86);
    pointer-events: auto;
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

  .rows + .rows {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgb(255 255 255 / 0.12);
  }

  /* Where the panel is (decision 0056): back, then the pages from the top. */
  .crumbs {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    margin-bottom: 4px;
    font-size: 0.85rem;
    color: rgb(255 255 255 / 0.6);
  }

  .crumbs .back {
    display: grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
    cursor: pointer;
  }

  .trail {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trail .sep {
    margin: 0 5px;
  }

  .trail .here {
    color: #fff;
    font-weight: 600;
  }

  /* A row that opens a page: its name, a line of what it is now, the chevron. */
  .go {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    padding: 6px 4px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: #fff;
    font: inherit;
    font-size: 0.92rem;
    text-align: left;
    cursor: pointer;
  }

  .go:hover {
    background: rgb(255 255 255 / 0.06);
  }

  .go.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .go.still {
    cursor: default;
  }

  .go.still:hover {
    background: none;
  }

  .go .name {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .go small {
    color: rgb(255 255 255 / 0.55);
    font-size: 0.8rem;
  }

  .go output {
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.75);
  }

  .go.add .name {
    color: var(--accent);
  }

  /* A knob: its row, and what moves it (how many routes, a page of them). */
  .knob {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .knob > .row {
    flex: 1;
    min-width: 0;
  }

  .moves {
    display: flex;
    align-items: center;
    height: 30px;
    padding: 0 2px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: rgb(255 255 255 / 0.35);
    font: inherit;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .moves.on {
    color: var(--accent);
  }

  /* A knob nothing can move keeps the button's room, so the sliders line up. */
  .moves-gap {
    flex: none;
    width: 30px;
  }

  .danger {
    color: var(--danger, #ff6b6b);
  }

  .rows h3,
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
