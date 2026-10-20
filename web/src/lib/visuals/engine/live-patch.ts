// The store's patch as the engine runs it (decision 0057, the switch): flattened, and the same object for as long as its
// structure is the same. The store makes a new patch on every change, a knob's drag included; the engine works its
// structure out once a patch object (graph.ts's plan). So a change that only moves values (knobs, cables' amounts, macros'
// values and ranges) is copied into the patch the engine already runs, in place; one that changes what is wired (a module
// or a cable added, taken away or moved) gives a new one.

import type { PatchV2 } from '../ports';
import { flatten } from '../v2';

/** What makes a patch's structure: its modules' ids and types, its cables' ids and ends and depths, its macros' ids and
 *  targets, its pins. */
function structureOf(p: PatchV2): string {
  return JSON.stringify([
    // A Music signal module's reading and hold are its row's (music-signals.ts: read as the hub's, once a structure).
    p.modules.map((m) => (m.type.startsWith('music-') ? [m.id, m.type, m.knobs] : [m.id, m.type])),
    p.cables.map((c) => [c.id, c.from, c.to, c.depth ?? null]),
    p.macros.map((x) => [x.id, x.targets.map((t) => [t.module, t.knob])]),
    p.pins,
  ]);
}

export class LivePatch {
  #from: PatchV2 | null = null;
  #flat: PatchV2 | null = null;
  #structure = '';
  /** How many times the structure changed (a new object): for a test, and a debugger. */
  rebuilds = 0;

  /** The engine's patch for the store's `p`: the same object while only values change. */
  update(p: PatchV2): PatchV2 {
    if (p === this.#from && this.#flat) return this.#flat;
    this.#from = p;
    const flat = flatten(p);
    const structure = structureOf(flat);
    if (!this.#flat || structure !== this.#structure) {
      this.#flat = flat;
      this.#structure = structure;
      this.rebuilds++;
      return flat;
    }
    // The same wiring: the values into the patch the engine runs, in place.
    const live = this.#flat;
    flat.modules.forEach((m, i) => {
      const knobs = live.modules[i].knobs;
      for (const k of Object.keys(knobs)) if (!(k in m.knobs)) delete knobs[k];
      Object.assign(knobs, m.knobs);
    });
    flat.cables.forEach((c, i) => (live.cables[i].value = c.value));
    flat.macros.forEach((x, i) => {
      live.macros[i].value = x.value;
      x.targets.forEach((t, j) => Object.assign(live.macros[i].targets[j], { from: t.from, to: t.to }));
    });
    return live;
  }
}
