// The one engine's cost per frame against today's (decision 0057, the switch): each over a recorded track at 30 frames a
// second, today's chain and a busy one; mean and 95th percentile per frame, in milliseconds.
//   node --experimental-strip-types --import ./tests/ts-resolve.mjs scripts/engine-bench.ts
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newAspects, stepAspects } = await import('../src/lib/visuals/aspects.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
const { TRACKS, track } = await import('../tests/harness/aspects-record.ts');

const busy = structuredClone(DEFAULTS);
busy.routes = {
  ...busy.routes,
  ...Object.fromEntries([0, 1, 2, 3, 4, 5].flatMap((b) => [[routeKey('level', b, 'rotation'), 1], [routeKey('flux', b, 'hue'), 1], [routeKey('level', b, 'tunnel'), 0.5]])),
};
const FRAMES = 30 * 60;
const dt = 1 / 30;

/** The best of seven rounds' mean (the first a warm-up): another process's load only ever makes a round slower. */
function bench(name: string, s: typeof DEFAULTS, step: () => (m: ReturnType<typeof musicOf>, at: number) => void) {
  const t = track(TRACKS[0]);
  const means: number[] = [];
  let p95 = Infinity;
  for (let round = 0; round < 7; round++) {
    const music = newMusicState();
    const frame = step();
    const times: number[] = [];
    for (let k = 0; k < FRAMES; k++) {
      const at = 20 + k * dt;
      const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: s.lighting });
      const t0 = performance.now();
      frame(m, at);
      times.push(performance.now() - t0);
    }
    if (round === 0) continue;
    times.sort((a, b) => a - b);
    means.push(times.reduce((a, b) => a + b, 0) / times.length);
    p95 = Math.min(p95, times[Math.floor(times.length * 0.95)]);
  }
  const best = Math.min(...means);
  console.log(`${name.padEnd(30)} best mean ${(best * 1000).toFixed(1)} µs  p95 ${(p95 * 1000).toFixed(1)} µs`);
  return best;
}

for (const [label, s] of [['the Cratarr chain', structuredClone(DEFAULTS)], ['busy', busy]] as const) {
  const old = bench(`today's (${label})`, s, () => {
    const st = newAspects();
    return (m, at) => void stepAspects(st, m, s, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
  });
  const p = flatten(toV2(s));
  const now = bench(`one engine (${label})`, s, () => {
    const st = newEngine();
    return (m, at) => void stepEngine(st, p, {}, m, () => 0, s, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
  });
  console.log(`  ${(now / old).toFixed(1)}× today's; ${p.modules.length} modules, ${p.cables.length} cables`);
}
