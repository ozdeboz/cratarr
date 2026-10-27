// The downbeat at a place in a track, for a small light outside the visuals (the tape deck's head, the owner
// 2026-10-27): from the server's beats and bars only, never by listening (that costs a tablet much of a frame).
import { beatAt, beatsOf } from './beats';
import { ID, seriesAt, signalsOf } from './signals';

/** How long a downbeat's light takes to die away (seconds). */
const RING = 0.18;

/** 0..1: a bar's first beat just passed at `pos` (seconds) of the track, dying away; 0 where nothing is known yet. */
export function downbeatAt(source: string, id: string, pos: number): number {
  const beats = beatsOf(source, id);
  const signals = signalsOf(source, id);
  if (!beats || !signals || !signals.series.get(ID.bar)) return 0;
  const beat = beatAt(beats, pos);
  if (!beat) return 0;
  // A beat that starts its bar: the bar series' place at that beat is its first.
  const first = Math.round((seriesAt(signals, ID.bar, pos, beat.index)?.[0] ?? 1) * 255) === 0;
  return first ? Math.exp(-beat.since / RING) : 0;
}
