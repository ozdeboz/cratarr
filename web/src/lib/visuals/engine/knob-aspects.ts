// What each knob means, for the aspect dice (the owner, 2026-10-30: "all nodes should select by what they mean"):
// Spatial, Motion, Colours and Lightness each roll every knob of their meaning in any module, a source's and the loop's
// as much as a Grid's or a Fold's (before, Spatial was a list of module types and never reached a source).
//
// Each module declares its knobs' meanings where it declares its knobs (`withAspects`), so a renamed knob keeps its
// meaning and a new one shows as undeclared in tests/knob-aspects.test.ts instead of silently falling out of a scope.
// A modulator's knobs (how it follows, its dynamics) mean none of the four unless it declares one (a Random's Rate:
// how fast what it moves moves). (One table by knob id, as these were first kept, gave a knob its namesakes' meaning:
// a Dice's Reach, a Sequencer's Length and a Shaper's handles came out spatial.)

import type { Knob, KnobAspect } from '../patch';

export type { KnobAspect } from '../patch';

/** `knobs` with their meanings: each named in `of` gets that one, each other keeps the one it declares, else `rest`
 *  (`null` for a module none of whose knobs mean any of the four; left out, the knob stays undeclared). */
export function withAspects<K extends Knob>(knobs: readonly K[], of: Readonly<Record<string, KnobAspect | null>>, rest?: null): K[] {
  return knobs.map((k) => {
    const aspect = k.id in of ? of[k.id] : k.aspect !== undefined ? k.aspect : rest;
    return aspect === undefined ? k : { ...k, aspect };
  });
}

/** A knob's meaning as its module declares it; `undefined` when it declares none (the old matching then decides, and
 *  the test lists it). A polyphonic knob's channels (`lighting.3`) carry their knob's. */
export const aspectOf = (knob: Pick<Knob, 'aspect'>): KnobAspect | null | undefined => knob.aspect;
