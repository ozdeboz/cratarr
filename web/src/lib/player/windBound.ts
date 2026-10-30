// A wind lives only inside its commitment (0.10; the architecture audit, 2026-10-29). The tape's ramp (rampSlot)
// and its motion (motionSlot) are tied here: a wind begun is committed to playing from its first moment; a ramp ended
// outright while a wind sounds (a new queue, a Stop, a seek during a ramp) ends the wind first through its own end
// (`endWind`, lib/localPlayer's endRewind: its clip stopped, its parts let go), then the ramp. Pure.
import type { MotionSlot } from './motionSlot';
import type { RampSlot } from './rampSlot';

export type WindBound<W> = { commit(v: boolean | null): void; wind(w: W | null): void };

export function windBound<W, P, G>(ramp: RampSlot, motion: MotionSlot<W, P, G>, endWind: () => void): WindBound<W> {
  return {
    commit(v) {
      if (v === null && motion.rewinding !== null) endWind();
      ramp.committed = v;
    },
    wind(w) {
      motion.rewinding = w;
      if (w !== null && ramp.committed === null) ramp.committed = false;
    },
  };
}
