// Dice lab (untracked): each rack rolled N times from where it is, the screen's brightness after each roll, and every
// knob the roll moved. window.dice.result when done.
import { VisualsLoop } from './lib/visuals/visuals-loop';
import { v2TypeRecord } from './lib/visuals/v2-types';
import { v2Types } from './lib/visuals/picture-graph';
import { cratarrTypes } from './lib/visuals/patch';
import { DEFAULTS, RANGES, visualSettings } from './lib/visuals/settings.svelte';
import { flatten } from './lib/visuals/v2';
import type { PatchV2 } from './lib/visuals/ports';

const TYPES = v2TypeRecord();
const PICTURE_TYPES = v2Types(cratarrTypes(RANGES, DEFAULTS));
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const canvas = document.getElementById('c') as HTMLCanvasElement;
let patch: PatchV2 = { v: 2, modules: [], cables: [], macros: [], pins: [] } as unknown as PatchV2;
let changed = 0;
const store = {
  get patch() { return patch; },
  perform(f: (p: PatchV2) => PatchV2 | void) { const n = clone(patch); patch = (f(n) as PatchV2) ?? n; changed++; },
  change(f: (p: PatchV2) => PatchV2 | void) { this.perform(f); },
};
const loop = new VisualsLoop({ canvas, store, device: () => visualSettings as any, types: TYPES, pictureTypes: PICTURE_TYPES, onFrame: () => {} });
loop.start();

async function brightness(): Promise<number> {
  const url = await loop.picture();
  if (!url) return -1;
  const img = new Image();
  img.src = url;
  await img.decode();
  const c = document.createElement('canvas');
  [c.width, c.height] = [img.width, img.height];
  const g = c.getContext('2d')!;
  g.drawImage(img, 0, 0);
  const d = g.getImageData(0, 0, c.width, c.height).data;
  let sum = 0;
  for (let i = 0; i < d.length; i += 4) sum += 0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2];
  return sum / (d.length / 4);
}
async function settled(): Promise<number> {
  const xs: number[] = [];
  for (let i = 0; i < 4; i++) { xs.push(await brightness()); await wait(400); }
  return xs.reduce((a, b) => a + b, 0) / xs.length;
}
const knobsOf = (p: PatchV2) => Object.fromEntries(flatten(p).modules.flatMap((m) => Object.entries(m.knobs).map(([k, v]) => [`${m.type}:${m.id}.${k}`, v])));

async function run() {
  const racks: Record<string, PatchV2> = await (await fetch('/dice-racks.json')).json();
  const n = Number(new URLSearchParams(location.search).get('n') ?? 25);
  const result: any[] = [];
  let done = 0;
  for (const [name, rack] of Object.entries(racks)) {
    patch = clone(rack);
    await wait(5000);
    const base = await settled();
    const rolls: any[] = [];
    for (let i = 0; i < n; i++) {
      patch = clone(rack);
      await wait(2500);
      const before = knobsOf(patch);
      const was = changed;
      loop.roll('everything');
      const until = performance.now() + 8000;
      while (changed === was && performance.now() < until) await wait(100);
      await wait(1500 + 2500);
      const after = knobsOf(patch);
      const moved: Record<string, [number, number]> = {};
      for (const k of Object.keys(after)) if (before[k] !== after[k]) moved[k] = [before[k], after[k]];
      rolls.push({ applied: changed !== was, brightness: await settled(), moved });
      (window as any).dice.progress = ++done;
    }
    result.push({ name, base, rolls });
  }
  (window as any).dice.result = result;
}
(window as any).dice = { progress: 0 };
run().catch((e) => ((window as any).dice.error = String(e)));
