// The one rule table (shared/rules.json; the audit, 2026-10-29): the page's player rules checked against the same
// numbers the server's and the app's tests read. A rule changed there and not here fails here. wind_shortest_ms is
// the server's alone (the page asks it for its winds' streams).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const rules = JSON.parse(
  readFileSync(new URL("../../shared/rules.json", import.meta.url), "utf8"),
);
const {
  brakeLengthMs,
  brakeSpeed,
  brakeTravel,
  DECEL_X,
  MOTION,
  RAMP_MS,
  RESTART_AFTER,
  TAPE,
  WIND_CAP_MS,
} = await import("../src/lib/startStop.ts");

test("the single numbers", () => {
  assert.equal(
    RESTART_AFTER * 1000,
    rules.restart_after_ms,
    "Previous's restart",
  );
  assert.equal(WIND_CAP_MS, rules.wind_cap_ms, "a wind to a point at most");
  assert.equal(DECEL_X, rules.decel_x, "a held wind's brake's deceleration");
  assert.deepEqual(RAMP_MS, rules.start_stop_ms, "Start and stop's range");
});

test("the tape's ranges", () => {
  const t = rules.tape;
  assert.deepEqual({ ...TAPE.attackMs }, t.attack_ms);
  assert.deepEqual({ ...TAPE.brakeMs }, t.brake_ms);
  assert.deepEqual({ ...TAPE.gainDb }, t.gain_db);
  assert.deepEqual({ ...MOTION.toStart }, t.point);
  assert.deepEqual({ ...MOTION.holdBack }, t.hold);
});

const { brakeRulesOf, instantiateRules } =
  await import("../src/lib/wasm/brakeRules.ts");
const { JS_BRAKE, useBrake } = await import("../src/lib/startStop.ts");
const wasm = brakeRulesOf(
  await instantiateRules(
    readFileSync(new URL("../src/lib/wasm/rules.wasm", import.meta.url)),
  ),
);

// Each way the page can take its brake (the 0075 spike: the Rust rules crate's, as WebAssembly, and its own).
for (const [way, rules_] of [
  ["JS", JS_BRAKE],
  ["WebAssembly", wasm],
] as const)
  test(`the brake's worked cases (${way})`, () => {
    useBrake(rules_);
    for (const c of rules.brake) {
      const ms = brakeLengthMs(c.v0, c.to, c.brake_ms);
      assert.equal(ms, c.length_ms, `${c.v0}× to ${c.to}×: its length`);
      for (const p of c.at) {
        assert.ok(
          Math.abs(brakeSpeed(c.v0, c.to, ms, p.t_ms) - p.v) < 1e-6,
          `${c.v0}× at ${p.t_ms} ms: its speed`,
        );
        assert.ok(
          Math.abs(
            brakeTravel(c.v0, ms, p.t_ms / 1000, c.to) * 1000 - p.way_ms,
          ) < 1e-3,
          `${c.v0}× at ${p.t_ms} ms: its way`,
        );
      }
    }
  });

test("the page goes back to its own brake", () => useBrake(JS_BRAKE));
