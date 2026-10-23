// Show lab (untracked): window.show(name) puts that rack on; the driver takes the screenshots.
import { VisualsLoop } from './lib/visuals/visuals-loop';
import { v2TypeRecord } from './lib/visuals/v2-types';
import { v2Types } from './lib/visuals/picture-graph';
import { cratarrTypes } from './lib/visuals/patch';
import { DEFAULTS, RANGES, visualSettings } from './lib/visuals/settings.svelte';
import type { PatchV2 } from './lib/visuals/ports';
const TYPES = v2TypeRecord();
const PICTURE_TYPES = v2Types(cratarrTypes(RANGES, DEFAULTS));
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const canvas = document.getElementById('c') as HTMLCanvasElement;
let patch = { v: 2, modules: [], cables: [], macros: [], pins: [] } as unknown as PatchV2;
const errors: string[] = [];
const store = { get patch() { return patch; }, perform(f: (p: PatchV2) => PatchV2 | void) { const n = clone(patch); patch = (f(n) as PatchV2) ?? n; }, change(f: (p: PatchV2) => PatchV2 | void) { this.perform(f); } };
const loop = new VisualsLoop({ canvas, store, device: () => visualSettings as any, types: TYPES, pictureTypes: PICTURE_TYPES, onFrame: () => {}, onErrors: (e: string[]) => errors.push(...e) } as any);
loop.start();
let racks: Record<string, PatchV2> = {};
(window as any).show = async (file: string, name: string) => {
  if (!racks[name]) racks = await (await fetch(file)).json();
  patch = clone(racks[name]);
  errors.length = 0;
  return Object.keys(racks);
};
(window as any).errors = () => [...new Set(errors)];
