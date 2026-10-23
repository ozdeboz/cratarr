// Grid lab (untracked): each rack drawn for a while, three pictures of it 2.5 s apart. window.grids.result when done.
import { VisualsLoop } from './lib/visuals/visuals-loop';
import { v2TypeRecord } from './lib/visuals/v2-types';
import { v2Types } from './lib/visuals/picture-graph';
import { cratarrTypes } from './lib/visuals/patch';
import { DEFAULTS, RANGES, visualSettings } from './lib/visuals/settings.svelte';
import type { PatchV2 } from './lib/visuals/ports';
const TYPES = v2TypeRecord();
const PICTURE_TYPES = v2Types(cratarrTypes(RANGES, DEFAULTS));
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const canvas = document.getElementById('c') as HTMLCanvasElement;
let patch = { v: 2, modules: [], cables: [], macros: [], pins: [] } as unknown as PatchV2;
const errors: string[] = [];
const store = { get patch() { return patch; }, perform(f: (p: PatchV2) => PatchV2 | void) { const n = clone(patch); patch = (f(n) as PatchV2) ?? n; }, change(f: (p: PatchV2) => PatchV2 | void) { this.perform(f); } };
const loop = new VisualsLoop({ canvas, store, device: () => visualSettings as any, types: TYPES, pictureTypes: PICTURE_TYPES, onFrame: () => {}, onErrors: (e: string[]) => errors.push(...e) } as any);
loop.start();
async function run() {
  const racks: Record<string, PatchV2> = await (await fetch(new URLSearchParams(location.search).get('racks') ?? '/grid-racks.json')).json();
  const result: any[] = [];
  for (const [name, rack] of Object.entries(racks)) {
    patch = clone(rack);
    errors.length = 0;
    await wait(3000);
    const shots: (string | null)[] = [];
    for (let i = 0; i < 3; i++) { shots.push(await loop.picture()); await wait(2500); }
    result.push({ name, shots, errors: [...new Set(errors)] });
    (window as any).grids.progress = result.length;
  }
  (window as any).grids.result = result;
}
(window as any).grids = { progress: 0 };
run().catch((e) => ((window as any).grids.error = String(e)));
