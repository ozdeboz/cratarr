// Each band's Envelope (envelope.ts, decision 0050): a struck drum flares and is gone, a pad glows, a chord flares then
// holds while it rings.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { followEnvelopes, SUSTAIN } from '../src/lib/visuals/envelope.ts';
import { B, BAND_SIGNALS } from '../src/lib/visuals/routes.ts';

const LINGER = 0.15;
const one = () => ({ bands: [new Float32Array(BAND_SIGNALS.length)], held: new Float32Array(1), heldAt: -1 });
/** Steps a band 60 frames a second for `secs`, its level and hits as `at(t)` gives them; the envelope at each frame. */
function run(at: (t: number) => { level: number; hits: number }, secs: number) {
  const st = one();
  const out: number[] = [];
  for (let i = 0; i <= secs * 60; i++) {
    const t = i / 60;
    const { level, hits } = at(t);
    st.bands[0][B.level] = level;
    st.bands[0][B.hits] = hits;
    followEnvelopes(st, t * 1000, [LINGER]);
    out.push(st.bands[0][B.envelope]);
  }
  return out;
}
const ring = (t: number, from: number) => (t < from ? 0 : Math.exp(-(t - from) / LINGER));

test('a struck drum: flares to the hit, gone soon after its level drops', () => {
  const e = run((t) => ({ level: t >= 0.5 && t < 0.6 ? 1 : 0, hits: ring(t, 0.5) }), 2);
  assert.ok(Math.abs(e[30] - 1) < 1e-6, `flare ${e[30]}`);
  assert.ok(e[30 + 60] < 0.02, `gone a second later: ${e[90]}`);
});

test('a pad, never struck: rises to the sustain share of its level and holds there', () => {
  const e = run((t) => ({ level: t >= 0.2 ? 0.8 : 0, hits: 0 }), 2);
  assert.ok(Math.abs(e[60] - SUSTAIN * 0.8) < 1e-3, `held ${e[60]}`);
  assert.ok(Math.abs(e[119] - SUSTAIN * 0.8) < 1e-3);
  assert.ok(e[12 + 3] < SUSTAIN * 0.8, 'it rises, not jumps');
});

test('a struck chord: flares, then holds while it rings, then lets go over the Linger', () => {
  const e = run((t) => ({ level: t >= 0.5 && t < 1.5 ? 0.9 : 0, hits: ring(t, 0.5) }), 3);
  assert.ok(Math.abs(e[30] - 1) < 1e-6);
  assert.ok(Math.abs(e[60] - SUSTAIN * 0.9) < 1e-2, `holds ${e[60]}`);
  const after = e[90 + Math.round(LINGER * 60)];
  assert.ok(after < SUSTAIN * 0.9 * 0.45 && after > 0.05, `lets go over its Linger: ${after}`);
});
