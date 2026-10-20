// The visuals' frame loop on the patch store (decision 0057, the switch, S2): each frame the one engine over the
// store's patch (flattened, the same object while only values change: engine/live-patch.ts), the picture drawn by the
// renderer and its compositor, measured, the measures back into the patch's sidechains. What the visuals change as they
// play (a moment, a new track's look: the patch's Dice) goes back into the store as a performance (no undo step); the
// dice the owner taps as an edit (one undo step). The picture's own pace, framing, colours and stirring as the page's
// loop had them (Visuals.svelte, where the old loop stays until this one is labbed).
//
// Read from the device's own settings: Quality, FPS, Brightness (`device`). Everything else is the patch's.

import { coverUrl } from '../api';
import { reducedMotion } from '../motion';
import { playing } from '../remote.svelte';
import { wavesMotion, zoomsMotion } from './aspects';
import { beatsOf } from './beats';
import { chainNow } from './chain-params';
import { settingsReport } from './effects';
import { fluidGains } from './engine/fluid-gains';
import { newEngine, stepEngine, type EngineFrame } from './engine/graph';
import { LivePatch } from './engine/live-patch';
import { randomize, randomizeCables, scopeNamed, type Randomized } from './engine/randomize';
import { settingsOfPatch } from './engine/settings-view';
import { holding, leversOf, STANDING, type Levers } from './engine/sidechain';
import { between, coloured, evolved, flameOf, measuring, partsAt, PRESET_KEYS, seedOf, type ColourMode, type Flame, type Preset } from './flame';
import type { FrameStats, MeterValues } from './meters';
import { musicNow, stopMusic } from './music.svelte';
import { newNormalState, signalsFor } from './normal';
import { coverPalette, type PaletteColour } from './palette';
import type { ModuleType, SignalRef } from './patch';
import { isDevice, RANDOMIZE_SCOPES, type PatchV2 } from './ports';
import { precheckingPatch, type Checked } from './precheck';
import { ceilingOf } from './quality';
import { FlameRenderer, type PictureInputs } from './render';
import { A, B, T } from './routes';
import { FLUID_FULL, type VisualSettings } from './settings.svelte';
import { signalsOf } from './signals';
import { newStirring, stir } from './stirring';

/** A new track's look blends in over this long; one the dice made, quicker (Visuals.svelte's). */
const BLEND_MS = 6000;
const ANOTHER_MS = 1500;
/** The fewest points the device's own pace steps down to. */
const MIN_POINTS = 1 << 14;
/** How many seeds the dice try against the track before taking the least over its limits. */
const TRIES = 8;
const SHAPING = PRESET_KEYS.filter((k) => k !== 'spread' && k !== 'purity');
const BY_DEGREES = new Set<string>(['detail', 'twist', 'warp', 'hue', 'lightness', 'vividness']);
const modeOf = (family: number): ColourMode => (family >= 5 ? 'fire' : family >= 4 ? 'cover' : 'rainbow');

/** A module of a patch by its flattened id (`show/curve`: the Show device's Curve), devices opened. */
function moduleAt(p: PatchV2, id: string) {
  const path = id.split('/');
  let modules = p.modules;
  for (const step of path.slice(0, -1)) {
    const d = modules.find((m) => m.id === step);
    if (!d || !isDevice(d)) return undefined;
    modules = d.patch.modules;
  }
  return modules.find((m) => m.id === path[path.length - 1]);
}

/** New knob values into a patch, by flattened module id. */
function applyKnobs(p: PatchV2, values: Randomized) {
  for (const [id, knobs] of Object.entries(values)) {
    const m = moduleAt(p, id);
    if (m) Object.assign(m.knobs, knobs);
  }
}



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

/** What a roll reaches: a scope of RANDOMIZE_SCOPES, or modules by id (a module's own Randomize). */
export type Roll = (typeof RANDOMIZE_SCOPES)[number] | { module: string[] };

/** What the loop is given. */
export type LoopHost = {
  canvas: HTMLCanvasElement;
  /** The patch store (visuals-patch.svelte.ts). */
  store: { readonly patch: PatchV2; perform(f: (p: PatchV2) => PatchV2 | void): void; change(f: (p: PatchV2) => PatchV2 | void, key?: string | null): void };
  /** The device's own settings (Quality, FPS, Brightness), and what a patch has no knob for (settings.svelte.ts). */
  device: () => VisualSettings;
  /** Every module type by its type (the engine's, the sources', the chain's as v2 has them). */
  types: Record<string, ModuleType>;
  /** The picture side's types (picture-graph.ts v2Types). */
  pictureTypes: Parameters<FlameRenderer['setPatch']>[1];
  /** Four times a second: frames a second, what the device draws, what the sidechains hold back. */
  onPace?: (pace: { fps: number; points: number; steps: number; cleared: number }) => void;
  onHeld?: (held: { flash: boolean; speed: boolean; wild: boolean }) => void;
  /** The engine's frame, four times a second (the panel's live values and meters). */
  onFrame?: (frame: EngineFrame, levers: Levers) => void;
  /** The GPU cannot draw (none, or it took the context away). */
  onUnsupported?: () => void;
};

export class VisualsLoop {
  #raf = 0;
  #stop: (() => void) | null = null;
  #pictureWanted: ((picture: string | null) => void) | null = null;
  #rollWanted: { scope: Roll; seed: number | null; edit: string } | null = null;

  constructor(private host: LoopHost) {}

  /** A roll of the dice (the panel's): over `scope` (everything, a category, the cables; or these modules: a module's
   *  own Randomize), a fresh seed, checked against the track, an edit (one undo step). */
  roll(scope: Roll = 'everything') {
    this.#rollWanted = { scope, seed: null, edit: 'dice' };
  }

  /** The playing track's own look again (its seed, everything not pinned), as a new track rolls it; an edit. */
  resetToSeed() {
    this.#rollWanted = { scope: 'everything', seed: -1, edit: 'reset' };
  }

  /** The middle of the picture at 16:9, 160 x 90, as a JPEG (a snapshot's thumbnail), from the next frame drawn. */
  picture(): Promise<string | null> {
    return new Promise((done) => (this.#pictureWanted = done));
  }

  stop() {
    cancelAnimationFrame(this.#raf);
    this.#stop?.();
    this.#stop = null;
  }

  start() {
    const host = this.host;
    const canvas = host.canvas;
    const devicePixels = () => innerWidth * innerHeight * Math.min(devicePixelRatio || 1, 2) ** 2;
    const pace = keptPace();
    let renderer: FlameRenderer | null = null;
    try {
      renderer = new FlameRenderer(canvas, Math.min(Math.round(2 ** ceilingOf(host.device().quality, devicePixels()).points), pace?.points ?? Infinity));
    } catch (e) {
      console.warn('Visuals:', e);
      host.onUnsupported?.();
      return;
    }

    // The patch as the engine runs it, and what it gives the picture's side and the meters as settings.
    const live = new LivePatch();
    let rebuildsSeen = -1;
    let storeSeen: PatchV2 | null = null;
    let how = host.device();
    const engine = newEngine();
    const normal = newNormalState();
    const stirring = newStirring();
    let meterValues: MeterValues | undefined;
    let meteredAt = 0;

    // The flame: blended from (`from`) to the look's (`to`); `shown` the last drawn. The look as the Flame's knobs
    // make it, made anew when they change; a change blends in as `nextBlend` asks (a new track's look, the dice), else
    // over a second and a half, one dragged by degrees at once.
    let from: Flame | null = null;
    let to: Flame | null = null;
    let shown: Flame | null = null;
    let blendStart = 0;
    let blendMs = BLEND_MS;
    let nextBlend: number | null = null;
    let lookCame: FrameStats['look'] = null;
    let base: Flame | null = null;
    let made: Record<string, number> | null = null;
    let madeColours = '';
    let cover: { key: string; colours: PaletteColour[] | null } | null = null;
    let fire = 0;
    let key: string | null | undefined;
    let looked = false;

    // Values moving in after a roll or a new track's look (the one morph, 0057): each knob from where it was to where
    // the roll put it, over the blend; a step at once, and the Flame's look as a flame blends (`from` → `to`).
    let easing: { from: Randomized; to: Randomized; start: number; ms: number } | null = null;
    const ease = (flat: PatchV2, now: number) => {
      if (!easing) return;
      const k = Math.min(1, (now - easing.start) / easing.ms);
      const s = k * k * (3 - 2 * k);
      for (const [id, knobs] of Object.entries(easing.to)) {
        const m = flat.modules.find((x) => x.id === id);
        if (!m) continue;
        for (const [knob, v] of Object.entries(knobs)) {
          const was = easing.from[id]?.[knob];
          if (typeof was === 'number' && typeof v === 'number') m.knobs[knob] = was + (v - was) * s;
        }
      }
      if (k >= 1) easing = null;
    };
    /** Values into the store as the visuals play them (a performance) or as an edit (the dice), eased in. */
    const apply = (values: Randomized, cables: Record<string, PatchV2['cables'][number]['value']>, ms: number, edit: string | null, now: number) => {
      const flat = live.update(host.store.patch);
      const fromValues: Randomized = {};
      const toValues: Randomized = {};
      for (const [id, knobs] of Object.entries(values)) {
        const m = flat.modules.find((x) => x.id === id);
        if (!m || m.type === 'flame') continue;
        const t = host.types[m.type];
        const eased = Object.keys(knobs).filter((k) => t?.knobs.find((x) => x.id === k.replace(/\.\d+$/, ''))?.kind !== 'step');
        if (!eased.length) continue;
        fromValues[id] = Object.fromEntries(eased.map((k) => [k, m.knobs[k]]));
        toValues[id] = Object.fromEntries(eased.map((k) => [k, knobs[k]]));
      }
      const f = (p: PatchV2) => {
        applyKnobs(p, values);
        for (const c of p.cables) if (c.id in cables) c.value = cables[c.id];
      };
      if (edit) host.store.change(f, edit);
      else host.store.perform(f);
      if (ms > 0) easing = { from: fromValues, to: toValues, start: now, ms };
    };

    // A roll being worked out, over frames (the flame's survey yields): a new track's (the patch's Dice: a performance)
    // or the dice's (tried against the track, an edit).
    let rolling: Generator<void, { values: Randomized; cables: Record<string, PatchV2['cables'][number]['value']> }> | null = null;
    let rollingAs: { edit: string | null; ms: number } = { edit: null, ms: BLEND_MS };
    const rollOf = function* (seed: number, roll: Roll, check: { key: string; position: number } | null) {
      const patch = host.store.patch;
      const scope = typeof roll === 'string' ? roll : null;
      const ids = typeof roll === 'string' ? null : new Set(roll.module);
      const knobsScope = scope ? scopeNamed(scope, host.types) : (m: { id: string }) => ids!.has(m.id);
      const withCables = scope === 'everything' || scope === 'cables';
      const values: Randomized = knobsScope ? yield* randomize(patch, host.types, { seed, scope: knobsScope }) : {};
      const cables = withCables ? randomizeCables(patch, host.types, { seed }) : {};
      if (!check) return { values, cables };
      // The dice: this seed and the next ones, each checked against the track (its sidechains taken out), the first
      // within the limits taken, else the least over them.
      const [source, id] = [check.key.slice(0, check.key.indexOf('/')), check.key.slice(check.key.indexOf('/') + 1)];
      const signals = signalsOf(source, id);
      if (!signals) return { values, cables };
      const wild = host.store.patch.macros.find((x) => x.id === 'wild')?.value ?? 0.5;
      const limits = Object.fromEntries(Object.entries(STANDING).map(([k, v]) => [k, v * 2 ** (2 * (wild - 0.5))]));
      let best: { values: Randomized; cables: typeof cables; by: number } | null = null;
      for (let t = 0; t < TRIES; t++) {
        const tryValues: Randomized = t === 0 ? values : knobsScope ? yield* randomize(patch, host.types, { seed: seed + t, scope: knobsScope }) : {};
        const tryCables = t === 0 ? cables : withCables ? randomizeCables(patch, host.types, { seed: seed + t }) : {};
        const candidate = structuredClone(patch);
        applyKnobs(candidate, tryValues);
        for (const c of candidate.cables) if (c.id in tryCables) c.value = tryCables[c.id];
        const checked: Checked = yield* precheckingPatch(candidate, how, signals, beatsOf(source, id) ?? null, check.position, limits);
        if (checked.ok) return { values: tryValues, cables: tryCables };
        const by = checked.over.reduce((sum, m) => sum + ((checked.worst as Record<string, number>)[m] ?? 0) / ((limits as Record<string, number>)[m] ?? 1), 0);
        if (!best || by < best.by) best = { values: tryValues, cables: tryCables, by };
      }
      return best ?? { values, cables };
    };

    // The fluid's pushes: where the Flame's parts are on the screen, measured four times a second while it flows.
    let partsWhere: ([number, number] | null)[] = [];
    let partsMeasured = 0;
    let passes = Math.min(ceilingOf(how.quality, devicePixels()).steps, pace?.passes ?? Infinity);
    let last = performance.now();
    let drawnAt = 0;
    let refresh = 1000 / 60;
    let lastTick = performance.now();
    let late = 0;
    let drawn = 0;
    let judged = performance.now();
    let fit = Math.min(1, Math.max(0.6, pace?.fit ?? 1));
    let calm = 0;
    let wait = 6;
    let ceiling = pace?.ceiling && pace.ceiling.until > Date.now() ? pace.ceiling : null;
    let upTo = 0;
    const costOf = (points: number, steps: number, share: number) => points * steps * share * share;
    let settlingFrames = 0;
    let steppedUp = 0;
    let steppedDown = 0;
    let frame: { center: [number, number]; size: number; coverage: number } | null = null;
    let measured = 0;
    let measure: ReturnType<typeof measuring> | null = null;
    let counted = { since: performance.now(), frames: 0 };
    let fps = 0;
    let reported = 0;
    let heldBefore = { flash: false, speed: false, wild: false };

    const loop = (now: number) => {
      this.#raf = requestAnimationFrame(loop);
      if (!renderer) return;
      const device = host.device();
      const tick = now - lastTick;
      lastTick = now;
      if (tick > 4 && tick < 21) refresh += (tick - refresh) * 0.05;
      const every = Math.max(1, Math.ceil((1000 / device.fps - 1) / refresh));
      const frameMs = every * refresh;
      if (now - drawnAt < frameMs - refresh / 2) return;
      const spent = now - drawnAt;
      drawnAt = now;
      counted.frames++;
      if (now - counted.since >= 1000) {
        fps = Math.round((counted.frames * 1000) / (now - counted.since));
        counted = { since: now, frames: 0 };
        host.onPace?.({ fps, points: renderer.pointCount, steps: passes, cleared: renderer.scrubbed });
      }
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      if (renderer.lost) {
        console.warn('Visuals: the GPU took the context away', { points: renderer.pointCount, steps: passes, precision: renderer.precision });
        renderer = null;
        host.onUnsupported?.();
        return;
      }
      renderer.tiles();

      // The patch: as the engine runs it, its values eased where a roll moves them; as settings for the picture's side.
      const patch = host.store.patch;
      const flat = live.update(patch);
      ease(flat, now);
      if (patch !== storeSeen || easing) {
        storeSeen = patch;
        how = { ...settingsOfPatch(flat, device), quality: device.quality, fps: device.fps, brightness: device.brightness };
      }
      if (live.rebuilds !== rebuildsSeen) {
        rebuildsSeen = live.rebuilds;
        try {
          renderer.setPatch(patch, host.pictureTypes);
          if (renderer.errors.length) console.warn(`Visuals: ${renderer.errors.join(' · ')}`);
        } catch (e) {
          console.warn('Visuals: the patch could not be drawn as a graph', e);
        }
      }

      const m = musicNow(now, how.lighting, flat.modules.some((x) => x.type === 'waves'));
      key = m.key;
      // The dice the owner tapped: a roll over its scope, checked against the track, as an edit.
      if (this.#rollWanted && !rolling) {
        const want = this.#rollWanted;
        this.#rollWanted = null;
        // Reset to seed: the track's own, unchecked (a new track's look is not checked either); the dice: a fresh one.
        const reset = want.seed === -1;
        const seed = reset ? seedOf(m.key ?? 'cratarr') : Math.floor(Math.random() * 0x100000000);
        rolling = rollOf(seed, want.scope, reset || !m.key ? null : { key: m.key, position: m.position });
        rollingAs = { edit: want.edit, ms: reset ? BLEND_MS : ANOTHER_MS };
        lookCame = reset ? 'track' : 'dice';
      }
      if (rolling) {
        const until = performance.now() + 4;
        let next = rolling.next();
        while (!next.done && performance.now() < until) next = rolling.next();
        if (next.done) {
          rolling = null;
          apply(next.value.values, next.value.cables, rollingAs.ms, rollingAs.edit, now);
          nextBlend = rollingAs.ms;
        }
      }

      // The colours: the cover's asked for as a track starts, where they are wanted.
      const mode = modeOf(how.family);
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
      }
      const covered = mode === 'cover' && cover?.key === m.key ? cover.colours : null;
      fire += ((mode === 'fire' ? 1 : 0) - fire) * Math.min(1, dt / 1.5);
      const lookOf = (k: string) => (how as unknown as Record<string, number>)[k];
      const colours = `${mode}/${covered ? m.key : ''}`;
      const changed = !made ? SHAPING : SHAPING.filter((k) => made![k] !== lookOf(k));
      if (changed.length || colours !== madeColours) {
        if (changed.length) base = flameOf(Object.fromEntries(PRESET_KEYS.map((k) => [k, lookOf(k)])) as Preset, false);
        const coverCame = !changed.length && madeColours.startsWith('cover/') && colours.startsWith('cover/') && from;
        const blend = nextBlend ?? (changed.every((k) => BY_DEGREES.has(k)) && colours === madeColours ? 0 : coverCame ? null : ANOTHER_MS);
        if (blend && shown) {
          from = shown;
          blendStart = now;
          blendMs = blend;
        }
        made = Object.fromEntries(SHAPING.map((k) => [k, lookOf(k)]));
        madeColours = colours;
        to = coloured(base!, mode, covered);
        if (!from) shown = to;
      }
      nextBlend = null;

      // The engine over the patch, its sidechains keyed by the meters; the Flame's parts its points.
      const still = reducedMotion();
      let signals: ((ref: SignalRef) => number) | null = null;
      const read = (ref: SignalRef) => {
        if (!signals) {
          m.track[T.track] = engine.signals[T.track];
          signals = signalsFor(normal, m, now);
        }
        return signals(ref);
      };
      // The canvas at the device's quality and the pace's fit, and the compose targets at the buffer's size (kept with
      // or without a flame): the canvas's size, and whether the buffer's changed.
      const sized = (): [number, number, boolean, ReturnType<typeof ceilingOf>] => {
        const dpr = Math.min(devicePixelRatio || 1, 2);
        const top = ceilingOf(device.quality, canvas.clientWidth * canvas.clientHeight * dpr * dpr);
        const r = top.resolution * fit;
        const w = Math.max(1, Math.round(canvas.clientWidth * dpr * r));
        const h = Math.max(1, Math.round(canvas.clientHeight * dpr * r));
        if (canvas.width !== w || canvas.height !== h) {
          canvas.width = w;
          canvas.height = h;
        }
        const scale = Math.min(1, Math.sqrt(600_000 / (w * h)));
        const bw = Math.max(1, Math.round(w * scale));
        const bh = Math.max(1, Math.round(h * scale));
        const resized = renderer!.width !== bw || renderer!.height !== bh;
        renderer!.resize(bw, bh);
        return [w, h, resized, top];
      };
      const stepped = stepEngine(engine, flat, host.types, m, read, how, dt, { still, now, fluidFull: FLUID_FULL, meters: meterValues, flamePoints: partsWhere });
      const levers = leversOf(stepped.outputs);
      // The layers past the flame and the picture's own loop: the compositor's, from the patch (setPatch), each
      // module's knobs as the engine moves them; where a roll is easing in, where each knob is going.
      const inputs: PictureInputs = {
        knobs: (id) => stepped.knobs[id],
        target: (id, knob) => easing?.to[id]?.[knob] ?? stepped.knobs[id]?.[knob] ?? flat.modules.find((x) => x.id === id)?.knobs[knob] ?? 0,
        frame: { dt, time: now / 1000, beat: m.beat, signals: read, waveform: m.wave ?? null },
      };
      // A patch with no Cratarr device (a blank slate): no flame, whatever else reaches the Screen drawn from black
      // (the renderer blanks it when nothing does).
      if (!stepped.aspects) {
        const [w, h] = sized();
        renderer.frame(passes, null, w, h, inputs);
        return;
      }
      const d = stepped.aspects;

      // A new track's look, as the patch's Dice roll it: worked out over the next frames, a performance.
      for (const r of stepped.dice) {
        if (rolling) break;
        rolling = rollOf(r.seed, r.scope, null);
        // The first look shows at once; each next one blends in over a new track's time.
        rollingAs = { edit: null, ms: looked ? BLEND_MS : 0 };
        looked = true;
        lookCame = 'track';
      }
      // Each step whose Next a trigger fired (0057 §7e: a Gate's, in sight): its next step, a performance. The rack as
      // it is: an effect is a module, not a step, so nothing here swaps one.
      const moved: Randomized = {};
      for (const r of stepped.nexts) {
        const mod = flat.modules.find((x) => x.id === r.module);
        const knob = mod && host.types[mod.type]?.knobs.find((k) => k.id === r.knob);
        if (!mod || !knob || patch.pins.includes(`${r.module}.${r.knob}`)) continue;
        const v = Math.round(mod.knobs[r.knob] ?? knob.default);
        (moved[r.module] ??= {})[r.knob] = v + 1 > knob.max ? knob.min : v + 1;
      }
      if (Object.keys(moved).length) {
        apply(moved, {}, 0, null, now);
        nextBlend = ANOTHER_MS;
        lookCame = 'next';
      }

      if (!to) return;
      const k = from ? Math.min(1, (now - blendStart) / blendMs) : 1;
      shown = from && k < 1 ? between(from, to, k * k * (3 - 2 * k)) : to;
      if (k >= 1) from = null;
      const drawnFlame = evolved(shown, d.parts, lookOf('purity'), how.spread);
      if (!measure && now - measured > 500) {
        measured = now;
        measure = frame ? measuring(drawnFlame) : measuring(drawnFlame, 6000, 6000);
      }
      const measuredNow = measure?.next();
      if (measuredNow?.done) {
        measure = null;
        const seen = measuredNow.value;
        if (seen) {
          if (!frame) frame = seen;
          else {
            const kk = 1 - Math.exp(-0.5 / 1.5);
            frame = {
              center: [frame.center[0] + (seen.center[0] - frame.center[0]) * kk, frame.center[1] + (seen.center[1] - frame.center[1]) * kk],
              size: Math.exp(Math.log(frame.size) + (Math.log(seen.size) - Math.log(frame.size)) * kk),
              coverage: frame.coverage + (seen.coverage - frame.coverage) * kk,
            };
          }
        }
      }
      renderer.setFlame(frame ? { ...drawnFlame, center: frame.center, size: frame.size } : drawnFlame);

      const [w, h, resized, top] = sized();

      // The fluid, stirred: its pushes at what reaches its `at` (the Flame's parts by default), each worth what the
      // patch's cables into Stir, Flow and Swirl give it.
      const fluidNow = d.fluid;
      if (fluidNow > 0 && now - partsMeasured > 250) {
        partsMeasured = now;
        const cam = frame ?? drawnFlame;
        const aspect = w / h;
        const cos = Math.cos(d.turn);
        const sin = Math.sin(d.turn);
        // In place: the engine took this list as the Flame's points.
        partsWhere.length = 0;
        for (const p of partsAt(drawnFlame)) {
          if (!p) {
            partsWhere.push(null);
            continue;
          }
          let x = ((p[0] - cam.center[0]) * d.scale) / cam.size;
          let y = ((p[1] - cam.center[1]) * d.scale) / cam.size;
          [x, y] = [cos * x - sin * y, sin * x + cos * y];
          if (aspect > 1) x /= aspect;
          else y *= aspect;
          partsWhere.push([x * 0.5 + 0.5, y * 0.5 + 0.5]);
        }
      }
      const fluidAt = Object.entries(stepped.pointsAt)[0]?.[1];
      const beatNow = m.playing && !still ? m.beat : null;
      const wavesNow = still ? 0 : d.given[A.waves] * levers.pace;
      const gains = fluidGains(flat);
      const stirred = stir(stirring, {
        t: now / 1000,
        dt,
        amount: fluidNow,
        parts: (fluidAt ?? partsWhere) as ([number, number] | null)[],
        turns: d.rates,
        lights: d.hits,
        level: d.bands.map((v) => v[B.level]),
        continuity: still ? how.continuity.map(() => 1) : how.continuity,
        hits: m.playing && m.hits ? m.hits.map((hh) => (hh.index < 0 ? null : { index: hh.index, strength: hh.weight, nextIn: hh.nextIn })) : [],
        beat: beatNow ? { ...beatNow, nextIn: beatNow.length - beatNow.since } : null,
        downbeat: still ? 0 : d.track[T.downbeat],
        section: still ? 0 : d.track[T.section],
        loudness: d.lit,
        swell: d.track[T.swell],
        bps: m.bps,
        waves: wavesNow,
        position: m.position,
        center: [0.5, 0.5],
        gain: (signal, band, aspect, x) => (how as unknown as Record<string, number>)[aspect] * gains(signal, band, aspect as 'stir', x),
      });

      renderer.frame(
        passes,
        chainNow(d, { how, levers, still, ceiling: device.brightness, coverage: frame ? frame.coverage : null, fire, position: m.position, beat: beatNow, forces: stirred.forces, flow: stirred.flow, dt, speed: how.speed }),
        w,
        h,
        inputs,
      );
      renderer.measure(w, h, now / 1000);
      const stats: Omit<FrameStats, 'tiles'> = {
        t: now / 1000,
        extra: (() => {
          const fx = settingsReport(how, d.effects, levers, still, d.routed);
          const zooms = still ? 0 : zoomsMotion(d.tunnel * levers.pace, d.dive * levers.pace);
          return { ...fx, motion: (fx.motion ?? 0) + wavesMotion(wavesNow, d.fluid) + zooms };
        })(),
        turnRates: renderer.diveTurnRate ? d.rates.map((rr) => rr + renderer!.diveTurnRate) : d.rates,
        shape: d.shape,
        weights: d.parts.map((p) => p.weight ?? 1),
        hue: d.parts.map((p) => p.hue),
        spread: d.parts.map((p) => p.spread ?? 1),
        scale: d.scale,
        saturation: d.saturation,
        look: lookCame,
        symmetry: d.nexts.symmetry,
      };
      lookCame = null;
      renderer.meterAdd(stats);
      if (now - meteredAt >= 250) {
        meterValues = renderer.meterValues;
        meteredAt = now;
        const held = { flash: holding(levers, 'swing'), speed: holding(levers, 'pace'), wild: (['swing', 'pace', 'evolution', 'hue', 'size', 'gap'] as const).some((kk) => holding(levers, kk)) };
        if (held.flash !== heldBefore.flash || held.speed !== heldBefore.speed || held.wild !== heldBefore.wild) {
          heldBefore = held;
          host.onHeld?.(held);
        }
      }
      if (now - reported >= 250) {
        reported = now;
        host.onFrame?.(stepped, levers);
      }
      if (this.#pictureWanted) {
        const done = this.#pictureWanted;
        this.#pictureWanted = null;
        done(pictureOf(canvas));
      }

      // The device's pace (Visuals.svelte's): under what Quality allows, holding the FPS setting rather than settling
      // under it; a step down a second at most while frames come late, a step back up after `wait` calm seconds.
      const most = Math.round(2 ** top.points);
      if (renderer.pointCount > most) renderer.setPoints(most);
      passes = Math.min(passes, top.steps);
      if (renderer.settling || resized) settlingFrames = 2;
      else if (settlingFrames > 0) settlingFrames--;
      else {
        drawn++;
        if (spent > frameMs * 1.08 + 2) late++;
      }
      if (now - judged >= 1000) {
        let stepped2 = false;
        if (drawn >= 5 && late * 5 > drawn) {
          calm = 0;
          if (now - steppedUp < 5000 && upTo) {
            wait = Math.min(60, wait * 2);
            ceiling = { cost: upTo, until: Date.now() + 5 * 60_000 };
          }
          upTo = 0;
          steppedDown = now;
          if (passes > 1) passes--;
          else if (fit > 0.6) fit = Math.max(0.6, fit * 0.85);
          else if (renderer.pointCount > MIN_POINTS) renderer.setPoints(Math.max(MIN_POINTS, renderer.pointCount >> 1));
          stepped2 = true;
        } else if (drawn >= 5 && late === 0 && ++calm >= wait) {
          calm = 0;
          const points = renderer.pointCount;
          const next =
            points < most ? { points: Math.min(most, points * 2), passes, fit } : fit < 1 ? { points, passes, fit: Math.min(1, fit / 0.85) } : passes < top.steps ? { points, passes: passes + 1, fit } : null;
          if (ceiling && ceiling.until < Date.now()) ceiling = null;
          const cost = next ? costOf(next.points, next.passes, next.fit) : 0;
          if (next && (!ceiling || cost < ceiling.cost)) {
            steppedUp = now;
            upTo = cost;
            if (next.points !== points) renderer.setPoints(next.points);
            fit = next.fit;
            passes = next.passes;
            stepped2 = true;
          }
        }
        if (stepped2) keepPace({ points: renderer.pointCount, passes, fit, ceiling });
        if (now - steppedDown > 60_000) wait = 6;
        late = 0;
        drawn = 0;
        judged = now;
      }
    };
    this.#raf = requestAnimationFrame(loop);
    this.#stop = () => {
      renderer?.dispose();
      stopMusic();
    };
  }
}

/** The middle of the picture at 16:9, 160 x 90, as a JPEG (Visuals.svelte's). */
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
