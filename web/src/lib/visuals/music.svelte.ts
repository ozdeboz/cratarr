// What the visuals follow (decision 0046): the playing track's beats where they fall (its analysis, worked out on the
// server), its loudness where it is (its waveform, decision 0035), its tempo, and whether it plays. Nothing listens to
// the sound itself: in the app the music plays natively, out of the page's reach, and on another player it plays
// elsewhere. Read once per frame by the visuals' loop.
import { trackState } from '../api';
import { positionNow } from '../localPlayer.svelte';
import { isLocal } from '../output.svelte';
import { paused, playing } from '../remote.svelte';
import { loadWaveform, waveformOf } from '../waveforms.svelte';
import { beatAt, beatsOf, type Beat } from './beats';

export type Music = {
  /** The playing track (`source/id`), or null. */
  key: string | null;
  /** Its loudness here, 0..1 of its loudest moment (its waveform), or null where it has none. */
  energy: number | null;
  /** Its tempo, when the analysis is sure of one (decision 0035). */
  bpm: number | null;
  /** The beat it is at, where it has beats (a tempo) and they are known here yet. */
  beat: Beat | null;
  playing: boolean;
};

/** The picture is drawn this far ahead of the sound (seconds): a frame reaches the screen a frame or two after it is
 *  drawn, and a flash a little early reads as on the beat, a little late as after it. */
const AHEAD = 0.05;

const tempos = new Map<string, number | null>();

/** The music now. `at` is the frame's time (performance.now()), to move the position on between the player's reports. */
export function musicNow(at: number): Music {
  const np = playing();
  if (!np) return { key: null, energy: null, bpm: null, beat: null, playing: false };
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
  const pos = positionAt(key, on, at) + (on ? AHEAD : 0);
  const beats = beatsOf(np.source, np.id);
  // The next track's too, so it pulses from its first beat.
  if (np.upcoming[0]) beatsOf(np.source, np.upcoming[0].id);
  const wave = waveformOf(np.source, np.id);
  const length = np.progress?.duration_secs ?? null;
  return {
    key,
    energy: wave && length ? loudnessAt(wave, pos / length) : null,
    bpm: tempos.get(key) ?? null,
    beat: beats ? beatAt(beats, pos) : null,
    playing: on,
  };
}

// The position at the frame. This device's player (the app's, or the page's own audio) says it to the frame. Another
// player reports it now and then, through the server: between reports it moves on with the frame clock, and it is
// eased toward each report rather than set to it, since a jump back or forth would be a beat played twice or skipped;
// a report far off (a seek) is taken as it is.
let clock = { key: '', pos: 0, at: 0, heard: Number.NaN };
function positionAt(key: string, on: boolean, at: number): number {
  if (isLocal()) return positionNow();
  const heard = playing()?.progress?.position_secs ?? 0;
  if (clock.key !== key) clock = { key, pos: heard, at, heard };
  if (on) clock.pos += (at - clock.at) / 1000;
  clock.at = at;
  if (heard !== clock.heard) {
    const off = heard - clock.pos;
    clock.pos += Math.abs(off) > 0.3 ? off : off / 2;
    clock.heard = heard;
  }
  return clock.pos;
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
