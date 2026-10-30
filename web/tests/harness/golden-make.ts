// Makes the golden frames' references again (tests/golden): `node --experimental-strip-types --import
// ./tests/ts-resolve.mjs tests/harness/golden-make.ts [rack…]`. Only when a change of the picture is meant.
const { RACKS, goldenFrames, writeReference } = await import('./golden.ts');
const only = process.argv.slice(2);
for (const r of RACKS) {
  if (only.length && !only.includes(r.id)) continue;
  const t0 = performance.now();
  const frames = goldenFrames(r.patch, r.run);
  writeReference(r.id, frames);
  console.log(`${r.id}: ${frames.length} frames, ${frames[0]?.size ?? 0} numbers a frame, ${Math.round(performance.now() - t0)} ms`);
}
