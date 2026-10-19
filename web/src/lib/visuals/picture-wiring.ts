// The picture side of a converted patch (decision 0057, step 3; v2.ts's hook): today's chain as picture modules and
// cables, in the order today's renderer draws it. The flame's own loop on its light (Feedback → Trails → Fluid →
// Tunnel → the Loop effect → Dive → the points), Show's device, the layers mixed over the flame, the picture's own
// loop (a Feedback → its modules → a Mix keeping the brighter, with the new picture), the effects, the Screen.
import type { Layer, Module, Patch } from './patch';
import { MIX_MODES, type CableV2 } from './ports';
import { setPictureWiring } from './v2';

type End = [module: string, port: string];
const cable = (from: End, to: End): CableV2 => ({
  id: `picture:${from[0]}.${from[1]}>${to[0]}.${to[1]}`,
  from: { module: from[0], port: from[1] },
  to: { module: to[0], port: to[1] },
  value: 1,
  mode: 'level',
});

/** The flame's own loop, in the order its carry pass applies them. */
const CARRY_ORDER = ['trails', 'fluid', 'tunnel', 'loop-effect', 'dive'];

/** A layer's blend as a Mix input's mode (Mix's list holds the layers' blends and Lighten). */
export const mixMode = (b: Layer['blend']['mode']) => MIX_MODES.indexOf(b as (typeof MIX_MODES)[number]);

/** The modules and cables `p`'s pictures need beside its own modules. */
export function pictureWiring(p: Patch): { modules: Module[]; cables: CableV2[] } {
  const modules: Module[] = [];
  const cables: CableV2[] = [];
  const mixKnobs: Record<string, number> = {};
  const into: End[] = [];
  for (const layer of p.layers) {
    let at: End;
    if (layer.source.type === 'flame') {
      modules.push({ id: 'light-feedback', type: 'feedback', knobs: {} });
      let light: End = ['light-feedback', 'picture'];
      for (const id of CARRY_ORDER)
        if (layer.loop.some((m) => m.id === id)) {
          cables.push(cable(light, [id, 'light']));
          light = [id, 'light'];
        }
      cables.push(cable(light, [layer.source.id, 'light']), cable([layer.source.id, 'light'], ['light-feedback', 'picture']));
      at = [layer.source.id, 'light'];
    } else at = [layer.source.id, 'picture'];
    for (const m of layer.modules) {
      // Show's device takes the light and gives a picture; any other module a picture.
      cables.push(cable(at, [m.id, m.id === 'show' ? 'light' : 'picture']));
      at = [m.id, 'picture'];
    }
    const i = into.length + 1;
    mixKnobs[`mode${i}`] = mixMode(layer.blend.mode);
    mixKnobs[`amount${i}`] = layer.blend.amount;
    into.push(at);
  }
  let at: End | null = null;
  if (into.length) {
    modules.push({ id: 'layers', type: 'mix', knobs: mixKnobs });
    into.forEach((e, i) => cables.push(cable(e, ['layers', `in${i + 1}`])));
    at = ['layers', 'picture'];
  }
  // The picture's own loop: the last picture moved and kept, and the new one over it, the brighter of the two.
  if (at && p.loop.length) {
    modules.push({ id: 'loop-feedback', type: 'feedback', knobs: {} }, { id: 'loop-mix', type: 'mix', knobs: { mode1: MIX_MODES.indexOf('lighten'), amount1: 1, mode2: MIX_MODES.indexOf('lighten'), amount2: 1 } });
    let kept: End = ['loop-feedback', 'picture'];
    for (const m of p.loop) {
      cables.push(cable(kept, [m.id, 'picture']));
      kept = [m.id, 'picture'];
    }
    cables.push(cable(kept, ['loop-mix', 'in1']), cable(at, ['loop-mix', 'in2']), cable(['loop-mix', 'picture'], ['loop-feedback', 'picture']));
    at = ['loop-mix', 'picture'];
  }
  for (const m of p.finish) {
    if (at) cables.push(cable(at, [m.id, 'picture']));
    at = [m.id, 'picture'];
  }
  modules.push({ id: 'screen', type: 'screen', knobs: {} });
  if (at) cables.push(cable(at, ['screen', 'picture']));
  return { modules, cables };
}

setPictureWiring(pictureWiring);
