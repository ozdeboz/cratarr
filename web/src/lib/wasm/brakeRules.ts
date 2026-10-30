// The brake's rules from the Rust rules crate, as WebAssembly (the 0075 spike, the owner, 2026-10-30: "A as the joint
// spike, go"): the module's four exports made into startStop's BrakeRules. Pure, so the tests take the same module
// the page does (lib/wasm/brake.ts loads it there).
import type { BrakeRules } from '../startStop';

/** The module's exports, all f64 in and out: times in ms, speeds in ×, the way in ms of track. */
export type RulesExports = {
  brake_length_ms: (v0: number, to: number, brakeMs: number) => number;
  brake_v: (v0: number, to: number, lengthMs: number, tMs: number) => number;
  brake_way: (v0: number, to: number, lengthMs: number, tMs: number) => number;
};

export const brakeRulesOf = (x: RulesExports): BrakeRules => ({
  lengthMs: (v0, to, brakeMs) => x.brake_length_ms(v0, to, brakeMs),
  speed: (v0, to, ms, t) => x.brake_v(v0, to, ms, t),
  travel: (v, ms, s, to) => x.brake_way(v, to, ms, s * 1000) / 1000,
});

/** The module's bytes made ready (asynchronously: over 4 KB, Chrome compiles none on the main thread at once). */
export async function instantiateRules(bytes: BufferSource): Promise<RulesExports> {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return instance.exports as unknown as RulesExports;
}
