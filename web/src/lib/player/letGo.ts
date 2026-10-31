// A hold let go: its one plan (the owner, 2026-10-28: "make it pure"), and its brake handed from the wind's stream to
// the track's own player (lib/localPlayer land). The stretch the wind has played is never played again, and every head
// plays the brake at the wind's volume (the owner, 2026-10-31: the end of a long wind-down was heard twice, the second
// time as loud as the music). Pure.
import { brakeSpeed, brakeTravel } from '../startStop';

/** A place in the queue: which track, s into it. */
export type Place = { index: number; pos: number };
/** A hold let go: its one plan. From `start`, at `v0`× braking to `to`× on one raised cosine over `ms` from `b0`, along
 *  the queue places `order` the way `dir` goes, `D` s of track, to `aim`, the look-ahead; `skip`, the curve's way
 *  before `start` (s of track: 0, or carried on from where its sound was heard, heardFrom). What shows meanwhile is the
 *  plan's place; each head that sounds (the wind, the track's player, the backwards player) plays at the plan's speed,
 *  put right by how far it is from the plan's place (planSpeed); the brake ends at its aim. A browser's late rate
 *  changes (about 0.1 s) and its clock's jumps under load (the tablet's: 7 s of track at once at 128×) then never move
 *  where it lands. */
export type LetGo = { v0: number; to: number; b0: number; ms: number; start: Place; order: number[]; dir: 1 | -1; D: number; aim: Place; skip: number };
/** How fast a head is put back on the plan: what it is off, made up over this long. */
const PLAN_CATCH_S = 0.15;
/** A let-go's curve's speed at `now` (×). */
export const curveSpeed = (c: { v0: number; to: number; b0: number }, now: number, ms: number) => brakeSpeed(c.v0, c.to, ms, now - c.b0);
/** What letting a hold go does (the owner, 2026-10-31): not sounding yet, its fallback at once, its clip never sounding
 *  after it; braking already or handed over (let go twice), nothing; else its one brake. */
export const releaseOf = (w: { sounding: boolean; braking: boolean; handing: boolean }): 'fallback' | 'nothing' | 'brake' =>
  !w.sounding ? 'fallback' : w.braking || w.handing ? 'nothing' : 'brake';
/** The volume a let-go's brake sounds at on every head (× the music's): the wind's, from its gain (dB). */
export const brakeGain = (gainDb: number) => 10 ** (gainDb / 20);
/** A backward let-go's brake handed from the wind's stream to the backwards player, whose stream begins at `armed`
 *  (lib/localPlayer handBack): once the wind is heard there (`heard` within 0.02 s short of it), from where it is heard,
 *  its stream sought on by what the wind has played past its start (`seek`, s); missed where the wind is in another
 *  track or past what the stream has loaded (`room`, s past its start; at least 0.25). The mirror of handOver: the
 *  stretch the wind played never played again (the owner, 2026-10-31). */
export function takeBack(heard: Place, armed: Place, room: number): 'wait' | 'missed' | { at: Place; seek: number } {
  if (heard.index !== armed.index || armed.pos - heard.pos > Math.max(0.25, room)) return 'missed';
  if (heard.pos > armed.pos + 0.02) return 'wait';
  const seek = Math.max(0, armed.pos - heard.pos);
  return { at: { index: armed.index, pos: armed.pos - seek }, seek };
}

/** The plan's ways over the queue, each queue place's length from `len` (null: unknown). */
export function letGoPlans(len: (index: number) => number | null) {
  /** `d` s on from `place` the way `dir` goes along the queue places `order`, past a track's edge into the next. */
  function along(order: number[], dir: 1 | -1, place: Place, d: number): Place {
    let k = order.indexOf(place.index);
    let { index, pos } = place;
    for (;;) {
      const length = len(index) ?? Infinity;
      const room = dir < 0 ? pos : length - pos;
      if (d <= room || k < 0 || k + 1 >= order.length) return { index, pos: Math.max(0, Math.min(length, pos + dir * Math.min(d, room))) };
      d -= room;
      index = order[++k];
      pos = dir < 0 ? (len(index) ?? 0) : 0;
    }
  }
  /** How far a plan's way has come from its start to `place` (s of track; null: a place off its way). */
  function wayTo(c: LetGo, place: Place): number | null {
    if (place.index === c.start.index) return (place.pos - c.start.pos) * c.dir;
    const a = c.order.indexOf(c.start.index);
    const b = c.order.indexOf(place.index);
    if (a < 0 || b < a) return null;
    const at = (i: number) => len(c.order[i]) ?? 0;
    let d = c.dir > 0 ? at(a) - c.start.pos : c.start.pos;
    for (let i = a + 1; i < b; i++) d += at(i);
    return d + (c.dir > 0 ? place.pos : at(b) - place.pos);
  }
  /** The plan's place at `now`. */
  const planAt = (c: LetGo, now: number): Place => along(c.order, c.dir, c.start, brakeTravel(c.v0, c.ms, Math.max(0, (now - c.b0) / 1000), c.to) - c.skip);
  /** The speed a head at `place` plays at now (×, the track's): the plan's, and what it is off the plan's place made up
   *  over PLAN_CATCH_S; never backwards. */
  function planSpeed(c: LetGo, now: number, place: Place): number {
    const v = curveSpeed(c, now, c.ms);
    const got = wayTo(c, place);
    if (got === null) return v;
    const want = brakeTravel(c.v0, c.ms, Math.min(c.ms / 1000, Math.max(0, (now - c.b0) / 1000)), c.to) - c.skip;
    return Math.max(0, v + (want - got) / PLAN_CATCH_S);
  }
  /** The plan carried on from where its sound is heard at `now`: the same curve, from `heard`; its aim where that
   *  brings it. A head held off the plan (a wind's stream played on at its slowest after a stall of the page, 5.6 s of
   *  track ahead at 15×) is taken over where it is, never where the plan says. */
  function heardFrom(c: LetGo, now: number, heard: Place): LetGo {
    const p = { ...c, start: heard, skip: brakeTravel(c.v0, c.ms, Math.min(c.ms / 1000, Math.max(0, (now - c.b0) / 1000)), c.to) };
    return { ...p, aim: planAt(p, c.b0 + c.ms) };
  }
  /** A forward let-go's brake handed from the wind's stream to the track's own player, once its curve is within the
   *  track's reach (`top`×): where the wind is heard, at the curve's speed, the plan carried on from there; null while
   *  the curve is still above. */
  function handOver(c: LetGo, now: number, heard: Place, top: number): { plan: LetGo; at: Place; speed: number } | null {
    const v = curveSpeed(c, now, c.ms);
    if (v > top) return null;
    return { plan: heardFrom(c, now, heard), at: heard, speed: Math.min(top, v) };
  }
  return { along, wayTo, planAt, planSpeed, heardFrom, handOver };
}
