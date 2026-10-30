// The effects as modules seen as today's slots (decision 0057 §3, effect-modules.ts): what the chain's columns, the
// settings view, the dice and the moments still say by slot (fx1–fx3 and the loop's), worked out from the effect
// modules a patch holds. An effect is in one engine's list, so its type gives its picture slot; the loop's is
// `loop-<name>` (toV2's id).

import { effectKnobIds, effectOfType, effectType, type Slot } from '../effect-modules';
import { ENGINE_EFFECTS, LOOP_EFFECTS, type Effect, type Engine } from '../effects';
import type { Module } from '../patch';

export type { Slot };
export const SLOT_IDS: readonly Slot[] = ['fx1', 'fx2', 'fx3', 'loop-effect'];
const SLOT_OF: Record<Engine, Slot> = { kaleidoscope: 'fx1', eyedo: 'fx2', hydra: 'fx3' };
/** Each slot's settings (settings.svelte.ts): its effect, Amount and two controls. */
export const SLOT_SETTINGS: Record<Slot, { effect: string; amount: string; k1: string; k2: string }> = {
  fx1: { effect: 'fx1', amount: 'fx1Amount', k1: 'fx1K1', k2: 'fx1K2' },
  fx2: { effect: 'fx2', amount: 'fx2Amount', k1: 'fx2K1', k2: 'fx2K2' },
  fx3: { effect: 'fx3', amount: 'fx3Amount', k1: 'fx3K1', k2: 'fx3K2' },
  'loop-effect': { effect: 'fxLoop', amount: 'fxLoopAmount', k1: 'fxLoopK1', k2: 'fxLoopK2' },
};

/** The slot an effect module stands for (its id flattened or not); null for any other module. */
export function slotOf(id: string, type: string): Slot | null {
  const e = effectOfType(type);
  if (!e) return null;
  return id.slice(id.lastIndexOf('/') + 1).startsWith('loop-') ? 'loop-effect' : SLOT_OF[e.engine];
}

/** The slot's knob an effect module's knob is: Amount as it is, its own knobs K1 and K2; null for any other. */
export function slotKnobOf(type: string, knob: string): 'amount' | 'k1' | 'k2' | null {
  const e = effectOfType(type);
  if (!e) return null;
  if (knob === 'amount') return 'amount';
  const i = effectKnobIds(e).indexOf(knob);
  return i === 0 ? 'k1' : i === 1 ? 'k2' : null;
}

/** The slot's list of effects (its step's places, from 1). */
const listOf = (slot: Slot): readonly Effect[] => (slot === 'loop-effect' ? LOOP_EFFECTS : ENGINE_EFFECTS[(Object.keys(SLOT_OF) as Engine[]).find((e) => SLOT_OF[e] === slot)!]);

/** The slot's step for the effect a module of `type` is (its place in the slot's list, from 1); 0 for none. */
export const placeOf = (slot: Slot, type: string): number => listOf(slot).findIndex((e) => effectType(e) === type) + 1;

/** The effect at a slot's step (effects.ts id), 0 for none. */
export const effectAt = (slot: Slot, place: number): number => listOf(slot)[Math.round(place) - 1]?.id ?? 0;

/** The effect modules of a (flattened) patch, by the slot each stands for (the first, where two say one). */
export function slotModules(modules: readonly Module[]): Map<Slot, Module> {
  const out = new Map<Slot, Module>();
  for (const m of modules) {
    const slot = slotOf(m.id, m.type);
    if (slot && !out.has(slot)) out.set(slot, m);
  }
  return out;
}

/** An effect module's own knob for a slot's K1/K2 (null where the effect has none). */
export function ownKnob(type: string, k: 'k1' | 'k2'): string | null {
  const e = effectOfType(type);
  return (e && effectKnobIds(e)[k === 'k1' ? 0 : 1]) ?? null;
}
