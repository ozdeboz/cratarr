// The picture side of a converted patch (decision 0057, step 3; v2.ts's hook): today's chain as picture modules and
// cables, in the order today's renderer draws it. The flame's own loop on its light (Feedback → Trails → Fluid →
// Tunnel → the Loop effect → Dive → the points), Show's device, the layers mixed over the flame, the picture's own
// loop (a Feedback → its modules → a Mix keeping the brighter, with the new picture), the effects, the Screen.
import type { Layer, Module, Patch } from './patch';
import { effectOf, engineEffect } from './effects';
import { effectModuleOf, effectOfType, SLOTS } from './effect-modules';
import { engineOf } from './engine-module';
import { MIX_MODES, type CableV2 } from './ports';
import { setPictureWiring } from './v2';

type End = [module: string, port: string];
const cable = (from: End, to: End): CableV2 => ({
  id: `picture:${from[0]}.${from[1]}>${to[0]}.${to[1]}`,
  from: { module: from[0], port: from[1] },
  to: { module: to[0], port: to[1] },
  value: 1,
});

/** The flame's own loop, in the order its carry pass applies them. */
const CARRY_ORDER = ['trails', 'fluid', 'tunnel', 'loop-effect', 'dive'];

/** A layer's blend as a Mix input's mode (Mix's list holds the layers' blends and Lighten). */
export const mixMode = (b: Layer['blend']['mode']) => MIX_MODES.indexOf(b as (typeof MIX_MODES)[number]);

/** What an effect module draws: a shape, a colour, an echo (none for an engine on no effect). */
const kindOf = (m: Module) => {
  const engine = engineOf(m.type);
  return (effectOfType(m.type) ?? effectOf(engine ? engineEffect(engine, m.knobs.effect ?? 1) : 0))?.kind;
};

/** The effects as modules in a line drawing what today's one pass draws (effects.ts pictureShader: every shape in slot
 *  order, then the colours and echoes at the coordinates they give): the colours and echoes first in slot order, then
 *  the shapes in reverse (a shape reads its input where it moves, so the last one moves first). */
export function effectsInOrder(finish: Module[]): Module[] {
  const shapes = finish.filter((m) => kindOf(m) === 'shape');
  return [...finish.filter((m) => kindOf(m) !== 'shape'), ...shapes.reverse()];
}

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
      for (const id of CARRY_ORDER) {
        const m = layer.loop.find((x) => x.id === id);
        // The loop's slot as its effect's module, on the light (none on no effect).
        const effect = m && SLOTS.has(m.id) ? effectModuleOf(m) : null;
        if (!m || (SLOTS.has(m.id) && !effect)) continue;
        const to: End = effect ? [effect.id, 'picture'] : [id, 'light'];
        cables.push(cable(light, to));
        light = to;
      }
      cables.push(cable(light, [layer.source.id, 'light']), cable([layer.source.id, 'light'], ['light-feedback', 'picture']));
      at = [layer.source.id, 'light'];
    } else at = [layer.source.id, 'picture'];
    for (const m of layer.modules) {
      // Show's device takes the light and gives a picture; any other module a picture.
      cables.push(cable(at, [m.id, m.id === 'show' ? 'light' : 'picture']));
      // A Tint on a layer after the first takes its colours from the first (the flame's), as 70.1's tint did.
      if (m.type === 'tint' && into.length) cables.push(cable(into[0], [m.id, 'by']));
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
  // An Echo (the owner): a visible loop, the line's finished picture fed back, moved and dimmed by the Echo, kept where
  // it is the brighter (a Mix's Lighten) with the picture before it, then any shapes turn the two together (the trails
  // spiral as they always did). No Echo keeps a hidden trail of its own.
  const line = effectsInOrder(p.finish.flatMap((m) => (SLOTS.has(m.id) ? (effectModuleOf(m) ?? []) : [m])));
  const looped = line.filter((m) => kindOf(m) === 'echo');
  for (const m of line) {
    if (looped.includes(m)) {
      const mix = `${m.id}-mix`;
      const fb = `${m.id}-feedback`;
      modules.push({ id: fb, type: 'feedback', knobs: {} }, { id: mix, type: 'mix', knobs: { mode1: MIX_MODES.indexOf('over'), amount1: 1, mode2: MIX_MODES.indexOf('lighten'), amount2: 1 } });
      if (at) cables.push(cable(at, [mix, 'in1']));
      cables.push(cable([fb, 'picture'], [m.id, 'picture']), cable([m.id, 'picture'], [mix, 'in2']));
      at = [mix, 'picture'];
      continue;
    }
    if (at) cables.push(cable(at, [m.id, 'picture']));
    at = [m.id, 'picture'];
  }
  if (at) for (const m of looped) cables.push(cable(at, [`${m.id}-feedback`, 'picture']));
  modules.push({ id: 'screen', type: 'screen', knobs: {} });
  if (at) cables.push(cable(at, ['screen', 'picture']));
  return { modules, cables };
}

setPictureWiring(pictureWiring);
