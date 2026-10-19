// The ADSR (engine/modules.ts, decision 0057) in place of the hidden Linger: over the recorded tracks, an ADSR per band
// at Linger's release, keyed by the band's strikes, gives what the Hits and Drum hits signals gave, at any Linger; and
// its own stages, as a synth's.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/aspects-record.ts';

const { adsrOf } = await import('../src/lib/visuals/engine/modules.ts');
const { musicOf, newMusicState, ATTACK } = await import('../src/lib/visuals/music-core.ts');
const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { B } = await import('../src/lib/visuals/routes.ts');

const LINGERS = [DEFAULTS.lighting, [0.02, 0.05, 0.3, 0.6, 1, 0.5], [1, 1, 1, 1, 1, 1]];

test('over the recorded tracks, an ADSR per band at Linger gives the Hits and Drum hits signals, at any Linger', () => {
  for (const id of TRACKS) {
    const t = track(id);
    for (const lighting of LINGERS) {
      const st = newMusicState();
      let lit = 0;
      for (let k = 0; k < 180 * 30; k++) {
        const pos = 10 + k / 30;
        const m = musicOf(t.signals, t.beats, pos, true, { state: st, at: pos * 1000, lighting });
        for (let b = 0; b < 6; b++) {
          const knobs = { attack: ATTACK, decay: 0, sustain: 0, release: lighting[b] };
          const hits = adsrOf(m.strikes![b].hits, knobs);
          const drums = adsrOf(m.strikes![b].drums, knobs);
          // The signals are stored as 32-bit floats.
          assert.ok(Math.abs(hits - m.bands![b][B.hits]) <= 1e-6, `${id} ${pos.toFixed(3)} s band ${b} hits: ${hits} against ${m.bands![b][B.hits]}`);
          assert.ok(Math.abs(drums - m.bands![b][B.percussiveHits]) <= 1e-6, `${id} ${pos.toFixed(3)} s band ${b} drums: ${drums} against ${m.bands![b][B.percussiveHits]}`);
          if (hits > 0.05) lit++;
        }
      }
      assert.ok(lit > 100, `${id}: the bands lit (${lit} frames)`);
    }
  }
});

const strike = (since: number, velocity = 1, next: { until: number; velocity: number } | null = null) => ({ since, velocity, next });
const near = (a: number, b: number) => Math.abs(a - b) < 1e-9;

test('the stages: attack ahead of a strike known ahead, at once heard live; decay to the sustain; the release rings and is gone after five', () => {
  const k = { attack: 0.04, decay: 0, sustain: 0, release: 0.2 };
  assert.ok(near(adsrOf(strike(Infinity, 0, { until: 0.04, velocity: 0.8 }), k), 0));
  assert.ok(near(adsrOf(strike(Infinity, 0, { until: 0.02, velocity: 0.8 }), k), 0.4));
  assert.ok(near(adsrOf(strike(0, 0.8), k), 0.8));
  assert.ok(near(adsrOf(strike(0.2, 0.8), k), 0.8 * Math.exp(-1)));
  assert.equal(adsrOf(strike(1.01, 0.8), k), 0);
  assert.equal(adsrOf(strike(Infinity), k), 0);
  const d = { attack: 0.04, decay: 0.1, sustain: 0.5, release: 0.2 };
  assert.ok(near(adsrOf(strike(0.05), d), 0.75));
  assert.ok(near(adsrOf(strike(0.1), d), 0.5));
  assert.ok(near(adsrOf(strike(0.3), d), 0.5 * Math.exp(-1)));
  // A retrigger: the strike to come rising under what still rings never dips it.
  assert.ok(near(adsrOf(strike(0.05, 1, { until: 0.03, velocity: 0.3 }), k), Math.exp(-0.25)));
  assert.ok(near(adsrOf(strike(0.05, 0.2, { until: 0, velocity: 0.9 }), k), 0.9));
});
