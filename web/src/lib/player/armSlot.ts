// A stream the tape asks for ahead, as one value with its transitions (0.10; the architecture audit, 2026-10-29):
// the wind to a point's, a hold's asked at its touch, the direction knob's backwards one asked at its turn. Asked
// anew, the one before is let go (its stream dropped); taken, it is the taker's; dropped, its stream goes; one that
// gave up lets go of itself only if it is still the one held. lib/localPlayer's tape reads them. Pure.

export type ArmSlot<A> = {
  /** The stream held, or none. */
  readonly now: A | null;
  /** Holds `a`, the one before let go. */
  arm(a: A): void;
  /** The one held, now the caller's (not let go); none held after. */
  take(): A | null;
  /** The one held let go; none held after. */
  drop(): void;
  /** `a` no longer held, if it still is (it let go of its stream itself). */
  release(a: A): void;
};

export function armSlot<A>(letGo: (a: A) => void): ArmSlot<A> {
  let held: A | null = null;
  return {
    get now() {
      return held;
    },
    arm(a) {
      if (held !== null && held !== a) letGo(held);
      held = a;
    },
    take() {
      const a = held;
      held = null;
      return a;
    },
    drop() {
      if (held !== null) letGo(held);
      held = null;
    },
    release(a) {
      if (held === a) held = null;
    },
  };
}
