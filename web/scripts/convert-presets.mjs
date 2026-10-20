// Saved visuals as v2 presets (decision 0057, the switch): each input converted by toV2 as it is (the owner: no polish,
// no backwards compatibility beyond this), the cables listed in presets/pruned.json taken out (those that changed
// nothing, and the weak ones by eye; a `|*|` id is a route's six bands), and the plumbing left with nothing to do. Run with node's strip-types and tests/ts-resolve.mjs:
//   node --experimental-strip-types --import ./tests/ts-resolve.mjs scripts/convert-presets.mjs <file>… --only a,b --defaults --out dir
import { writeFileSync } from 'node:fs';
import { readFileSync } from 'node:fs';
import { readInputs } from './visuals-inputs.mjs';

const PRUNED = JSON.parse(readFileSync(new URL('../src/lib/visuals/presets/pruned.json', import.meta.url), 'utf8'));
const PLUMBING = new Set(['vca', 'merge', 'gate', 'shaper']);
function pruned(p, ids = []) {
  const gone = new Set(ids.flatMap((i) => (i.includes('|*|') ? [0, 1, 2, 3, 4, 5].map((b) => i.replace('|*|', `|${b}|`)) : [i])));
  p.cables = p.cables.filter((c) => !gone.has(c.id));
  for (;;) {
    const fed = new Set(p.cables.map((c) => c.to.module));
    const feeds = new Set(p.cables.map((c) => c.from.module));
    const dead = new Set(p.modules.filter((m) => PLUMBING.has(m.type) && (!fed.has(m.id) || !feeds.has(m.id))).map((m) => m.id));
    if (!dead.size) return p;
    p.modules = p.modules.filter((m) => !dead.has(m.id));
    p.cables = p.cables.filter((c) => !dead.has(c.from.module) && !dead.has(c.to.module) && !dead.has(c.depth?.module));
  }
}

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
  const id = name === 'DEFAULTS' ? 'default' : name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  writeFileSync(`${out}/${id}.json`, JSON.stringify(pruned(toV2(input.v1), PRUNED[id])));
  console.log(`${id}: ${name}`);
}
