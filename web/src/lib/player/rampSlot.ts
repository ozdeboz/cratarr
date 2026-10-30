// The tape's ramp as one value (0.10; the architecture audit, 2026-10-29: the tape's two dozen flags made from what
// it is doing): a ramp under way has what it committed to (paused or playing) and where it holds the counter, or there
// is none. The counter is held only by a ramp, so with none there is no counter held: a state the tape's check once
// had to put right (lib/player/tape) cannot be. lib/localPlayer's `tape.committed` and `tape.heldAt` read and write
// it. Pure.

/** A ramp under way: `committed` paused (true) or playing (false); `heldAt` the counter's place while it sounds. */
export type Ramp = { committed: boolean; heldAt: number | null };

export type RampSlot = {
  /** What the ramp under way has committed to; null: no ramp. Set null, the ramp ends, its counter let go. */
  committed: boolean | null;
  /** Where the ramp holds the counter (stream time); null: where it plays. Held only while a ramp runs. */
  heldAt: number | null;
  /** The ramp itself, or none. */
  readonly ramp: Ramp | null;
};

export function rampSlot(): RampSlot {
  let ramp: Ramp | null = null;
  return {
    get committed() {
      return ramp?.committed ?? null;
    },
    set committed(v: boolean | null) {
      ramp = v === null ? null : { committed: v, heldAt: ramp?.heldAt ?? null };
    },
    get heldAt() {
      return ramp?.heldAt ?? null;
    },
    set heldAt(v: number | null) {
      if (ramp) ramp = { ...ramp, heldAt: v };
    },
    get ramp() {
      return ramp;
    },
  };
}
