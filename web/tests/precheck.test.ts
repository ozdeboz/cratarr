// The dice's check (precheck.ts) on the one engine (decision 0057): over the recorded tracks, random recipes get the
// verdict, the worst and the mean they got from today's engine (the patch without its sidechains, as the check ran
// without the governor).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/aspects-record.ts';

const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { precheck, measure, diveTurnPerRate, DEFAULT_LIMITS } = await import('../src/lib/visuals/precheck.ts');
const { recipeFor, settingsOf } = await import('../src/lib/visuals/recipe.ts');
const { newAspects, stepAspects } = await import('../src/lib/visuals/aspects.ts');
const { Meters } = await import('../src/lib/visuals/meters.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { BUDGETED, budgetOf } = await import('../src/lib/visuals/budget.ts');
const { ASPECTS } = await import('../src/lib/visuals/routes.ts');

/** Today's check, as it was before the one engine (stepAspects), for the comparison. */
function before(recipe: ReturnType<typeof recipeFor>, signals: Parameters<typeof musicOf>[0], beats: Parameters<typeof musicOf>[1], from: number, seconds: number) {
  const settings = settingsOf(recipe, DEFAULTS, 'all');
  const divePerRate = settings.dive > 0 ? diveTurnPerRate(settings) : 0;
  const aspects = newAspects();
  const state = newMusicState();
  const meters = new Meters();
  const worst: Record<string, number> = {};
  const dt = 1 / 30;
  for (let k = 0; k < Math.round(seconds * 30); k++) {
    const t = from + k * dt;
    const m = musicOf(signals, beats, t, true, { state, at: t * 1000, lighting: settings.lighting });
    const a = stepAspects(aspects, m, settings, dt, { still: false, now: t * 1000, fluidFull: FLUID_FULL });
    measure(meters, a, k * dt, settings, divePerRate);
    if (k * dt < 3) continue;
    const budget = budgetOf(1, 0.5, a.lit);
    for (const [key, v] of Object.entries(meters.now())) if (typeof v === 'number') worst[key] = Math.max(worst[key] ?? 0, (BUDGETED as readonly string[]).includes(key) ? v / budget : v);
  }
  return worst;
}

test('random recipes on the recorded tracks: the verdict and the worst as today’s engine gave them', () => {
  let over = 0;
  for (const id of TRACKS) {
    const t = track(id);
    for (const seed of [11, 222, 3333]) {
      const recipe = recipeFor(seed * 7919 + id.length, ASPECTS);
      const now = precheck(recipe, DEFAULTS, t.signals, t.beats, 20, DEFAULT_LIMITS, 12);
      const was = before(recipe, t.signals, t.beats, 20, 12);
      for (const [key, v] of Object.entries(was)) assert.ok(Math.abs((now.worst as Record<string, number>)[key] - v) <= 1e-6 * Math.max(1, Math.abs(v)), `${id} ${seed} ${key}: ${(now.worst as Record<string, number>)[key]} against ${v}`);
      const wasOver = Object.entries(DEFAULT_LIMITS).filter(([key, limit]) => limit !== undefined && (was[key] ?? 0) > limit).map(([key]) => key);
      assert.deepEqual(now.over, wasOver, `${id} ${seed}`);
      over += now.over.length;
    }
  }
  assert.ok(over >= 0);
});
