// Where the one engine's time goes (decision 0057, the switch): the Cratarr chain's patch timed whole, then without
// each group of what moved into it from outside today's stepAspects (the sidechains, the pause's dimming, the hits'
// ADSRs), and as bare as the chain gets.
//   node --experimental-strip-types --import ./tests/ts-resolve.mjs scripts/engine-bench-parts.ts
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newAspects, stepAspects } = await import('../src/lib/visuals/aspects.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { TRACKS, track } = await import('../tests/harness/aspects-record.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

const s = structuredClone(DEFAULTS);
const FRAMES = 30 * 60;
const dt = 1 / 30;
const t = track(TRACKS[0]);

function bench(step: () => (m: ReturnType<typeof musicOf>, at: number) => void): number {
  const means: number[] = [];
  for (let round = 0; round < 9; round++) {
    const music = newMusicState();
    const frame = step();
    let total = 0;
    for (let k = 0; k < FRAMES; k++) {
      const at = 20 + k * dt;
      const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: s.lighting });
      const t0 = performance.now();
      frame(m, at);
      total += performance.now() - t0;
    }
    if (round > 1) means.push((total / FRAMES) * 1000);
  }
  return Math.min(...means);
}
const engine = (p: PatchV2) => bench(() => {
  const st = newEngine();
  return (m, at) => void stepEngine(st, p, {}, m, () => 0, s, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
});
/** `p` without the modules whose ids match, and their cables. */
const without = (p: PatchV2, gone: RegExp): PatchV2 => {
  const ids = new Set(p.modules.filter((m) => gone.test(m.id)).map((m) => m.id));
  return { ...p, modules: p.modules.filter((m) => !ids.has(m.id)), cables: p.cables.filter((c) => !ids.has(c.from.module) && !ids.has(c.to.module)), macros: p.macros.filter((x) => !x.targets.some((tg) => ids.has(tg.module))) };
};

const today = bench(() => {
  const st = newAspects();
  return (m, at) => void stepAspects(st, m, s, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
});
const full = flatten(toV2(s));
const SIDECHAIN = /^(meter|screen|duck-.*)$/;
const PAUSE = /^pause-/;
/** The hits straight from Music (read as the routes read them) instead of through the ADSRs: the same picture. */
const noAdsr = (p: PatchV2): PatchV2 => {
  const adsrs = new Map(p.modules.filter((m) => m.type === 'adsr').map((m) => [m.id, m]));
  const gate = new Map(p.cables.filter((c) => adsrs.has(c.to.module) && c.to.port === 'gate').map((c) => [c.to.module, c.from.port]));
  const music = p.modules.find((m) => m.type === 'music')!;
  return {
    ...p,
    modules: p.modules.filter((m) => !adsrs.has(m.id)).map((m) => (m === music ? { ...m, knobs: { ...m.knobs, 'reference.hits': 2, 'reference.percussiveHits': 2 } } : m)),
    cables: p.cables.filter((c) => !adsrs.has(c.to.module)).map((c) => (adsrs.has(c.from.module) ? { ...c, from: { ...c.from, module: music.id, port: gate.get(c.from.module)! } } : c)),
  };
};
const rows: [string, PatchV2][] = [
  ['the whole patch', full],
  ['without the sidechains', without(full, SIDECHAIN)],
  ['without the pause dimming', without(full, PAUSE)],
  ['without both', without(full, new RegExp(`${SIDECHAIN.source}|${PAUSE.source}`))],
  ['without both, hits without ADSRs', noAdsr(without(full, new RegExp(`${SIDECHAIN.source}|${PAUSE.source}`)))],
];
console.log(`today's stepAspects`.padEnd(34), `${today.toFixed(1)} µs`);
for (const [name, p] of rows) console.log(name.padEnd(34), `${engine(p).toFixed(1)} µs  (${p.modules.length} modules, ${p.cables.length} cables)`);
