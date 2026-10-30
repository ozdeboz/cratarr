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
import { newEngine, stepEngine, type EngineFrame } from './engine/graph';
import { LivePatch } from './engine/live-patch';
import { rollOf, type Roll, type Rolled } from './engine/dice';
import type { Randomized } from './engine/randomize';
import { frameStats } from './engine/frame-stats';
import { holding, leversOf, type Levers } from './engine/sidechain';
import { seedOf } from './flame';
import type { FrameStats, MeterValues } from './meters';
import { musicNow, stopMusic } from './music.svelte';
import { newNormalState, signalsFor } from './normal';
import { coverImage, coverPalette, type PaletteColour } from './palette';
import type { ModuleType, SignalRef } from './patch';
import type { PatchV2 } from './ports';
import { ceilingOf } from './quality';
import { FlameRenderer, type PictureInputs } from './render';
import { T } from './routes';
import { BANDS, FLUID_FULL, type VisualSettings } from './settings.svelte';
import { asSet } from './v2';
import { applyEdit } from './patch-edits';

/** A new track's look blends in over this long; one the dice made, quicker (Visuals.svelte's). */
const BLEND_MS = 6000;
const ANOTHER_MS = 1500;
/** The fewest points the device's own pace steps down to. */
const MIN_POINTS = 1 << 14;




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

export type { Roll } from './engine/dice';

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
  /** Why some of the patch is not drawn (each a sentence; none: all of it), as a patch's structure changes. */
  onErrors?: (errors: string[]) => void;
};

export class VisualsLoop {
  #raf = 0;
  #stop: (() => void) | null = null;
  #pictureWanted: ((picture: string | null) => void) | null = null;
  #rollWanted: { scope: Roll; seed: number | null; edit: string } | null = null;
  #renderer: FlameRenderer | null = null;

  constructor(private host: LoopHost) {}

  /** A roll of the dice (the panel's): over `scope` (everything, a category, the cables; or these modules: a module's
   *  own Randomize), a fresh seed, checked against the track, an edit (one undo step). */
  roll(scope: Roll = 'macros') {
    this.#rollWanted = { scope, seed: null, edit: 'dice' };
  }

  /** The playing track's own look again (its seed, everything not pinned), as a new track rolls it; an edit. */
  resetToSeed() {
    this.#rollWanted = { scope: 'macros', seed: -1, edit: 'reset' };
  }

  /** The middle of the picture at 16:9, 160 x 90, as a JPEG (a snapshot's thumbnail), from the next frame drawn. */
  picture(): Promise<string | null> {
    return new Promise((done) => (this.#pictureWanted = done));
  }

  /** The observed module's picture drawn small into `canvas` (the graph's preview, render.ts): null where it shows,
   *  else why it has none. */
  thumbnail(module: string, canvas: HTMLCanvasElement): string | null {
    return this.#renderer ? this.#renderer.thumbnail(module, canvas) : 'Not running.';
  }

  stop() {
    cancelAnimationFrame(this.#raf);
    this.#renderer = null;
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
    this.#renderer = renderer;

    // The patch as the engine runs it, and what it gives the picture's side and the meters as settings.
    const live = new LivePatch();
    let rebuildsSeen = -1;
    let storeSeen: PatchV2 | null = null;
    // Music's Hold, from its module's knobs as set: worked out as the patch changes.
    let lighting: number[] = [];
    const engine = newEngine();
    const normal = newNormalState();
    let meterValues: MeterValues | undefined;
    let meteredAt = 0;

    // The Flame's look, framing and place on the screen are its node's (engine/flame-node.ts); the loop gives it what
    // only the page has: the cover's colours (fetched here once its Colours want them) and how long this frame's change
    // glides, as its cause asks (a new track's look, the dice, a step moved on: `nextBlend`).
    let nextBlend: number | null = null;
    let lookCame: FrameStats['look'] = null;
    let cover: { key: string; colours: PaletteColour[] | null } | null = null;
    let wantsCover = false;
    let coverPicture: { key: string; image: HTMLImageElement | null } | null = null;
    let key: string | null | undefined;
    let looked = false;

    // Values moving in after a roll or a new track's look (the one morph, 0057): each knob from where it was to where
    // the roll put it, over the blend; a step at once, and the Flame's look as a flame blends (`from` → `to`).
    let easing: { from: Randomized; to: Randomized; macrosFrom: Record<string, number>; macrosTo: Record<string, number>; start: number; ms: number } | null = null;
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
      for (const [id, v] of Object.entries(easing.macrosTo)) {
        const x = flat.macros.find((y) => y.id === id);
        const was = easing.macrosFrom[id];
        if (x && typeof was === 'number') x.value = was + (v - was) * s;
      }
      if (k >= 1) easing = null;
    };
    /** A knob's value as the morph under way shows it at `now`, else `stored`. */
    const shownNow = (id: string, knob: string, stored: number, now: number): number => {
      const was = easing?.from[id]?.[knob];
      const to = easing?.to[id]?.[knob];
      if (!easing || typeof was !== 'number' || typeof to !== 'number') return stored;
      const k = Math.min(1, (now - easing.start) / easing.ms);
      return was + (to - was) * k * k * (3 - 2 * k);
    };
    /** Values into the store as the visuals play them (a performance) or as an edit (the dice, a new track's look), eased
     *  in. */
    const apply = (values: Randomized, cables: Record<string, PatchV2['cables'][number]['value']>, ms: number, edit: string | null, now: number, macros: Record<string, number> = {}) => {
      const flat = live.update(host.store.patch);
      const fromValues: Randomized = {};
      const toValues: Randomized = {};
      for (const [id, knobs] of Object.entries(values)) {
        const m = flat.modules.find((x) => x.id === id);
        if (!m || m.type === 'flame') continue;
        const t = host.types[m.type];
        const eased = Object.keys(knobs).filter((k) => t?.knobs.find((x) => x.id === k.replace(/\.\d+$/, ''))?.kind !== 'step');
        if (!eased.length) continue;
        // From where the knob is shown now: mid-morph, between the last roll's start and end (so a Dice with
        // a Glide that fires again before its morph is done carries on from there, with no jump).
        fromValues[id] = Object.fromEntries(eased.map((k) => [k, shownNow(id, k, m.knobs[k], now)]));
        toValues[id] = Object.fromEntries(eased.map((k) => [k, knobs[k]]));
      }
      // A macro's value glides in the one morph (0077), unless it maps a step (Form, Symmetry, a Lens, Colours: a choice
      // changes at once, the Flame's own glide carrying its look over).
      const mapsStep = (x: { targets: { module: string; knob: string }[] }, depth = 0): boolean =>
        x.targets.some((t) => {
          const inner = t.knob === 'value' ? flat.macros.find((y) => y.id === t.module) : undefined;
          if (inner) return depth < 4 && mapsStep(inner, depth + 1);
          const type = flat.modules.find((m) => m.id === t.module)?.type ?? '';
          return host.types[type]?.knobs.find((k) => k.id === t.knob.replace(/\.\d+$/, ''))?.kind === 'step';
        });
      const macrosFrom: Record<string, number> = {};
      const macrosTo: Record<string, number> = {};
      for (const [id, v] of Object.entries(macros)) {
        const x = flat.macros.find((y) => y.id === id);
        if (!x || mapsStep(x)) continue;
        const was = easing?.macrosFrom[id];
        const to = easing?.macrosTo[id];
        const k = easing ? Math.min(1, (now - easing.start) / easing.ms) : 1;
        macrosFrom[id] = easing && typeof was === 'number' && typeof to === 'number' ? was + (to - was) * k * k * (3 - 2 * k) : x.value;
        macrosTo[id] = v;
      }
      const f = (p: PatchV2) => applyEdit(p, { kind: 'values', knobs: values, cables, macros }, host.types);
      // A roll (the dice's, a reset) is one undo step of its own: never merged with the one before, however soon after.
      if (edit) host.store.change(f, null);
      else host.store.perform(f);
      if (ms > 0) easing = { from: fromValues, to: toValues, macrosFrom, macrosTo, start: now, ms };
    };

    // A roll being worked out, over frames (the Flame's survey yields; engine/dice.ts): a new track's (the patch's Dice:
    // a performance) or the dice's (an edit).
    let rolling: Generator<void, Rolled> | null = null;
    let rollingAs: { edit: string | null; ms: number } = { edit: null, ms: BLEND_MS };
    const rollFor = (seed: number, roll: Roll, reach = 1) => rollOf(host.store.patch, host.types, seed, roll, reach);

    let passes = Math.min(ceilingOf(host.device().quality, devicePixels()).steps, pace?.passes ?? Infinity);
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
      const flameId = flat.modules.find((x) => x.type === 'flame')?.id;
      ease(flat, now);
      if (patch !== storeSeen || easing) {
        storeSeen = patch;
        const ofType = (type: string) => flat.modules.find((x) => x.type === type);
        const music = ofType('music');
        const kind = (type: string, knob: string) => host.types[type]?.knobs.find((k) => k.id === knob);
        lighting = Array.from({ length: BANDS }, (_, b) => asSet(flat, music, `hold.${b}`, host.types) ?? kind('music', 'hold')?.default ?? 0.15);
      }
      if (live.rebuilds !== rebuildsSeen) {
        rebuildsSeen = live.rebuilds;
        try {
          renderer.setPatch(patch, host.pictureTypes);
          if (renderer.errors.length) console.warn(`Visuals: ${renderer.errors.join(' · ')}`);
          host.onErrors?.(renderer.errors);
        } catch (e) {
          console.warn('Visuals: the patch could not be drawn as a graph', e);
          host.onErrors?.(['The patch could not be drawn.']);
        }
      }

      const m = musicNow(now, lighting, flat.modules.some((x) => x.type === 'waves'));
      key = m.key;
      // The dice the owner tapped: a roll over its scope, checked against the track, as an edit.
      if (this.#rollWanted && !rolling) {
        const want = this.#rollWanted;
        this.#rollWanted = null;
        // Reset to seed: the track's own, unchecked (a new track's look is not checked either); the dice: a fresh one.
        const reset = want.seed === -1;
        const seed = reset ? seedOf(m.key ?? 'cratarr') : Math.floor(Math.random() * 0x100000000);
        rolling = rollFor(seed, want.scope);
        rollingAs = { edit: want.edit, ms: reset ? BLEND_MS : ANOTHER_MS };
        lookCame = reset ? 'track' : 'dice';
      }
      if (rolling) {
        const until = performance.now() + 4;
        let next = rolling.next();
        while (!next.done && performance.now() < until) next = rolling.next();
        if (next.done) {
          rolling = null;
          apply(next.value.values, next.value.cables, rollingAs.ms, rollingAs.edit, now, next.value.macros);
          nextBlend = rollingAs.ms;
        }
      }

      // The cover's colours, asked for as a track starts, where the Flame's Colours want them (Cover).
      const playingNow = playing();
      if (wantsCover && playingNow && m.key && cover?.key !== m.key) {
        const want = m.key;
        cover = { key: want, colours: null };
        coverPalette(coverUrl(playingNow)).then(
          (p) => {
            if (cover?.key === want) cover = { key: want, colours: p?.colours ?? null };
          },
          () => {},
        );
      }
      // The cover as a picture, where the Music's reaches the Screen or is observed.
      if (renderer.wantsCover && playingNow && m.key && coverPicture?.key !== m.key) {
        const want = m.key;
        coverPicture = { key: want, image: null };
        coverImage(coverUrl(playingNow)).then(
          (image) => {
            if (coverPicture?.key === want) coverPicture = { key: want, image };
          },
          () => {},
        );
      }
      const glide = nextBlend;
      nextBlend = null;

      // The engine over the patch, its sidechains keyed by the meters; the Flame's parts its points.
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
      const stepped = stepEngine(engine, flat, host.types, m, read, dt, {
        now,
        fluidFull: FLUID_FULL,
        // The device's reduced motion (Settings, or the system's), read each frame as the chrome's fades read it.
        still: reducedMotion(),
        meters: meterValues,
        aspect: renderer ? renderer.width / Math.max(1, renderer.height) : undefined,
        fluidFlow: renderer?.fluidFlow(),
        view: { cover: { key: m.key ?? null, colours: cover?.key === m.key ? cover.colours : null }, glide },
        ceiling: device.brightness,
      });
      const fv = flameId ? stepped.flames[flameId] : undefined;
      wantsCover = fv?.mode === 'cover';
      // The sidechains act only through their cables (toV2's holds, in sight); what they hold back is read off the
      // Duckers' outputs for the page's mark, nothing else.
      const sidechains = leversOf(stepped.outputs);
      // The layers past the flame and the picture's own loop: the compositor's, from the patch (setPatch), each
      // module's knobs as the engine moves them; where a roll is easing in, where each knob is going.
      const inputs: PictureInputs = {
        knobs: (id) => stepped.knobs[id],
        target: (id, knob) => easing?.to[id]?.[knob] ?? stepped.knobs[id]?.[knob] ?? flat.modules.find((x) => x.id === id)?.knobs[knob] ?? 0,
        frame: { dt, time: now / 1000, beat: m.beat, signals: read, waveform: m.wave ?? null },
        params: (id) => stepped.params[id],
        fields: (id) => stepped.fields[id],
        // A Grid's points, or the Flame's parts where they are on the screen (in the Grid's frame: centred, the short side
        // -1..1), for Streamlines' seeds: the engine's.
        points: (id) => stepped.points[id],
        cover: () => (coverPicture?.key === m.key ? coverPicture.image : null),
      };
      // A snapshot's thumbnail (picture()), from the frame just drawn.
      const pictureTaken = () => {
        const done = this.#pictureWanted;
        this.#pictureWanted = null;
        if (done) done(pictureOf(canvas));
      };
      // A patch with no Cratarr device (a blank slate): no flame, whatever else reaches the Screen drawn from black
      // (the renderer blanks it when nothing does).
      if (!stepped.device) {
        const [w, h] = sized();
        renderer.frame(passes, null, w, h, inputs);
        pictureTaken();
        return;
      }
      const stages = stepped.device.stages;

      // A new track's look, as the patch's Dice roll it: worked out over the next frames. Each one after the first is an
      // undo step of its own (the owner: the track may change what is on, Undo brings back the look before it).
      for (const r of stepped.dice) {
        if (rolling) break;
        rolling = rollFor(r.seed, r.scope, r.reach);
        // A Dice with a Glide: its roll morphs in over that long, a performance (no undo step), from where
        // the knobs are now, mid-morph or not, so one morph flows into the next.
        if (r.glide > 0) {
          rollingAs = { edit: null, ms: r.glide * 1000 };
          continue;
        }
        // The first look shows at once; each next one blends in over a new track's time.
        rollingAs = { edit: looked ? 'track' : null, ms: looked ? BLEND_MS : 0 };
        looked = true;
        lookCame = 'track';
      }
      // Each step whose Next a trigger fired (0057 §7e: a Gate's, in sight), on to the step the engine gives it: into
      // the store as a performance. The rack as it is: an effect is a module, not a step, so nothing here swaps one.
      const moved: Randomized = {};
      for (const r of stepped.nexts) (moved[r.module] ??= {})[r.knob] = r.to;
      if (Object.keys(moved).length) {
        apply(moved, {}, 0, null, now);
        nextBlend = ANOTHER_MS;
        lookCame = 'next';
      }

      // The Flame as its node made it this frame: its look blended, framed as far as its Fit.
      if (!fv) return;
      renderer.setFlame(fv.camera);

      const [w, h, resized, top] = sized();


      renderer.frame(
        passes,
        stages,
        w,
        h,
        inputs,
      );
      renderer.measure(w, h, now / 1000);
      const stats = frameStats(stepped, { patch: flat, t: now / 1000, look: lookCame, diveTurn: renderer.diveTurnRate })!;
      lookCame = null;
      renderer.meterAdd(stats);
      if (now - meteredAt >= 250) {
        meterValues = renderer.meterValues;
        meteredAt = now;
        const held = { flash: holding(sidechains, 'swing'), speed: holding(sidechains, 'pace'), wild: (['swing', 'pace', 'evolution', 'hue', 'size', 'gap'] as const).some((kk) => holding(sidechains, kk)) };
        if (held.flash !== heldBefore.flash || held.speed !== heldBefore.speed || held.wild !== heldBefore.wild) {
          heldBefore = held;
          host.onHeld?.(held);
        }
      }
      if (now - reported >= 250) {
        reported = now;
        host.onFrame?.(stepped, sidechains);
      }
      pictureTaken();

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
