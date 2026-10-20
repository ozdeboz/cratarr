// The one engine's cost per frame against today's, in a browser (decision 0057, the switch: the phone's and the
// tablet's cost): scripts/engine-bench.ts's run over one recorded track, its files fetched beside the page; the
// results shown, and posted to `result` for the machine that served it.
//   npx esbuild scripts/engine-bench-browser.ts --bundle --format=esm --outfile=<dir>/bench.js
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newAspects, stepAspects } = await import('../src/lib/visuals/aspects.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
const { decode: decodeSignals } = await import('../src/lib/visuals/signals.ts');
const { decode: decodeBeats } = await import('../src/lib/visuals/beats.ts');

const out = document.getElementById('out')!;
const say = (line: string) => {
  out.textContent += `${line}\n`;
};
const fetched = async (name: string) => (await fetch(name)).arrayBuffer();
const signals = decodeSignals(await fetched('tr-10329.bands'))!;
const beats = decodeBeats(await fetched('tr-10329.beats'));

const busy = structuredClone(DEFAULTS);
busy.routes = {
  ...busy.routes,
  ...Object.fromEntries([0, 1, 2, 3, 4, 5].flatMap((b) => [[routeKey('level', b, 'rotation'), 1], [routeKey('flux', b, 'hue'), 1], [routeKey('level', b, 'tunnel'), 0.5]])),
};
const FRAMES = 30 * 60;
const dt = 1 / 30;
const results: Record<string, { mean: number; p95: number; max: number }> = {};

/** The best of seven rounds' mean (the first a warm-up), its 95th percentile and its slowest frame, in µs. */
async function bench(name: string, s: typeof DEFAULTS, step: () => (m: ReturnType<typeof musicOf>, at: number) => void) {
  const means: number[] = [];
  let p95 = Infinity;
  let max = Infinity;
  for (let round = 0; round < 7; round++) {
    const music = newMusicState();
    const frame = step();
    const times: number[] = [];
    for (let k = 0; k < FRAMES; k++) {
      const at = 20 + k * dt;
      const m = musicOf(signals, beats, at, true, { state: music, at: at * 1000, lighting: s.lighting });
      const t0 = performance.now();
      frame(m, at);
      times.push(performance.now() - t0);
    }
    // Let the page breathe between rounds.
    await new Promise((r) => setTimeout(r, 50));
    if (round === 0) continue;
    times.sort((a, b) => a - b);
    means.push(times.reduce((a, b) => a + b, 0) / times.length);
    p95 = Math.min(p95, times[Math.floor(times.length * 0.95)]);
    max = Math.min(max, times[times.length - 1]);
  }
  const best = Math.min(...means);
  results[name] = { mean: best * 1000, p95: p95 * 1000, max: max * 1000 };
  say(`${name.padEnd(30)} mean ${(best * 1000).toFixed(1)} µs  p95 ${(p95 * 1000).toFixed(1)} µs  slowest ${(max * 1000).toFixed(0)} µs`);
  return best;
}

say(navigator.userAgent);
for (const [label, s] of [['the Cratarr chain', structuredClone(DEFAULTS)], ['busy', busy]] as const) {
  const old = await bench(`today's (${label})`, s, () => {
    const st = newAspects();
    return (m, at) => void stepAspects(st, m, s, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
  });
  const p = flatten(toV2(s));
  const now = await bench(`one engine (${label})`, s, () => {
    const st = newEngine();
    return (m, at) => void stepEngine(st, p, {}, m, () => 0, s, dt, { still: false, now: at * 1000, fluidFull: FLUID_FULL });
  });
  say(`  ${(now / old).toFixed(1)}× today's`);
}
say('done');
await fetch('result', { method: 'POST', body: JSON.stringify({ agent: navigator.userAgent, results }) });
