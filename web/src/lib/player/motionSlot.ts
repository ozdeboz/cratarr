// The tape's motion as one value (0.10; the architecture audit, 2026-10-29): at most one of a wind sounding on this
// page's player, a hold on a track that cannot be wound (it plays faster on its own player), or a wind on the player
// driven, whose counter glides between its reports. They never ran together: a wind is the page's own player's, a glide
// the app's or a driven player's, a plain hold only where nothing winds; one value says so. lib/localPlayer's
// `tape.rewinding`, `tape.plainHold` and `tape.glide` read and write it: a motion begun replaces any other, one ended
// ends only itself. Pure.

export type Motion<W, P, G> = { kind: 'wind'; wind: W } | { kind: 'plainHold'; hold: P } | { kind: 'glide'; glide: G } | null;

export type MotionSlot<W, P, G> = {
  rewinding: W | null;
  plainHold: P | null;
  glide: G | null;
  readonly motion: Motion<W, P, G>;
};

export function motionSlot<W, P, G>(): MotionSlot<W, P, G> {
  let motion: Motion<W, P, G> = null;
  return {
    get rewinding() {
      return motion?.kind === 'wind' ? motion.wind : null;
    },
    set rewinding(v: W | null) {
      if (v !== null) motion = { kind: 'wind', wind: v };
      else if (motion?.kind === 'wind') motion = null;
    },
    get plainHold() {
      return motion?.kind === 'plainHold' ? motion.hold : null;
    },
    set plainHold(v: P | null) {
      if (v !== null) motion = { kind: 'plainHold', hold: v };
      else if (motion?.kind === 'plainHold') motion = null;
    },
    get glide() {
      return motion?.kind === 'glide' ? motion.glide : null;
    },
    set glide(v: G | null) {
      if (v !== null) motion = { kind: 'glide', glide: v };
      else if (motion?.kind === 'glide') motion = null;
    },
    get motion() {
      return motion;
    },
  };
}
