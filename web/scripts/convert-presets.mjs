// Saved visuals as v2 presets (decision 0057, the switch): each input converted by toV2 as it is (the owner: no polish,
// no backwards compatibility beyond this), written as a preset's patch. Run with node's strip-types and tests/ts-resolve.mjs:
//   node --experimental-strip-types --import ./tests/ts-resolve.mjs scripts/convert-presets.mjs <file>… --only a,b --defaults --out dir
import { writeFileSync } from 'node:fs';
import { readInputs } from './visuals-inputs.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  if (i < 0) return fallback;
  return args.splice(i, 2)[1];
};
const flag = (name) => {
  const i = args.indexOf(`--${name}`);
  if (i >= 0) args.splice(i, 1);
  return i >= 0;
};
const out = option('out', '.');
const only = option('only', '');
const defaults = flag('defaults');
await import('../src/lib/visuals/picture-wiring.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
for (const input of readInputs(args, { defaults }).filter((x) => !only || only.split(',').some((o) => x.name.toLowerCase().includes(o.toLowerCase())))) {
  const name = input.name.replace(/^snapshot "|"$/g, '').replace(/ · \d\d:\d\d$/, '');
  const id = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  writeFileSync(`${out}/${id}.json`, JSON.stringify(toV2(input.v1)));
  console.log(`${id}: ${name}`);
}
