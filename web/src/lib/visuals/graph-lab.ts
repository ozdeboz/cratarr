// Development only (graph-lab.html, PatchGraphLab.svelte, and the graph's tests): the lab's module types and its
// sample v2 patches. Default: today's settings as a v2 patch (v2.ts toV2, its picture side wired as the app's). `wiresV2`: every wire type drawn, as cables and as
// normals: the flame's light through the Fluid and the Loop effect back into it by a Feedback, its points into the
// Fluid's pushes, the Fluid's flow into Bend (and the Loop effect's by its normal), Waves (its sound by its normal) bent
// into a Mix to the screen, values dashed, triggers dotted, a band's six at once "· 6", the Clock into a Sequencer by
// its normals; an LFO, a single ADSR and one on the six bands, a beat Gate on the bass, each with its face.

import type { ModuleType } from './patch';
import type { CableV2, PatchV2 } from './ports';
import { MODULATORS } from './modulators/modulators';
import { WAVES } from './sources/waves';
import { toV2 } from './v2';
// The picture side of toV2 (its layers, loops and finish as cables), as the app's store has it.
import './picture-wiring';
import { v2TypeRecord } from './v2-types';
import { DEFAULTS } from './settings.svelte';

/** Every type the lab draws, by its id: the app's (v2-types.ts), the modulators and Waves first. */
export function labTypes(): Record<string, ModuleType> {
  return { ...Object.fromEntries([...Object.values(MODULATORS), WAVES].map((t) => [t.type, t])), ...v2TypeRecord() };
}

/** Today's settings as a v2 patch, as the app's store makes it (its picture side wired: picture-wiring.ts). */
export function labV2(): PatchV2 {
  return toV2(DEFAULTS);
}

/** Every wire type, as cables and as normals. */
export function wiresV2(): PatchV2 {
  const m = (id: string, type: string, knobs: Record<string, number> = {}) => ({ id, type, knobs });
  let n = 0;
  const c = (from: string, to: string, value = 1, channel?: number): CableV2 => {
    const [fm, fp] = from.split('.');
    const [tm, tp] = to.split('.');
    return { id: `w${++n}`, from: { module: fm, port: fp, ...(channel === undefined ? {} : { channel }) }, to: { module: tm, port: tp }, value };
  };
  return {
    v: 2,
    modules: [
      m('music', 'music'),
      m('clock', 'clock'),
      m('lfo', 'lfo', { rate: 0.5, shape: 0.3 }),
      m('adsr', 'adsr', { attack: 0.06, decay: 0.25, sustain: 0.45, release: 0.35 }),
      m('bands', 'adsr', { attack: 0.02, decay: 0.2, sustain: 0.3, release: 0.3 }),
      m('seq', 'sequencer'),
      m('flame', 'flame'),
      m('fluid', 'fluid', { stir: 0.5, flow: 0.4 }),
      m('loop', 'loop-effect'),
      m('feedback', 'feedback'),
      m('waves', 'waves'),
      m('bend', 'bend'),
      m('mix', 'mix'),
      m('gate', 'gate'),
      m('grid', 'grid'),
      m('screen', 'screen'),
    ],
    cables: [
      c('flame.light', 'fluid.light'),
      c('flame.points', 'fluid.at'),
      c('fluid.light', 'loop.light'),
      c('loop.light', 'feedback.picture'),
      c('feedback.picture', 'flame.light'),
      c('fluid.field', 'bend.field'),
      c('waves.picture', 'bend.picture'),
      c('bend.picture', 'mix.in1'),
      c('mix.picture', 'screen.picture'),
      c('music.beat', 'adsr.gate'),
      c('music.hits', 'bands.gate'),
      c('lfo.out', 'waves.hue', 0.3),
      c('adsr.out', 'bend.waves', 0.4),
      c('bands.out', 'flame.lights', 0.5),
      c('music.level', 'waves.amplitude', 0.4, 1),
      c('seq.out', 'waves.radius', 0.25),
      c('music.level', 'gate.in', 1, 0),
      c('gate.out', 'mix.amount2', 0.5),
    ],
    macros: [],
    pins: [],
  };
}
