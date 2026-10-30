// The rest of the tape's state, each part one value with its transitions (0.10; the architecture audit, 2026-10-29).
// Read from every write in lib/localPlayer: the tape's own ramp (quiet, and the turn's own ramp) may stand together
// or apart (a turn sets both, a skip's quiet only, a handover the turn's ramp with or without quiet); a Next kept for a
// pause's standstill and one kept for Autoplay's picks may both be waiting (Next on the last track while Autoplay
// fetches, then a pause, then Next during its brake); a hold on the player driven stays shown while its let-go lands.
// So each is one value whose fields change through its own transitions only. Pure.

/** The tape's own ramp: `quiet`, the tape's (shown as playing, not an effect's stop); `turn`, its own vinyl ramp over
 *  the spin-up and the Brake (a knob's turn, a hold's), whatever Start and stop says. */
export type OwnRamp<T> = { quiet: boolean; turn: T | null };
export const NO_OWN_RAMP: OwnRamp<never> = { quiet: false, turn: null };
/** The own ramp as a ramp beginning now should read it: set before `go`, let go after (`turned`, `quietly`). */
export const ownRamp = <T>(quiet: boolean, turn: T | null): OwnRamp<T> => ({ quiet, turn });

/** A Next waiting for its moment: the next track once a pause's brake stands still (`afterPause`), or Autoplay's
 *  first pick once it is in (`whenFilled`). */
export type PendingNext = { afterPause: boolean; whenFilled: boolean };
export const NO_PENDING_NEXT: PendingNext = { afterPause: false, whenFilled: false };
export const pendingNext = {
  /** Next pressed while a pause brakes. */
  afterPause: (p: PendingNext): PendingNext => ({ ...p, afterPause: true }),
  /** The brake stood still (or turned round): that Next done with. */
  pauseDone: (p: PendingNext): PendingNext => ({ ...p, afterPause: false }),
  /** Next on the last track while Autoplay fetches. */
  whenFilled: (p: PendingNext): PendingNext => ({ ...p, whenFilled: true }),
  /** Autoplay's picks are in (or none came): that Next done with. */
  filled: (p: PendingNext): PendingNext => ({ ...p, whenFilled: false }),
};

/** A hold sent to the player driven: `held` from its press until its wind has landed (its reported pauses not shown),
 *  `letGo` once let go. */
export type DrivenHold = { held: boolean; letGo: boolean };
export const NO_DRIVEN_HOLD: DrivenHold = { held: false, letGo: false };
export const drivenHold = {
  /** Pressed: held, not let go. */
  press: (): DrivenHold => ({ held: true, letGo: false }),
  /** Let go: still held until it lands. */
  letGo: (h: DrivenHold): DrivenHold => ({ ...h, letGo: true }),
  /** Landed, or its grace passed: over (let go stays said). */
  over: (h: DrivenHold): DrivenHold => ({ ...h, held: false }),
  /** Whether the player's word that it no longer winds ends it (a moment after): only after a let-go. */
  landing: (h: DrivenHold) => h.held && h.letGo,
};
