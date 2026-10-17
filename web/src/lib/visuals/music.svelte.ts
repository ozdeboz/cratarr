// What the visuals follow (decision 0046): the playing track's loudness where it is (its waveform, decision 0035), its
// tempo, and whether it plays. Nothing listens to the sound itself: in the app the music plays natively, out of the
// page's reach, and on another player it plays elsewhere. Read once per frame by the visuals' loop.
import { trackState } from '../api';
import { paused, playing, position } from '../remote.svelte';
import { loadWaveform, waveformOf } from '../waveforms.svelte';

export type Music = {
  /** The playing track (`source/id`), or null. */
  key: string | null;
  /** Its loudness here, 0..1 of its loudest moment (its waveform), or null where it has none. */
  energy: number | null;
  /** Its tempo, when the analysis is sure of one (decision 0035). */
  bpm: number | null;
  playing: boolean;
};

const tempos = new Map<string, number | null>();

/** The music now. `at` is the frame's time (performance.now()), to move the position on between the player's reports. */
export function musicNow(at: number): Music {
  const np = playing();
  if (!np) return { key: null, energy: null, bpm: null, playing: false };
  const key = `${np.source}/${np.id}`;
  loadWaveform(np.source, np.id);
  if (!tempos.has(key)) {
    tempos.set(key, null);
    trackState(np.source, np.id).then(
      (s) => tempos.set(key, s.info?.bpm ?? null),
      () => tempos.delete(key),
    );
  }
  const on = !paused();
  const wave = waveformOf(np.source, np.id);
  const length = np.progress?.duration_secs ?? null;
  return { key, energy: wave && length ? loudnessAt(wave, smoothPosition(key, on, at) / length) : null, bpm: tempos.get(key) ?? null, playing: on };
}

// The player reports its position a few times a second; between reports it moves on with the frame clock, so the
// loudness glides instead of stepping.
let anchor = { key: '', pos: 0, at: 0 };
function smoothPosition(key: string, on: boolean, at: number): number {
  const reported = position();
  if (anchor.key !== key || reported !== anchor.pos) anchor = { key, pos: reported, at };
  return on ? anchor.pos + (at - anchor.at) / 1000 : anchor.pos;
}

/** The waveform's loudness at `fraction` of the track, between its bins, against its loudest bin. */
function loudnessAt(bins: Uint8Array, fraction: number): number {
  let top = 1;
  for (const b of bins) if (b > top) top = b;
  const x = Math.min(Math.max(fraction, 0), 1) * (bins.length - 1);
  const i = Math.floor(x);
  const a = bins[i];
  const b = bins[Math.min(i + 1, bins.length - 1)];
  return (a + (b - a) * (x - i)) / top;
}
