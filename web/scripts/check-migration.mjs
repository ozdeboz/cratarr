// Migration check (decision 0057): saved visuals, as a device keeps them or as the server keeps its snapshots, run
// through the v1 → v2 conversion (v2.ts `toV2`, `flatten`), and the patch that comes out checked:
//   1. nothing dropped: every route set to something, every module and its knobs (where they moved to), every extra
//      module, cable and layer, the macros and pins, land in v2;
//   2. no dangling cable ends: each end's module is there, its port is its type's (ports.ts `portsOf`), its channel one
//      the port has;
//   3. the types fit (ports.ts `fits`);
//   4. every picture cycle passes through a Feedback (ports.ts `cyclesClosed`);
// and, with --picture, the picture side: each one recorded in today's renderer and replayed through the v2 picture
// graph (picture-lab.html, the harness of scripts/check-picture.mjs), frame by frame (`pictureCheck`).
//
//   node --experimental-strip-types --import ./tests/ts-resolve.mjs scripts/check-migration.mjs <file>… [--chains]
//        [--picture] [--base http://localhost:5173] [--frames 150] [--bits 32] [--only name,name] [--json out.json]
//
// A file is a device's kept settings (`{"cratarr-visuals": "<json>"}`, as localStorage holds them), the server's
// snapshots (a JSON list of rows with `name` and `settings`, as `sqlite3 -json` gives them from visual_snapshot or
// /api/visuals/snapshots answers), or one settings object. Each is read as the app reads it (settings.svelte.ts
// `load`, Visuals.svelte `recall`). --chains adds the built-in chains and examples (chains.ts). The data stays where
// it is: nothing here writes anywhere but --json's file. Exits 1 when a check fails.
import { writeFileSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { copy, openBrowser, readInputs } from './visuals-inputs.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  if (i < 0) return fallback;
  const [, value] = args.splice(i, 2);
  return value;
};
const flag = (name) => {
  const i = args.indexOf(`--${name}`);
  if (i >= 0) args.splice(i, 1);
  return i >= 0;
};
const base = option('base', 'http://localhost:5173').replace(/\/$/, '');
const frames = Number(option('frames', 150));
const bits = option('bits', '32').split(',').map(Number);
const only = option('only', '');
const jsonOut = option('json', '');
const withChains = flag('chains');
const withPicture = flag('picture');
const files = args;

const src = '../src/lib/visuals/';
const { DEFAULTS, RANGES, BEHAVIOUR_PINS, BAND_PINS, isPinnable } = await import(`${src}settings.svelte.ts`);
const { emptyExtra, cratarrPatch, modulesOf, cratarrTypes, ASPECT_KNOB } = await import(`${src}patch.ts`);
const { isRoute, keyNow, COLUMN_SCALES } = await import(`${src}routes.ts`);
const { PRESET_KEYS } = await import(`${src}flame.ts`);
const { EXTRA_TYPES } = await import(`${src}registry.ts`);
const { ENGINE_MODULES, GRAPH_MODULES, showPartTypes, portsOf, fits, cyclesClosed, isDevice, SHOW_PARTS } = await import(`${src}ports.ts`);
// The picture side's wiring (claude-632's, step 3), where this version has it: it installs itself into v2.ts.
const wiring = await import(`${src}picture-wiring.ts`).catch(() => null);
const { toV2, flatten } = await import(`${src}v2.ts`);

const BANDS = 6;
const chain = cratarrTypes(RANGES, DEFAULTS);
const TYPES = new Map([...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show'))].map((t) => [t.type, t]));
const typeOf = (t) => TYPES.get(t);

const inputs = readInputs(files, { chains: withChains });

// ---- The checks -------------------------------------------------------------------------------------------------

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const isZero = (v) => (typeof v === 'number' ? v === 0 : !!v && v.h === 0 && v.h1?.[1] === 0 && v.h2?.[1] === 0);

/** What the reading of saved data leaves out before the conversion (by the owner's choice: form 3 does not read what
 *  it replaced). Not a failure of the conversion; reported so it is known. */
function readingLosses(raw, v1) {
  const out = [];
  const routes = raw.routes && typeof raw.routes === 'object' ? raw.routes : {};
  for (const [k, v] of Object.entries(routes)) {
    const now = keyNow(k);
    if (!isRoute(now, BANDS)) {
      if (!isZero(v)) out.push(`route ${k} = ${JSON.stringify(v)}: no such route now`);
    } else if (now !== k && !isZero(v)) out.push(`route ${k} read as ${now}`);
  }
  for (const k of Object.keys(raw)) if (!(k in DEFAULTS) && !['v', 'keptForm', 'routes'].includes(k)) out.push(`setting ${k} = ${JSON.stringify(raw[k])}: not a setting now`);
  return out;
}

/** The column scales folded into their cables' amounts (v2.ts, 0057 §7e item 2): by column knob, its setting; the
 *  knob itself 1 in v2. Stir, Flow and Swirl are scaled where the fluid stirs. */
const SCALE_OF = new Map(Object.entries(COLUMN_SCALES).filter(([a]) => !['stir', 'flow', 'swirl'].includes(a)).map(([a, key]) => [ASPECT_KNOB[a].join('.'), key]));
const scaled = (v, k) => (k === 1 ? v : typeof v === 'number' ? v * k : { ...v, h1: [v.h1[0], v.h1[1] * k], h2: [v.h2[0], v.h2[1] * k], h: v.h * k });
const near = (a, b) => (typeof a === 'number' && typeof b === 'number' ? Math.abs(a - b) <= 1e-12 : same(a, b) || (a && b && near(a.h, b.h) && near(a.h1?.[1], b.h1?.[1]) && near(a.h2?.[1], b.h2?.[1]) && a.h1?.[0] === b.h1?.[0] && a.h2?.[0] === b.h2?.[0] && !!a.walks === !!b.walks));

/** Where a v1 module's knob lives in the flattened v2 patch (v2.ts's contract). */
function knobHome(module, knob) {
  if (module === 'frame' && knob === 'intensity') return [['envelope', 'intensity']];
  if (module === 'frame' && knob === 'flash') return [['accent', 'accent']];
  if (module === 'frame' && knob === 'speed') return [['clock', 'speed']];
  const band = /^lighting\.(\d)$/.exec(knob);
  if (module === 'flame' && band) return [['music', `hold.${band[1]}`]];
  if (module === 'show' && knob === 'size') return [['flame', 'zoom']];
  if (module === 'show') {
    const part = SHOW_PARTS.find((p) => p.knob === knob);
    return part ? [[`show/${part.type}`, knob]] : [];
  }
  return [[module, knob]];
}

/** The port a cable end names on its module's type, and why not where there is none. */
function portAt(modules, end, side) {
  const m = modules.get(end.module);
  if (!m) return { error: `${side} module ${end.module} is not in the patch` };
  const t = typeOf(m.type);
  if (!t) return { error: `${side} module ${end.module}'s type ${m.type} is not known` };
  const port = portsOf(t)[side === 'from' ? 'out' : 'in'].find((p) => p.id === end.port);
  if (!port) return { error: `${side} ${end.module} (${m.type}) has no ${side === 'from' ? 'output' : 'input'} ${end.port}` };
  if (end.channel !== undefined && !(port.channels && end.channel >= 0 && end.channel < port.channels))
    return { error: `${side} ${end.module}.${end.port} has no channel ${end.channel} (${port.channels ?? 1} channel${port.channels > 1 ? 's' : ''})` };
  return { port };
}

function check(input) {
  const { v1 } = input;
  const p = cratarrPatch(v1);
  const v2 = toV2(copy(v1));
  const f = flatten(v2);
  const fails = { dropped: [], dangling: [], fit: [], cycles: [] };
  const notes = [];
  const byId = new Map(v2.cables.map((c) => [c.id, c]));
  const v2Modules = new Map(v2.modules.map((m) => [m.id, m]));
  const flat = new Map(f.modules.map((m) => [m.id, m]));

  // 1. Nothing dropped. The routes set to something:
  for (const [key, value] of Object.entries(v1.routes)) {
    if (!isRoute(key, BANDS) || isZero(value)) continue;
    const c = byId.get(key);
    const [module, knob] = ASPECT_KNOB[key.split('|')[2]];
    const want = module === 'show' && knob === 'size' ? { module: 'flame', port: 'zoom' } : { module, port: knob };
    if (!c) fails.dropped.push(`route ${key} = ${JSON.stringify(value)}: no cable`);
    else if (!same(c.to, want)) fails.dropped.push(`route ${key}: into ${c.to.module}.${c.to.port}, not ${want.module}.${want.port}`);
    else if (!near(c.value, scaled(value, v1[SCALE_OF.get(`${module}.${knob}`)] ?? 1)))
      fails.dropped.push(`route ${key}: value ${JSON.stringify(c.value)}, was ${JSON.stringify(value)} (its column's scale ${v1[SCALE_OF.get(`${module}.${knob}`)] ?? 1})`);
  }
  // Every module and its knobs, where they moved to:
  for (const m of modulesOf(p).values()) {
    if (!v2Modules.has(m.id)) {
      fails.dropped.push(`module ${m.id} (${m.type}): not in v2`);
      continue;
    }
    for (const [k, value] of Object.entries(m.knobs)) {
      const homes = knobHome(m.id, k);
      if (!homes.length) fails.dropped.push(`knob ${m.id}.${k} = ${value}: nowhere in v2`);
      for (const [hm, hk] of homes) {
        const got = flat.get(hm)?.knobs[hk];
        if (got === undefined) fails.dropped.push(`knob ${m.id}.${k} = ${value}: ${hm}.${hk} is not in v2`);
        // A column's scale: in its cables' amounts (checked with the routes), the knob at 1.
        else if (got !== (SCALE_OF.has(`${m.id}.${k}`) ? 1 : value)) fails.dropped.push(`knob ${m.id}.${k} = ${value}: ${hm}.${hk} = ${got}`);
      }
    }
    if (m.type === 'drift' && v2Modules.get(m.id).type !== 'random') fails.dropped.push(`module ${m.id}: a Drift, now ${v2Modules.get(m.id).type}`);
  }
  // The extras' modules, cables and layers:
  const extra = v1.extra ?? emptyExtra();
  for (const c of extra.cables) if (!byId.has(c.id)) fails.dropped.push(`extra cable ${c.id}: no cable`);
  const layers = p.layers;
  const mix = f.modules.find((m) => m.id === 'layers' && m.type === 'mix');
  layers.forEach((l, i) => {
    for (const m of [l.source, ...l.modules, ...l.loop]) if (!v2Modules.has(m.id)) fails.dropped.push(`layer ${l.id}: its module ${m.id} is not in v2`);
    if (!wiring) return;
    const n = i + 1;
    const into = f.cables.find((c) => c.to.module === 'layers' && c.to.port === `in${n}`);
    if (!mix || !into) fails.dropped.push(`layer ${l.id}: no cable into the layers' Mix (in${n})`);
    else {
      const last = l.modules.length ? l.modules[l.modules.length - 1].id : l.source.id;
      const fromLast = into.from.module === last || into.from.module.startsWith(`${last}/`);
      if (!fromLast) fails.dropped.push(`layer ${l.id}: the Mix's in${n} comes from ${into.from.module}, not ${last}`);
      if (mix.knobs[`amount${n}`] !== l.blend.amount) fails.dropped.push(`layer ${l.id}: blend amount ${l.blend.amount}, the Mix's amount${n} ${mix.knobs[`amount${n}`]}`);
    }
  });
  if (!wiring && layers.length) notes.push(`the layers' blends (${layers.map((l) => `${l.id} ${l.blend.mode} ${l.blend.amount}`).join(', ')}) wait for the picture wiring (not in this version)`);
  for (const m of extra.loop) if (!v2Modules.has(m.id)) fails.dropped.push(`picture loop module ${m.id}: not in v2`);
  for (const m of extra.modulators) if (!v2Modules.has(m.id)) fails.dropped.push(`modulator ${m.id}: not in v2`);
  if (!same(v2.macros, p.macros)) fails.dropped.push('the macros differ');
  if (!same(v2.pins, p.pins)) fails.dropped.push('the pins differ');
  // A macro's target: a knob of a module there (a device's exposed input).
  for (const mac of v2.macros)
    for (const t of mac.targets) {
      const m = v2Modules.get(t.module);
      const ok = m && (isDevice(m) ? m.inputs.some((e) => e.as === t.knob) : t.knob in m.knobs || (typeOf(m.type) && portsOf(typeOf(m.type)).in.some((x) => x.id === t.knob)));
      if (!ok) fails.dropped.push(`macro ${mac.id}: its target ${t.module}.${t.knob} is no knob in v2`);
    }

  // 2, 3. Every cable's ends are there and fit.
  const intoPicture = new Map();
  for (const c of f.cables) {
    const a = portAt(flat, c.from, 'from');
    const b = portAt(flat, c.to, 'to');
    if (a.error || b.error) {
      fails.dangling.push(`cable ${c.id}: ${[a.error, b.error].filter(Boolean).join('; ')}`);
      continue;
    }
    if (c.depth) {
      const d = portAt(flat, c.depth, 'from');
      if (d.error) fails.dangling.push(`cable ${c.id}'s depth: ${d.error}`);
    }
    if (!fits(a.port.type, b.port.type, !!(a.port.polymorphic || b.port.polymorphic)))
      fails.fit.push(`cable ${c.id}: ${c.from.module}.${c.from.port} (${a.port.type}) into ${c.to.module}.${c.to.port} (${b.port.type})`);
    if (b.port.type === 'picture' || b.port.type === 'light') {
      const key = `${c.to.module}.${c.to.port}`;
      intoPicture.set(key, [...(intoPicture.get(key) ?? []), c.id]);
    }
  }
  for (const [key, ids] of intoPicture) if (ids.length > 1) notes.push(`${key} takes ${ids.length} pictures (${ids.join(', ')})`);

  // 4. Every picture cycle through a Feedback.
  if (!cyclesClosed(f, typeOf)) fails.cycles.push('a picture cycle passes no Feedback');

  return { v2, f, fails, notes, reading: readingLosses(input.raw, v1), counts: { modules: f.modules.length, cables: f.cables.length, pictureCables: f.cables.filter((c) => c.id.startsWith('picture:')).length } };
}

// ---- The picture side (decision 0057 step 3): v1's renderer against the v2 picture graph -----------------------

/** The picture lab's `current` pipeline draws the v2 patch of today's chain with the recording's extras and effects
 *  (picture-lab.ts); its values come from the recording. A replay stands for `v2` where the picture side of `v2` is
 *  that patch's: its picture modules and cables, their knobs. */
function labStandsFor(v1, v2) {
  const pictureSide = (patch) => {
    const fp = flatten(patch);
    const ids = new Set(fp.cables.filter((c) => c.id.startsWith('picture:')).flatMap((c) => [c.from.module, c.to.module]));
    return { cables: fp.cables.filter((c) => c.id.startsWith('picture:')).map((c) => c.id).sort(), knobs: fp.modules.filter((m) => ['mix', 'feedback', 'screen'].includes(m.type) && ids.has(m.id)).map((m) => [m.id, m.knobs]) };
  };
  const lab = toV2({ ...copy(DEFAULTS), fx1: v1.fx1, fx2: v1.fx2, fx3: v1.fx3, extra: copy(v1.extra ?? emptyExtra()) });
  const a = pictureSide(v2);
  const b = pictureSide(lab);
  const differs = [];
  if (!same(a.cables, b.cables)) differs.push(`picture cables: ${a.cables.filter((x) => !b.cables.includes(x)).join(', ') || '-'} not in the lab's; ${b.cables.filter((x) => !a.cables.includes(x)).join(', ') || '-'} only in the lab's`);
  if (!same(a.knobs, b.knobs)) differs.push(`picture modules' knobs: ${JSON.stringify(a.knobs)} vs the lab's ${JSON.stringify(b.knobs)}`);
  return differs;
}

let browser = null;
async function openLab() {
  if (browser) return browser;
  const b = await openBrowser(base);
  // Recordings are served from here, not from the repo's tests/pictures (saved data stays out of the repo).
  const recordings = new Map();
  await b.serve('*/tests/pictures/migration-*', (url) => recordings.get(/migration-[^/?]+(?=\.json\.gz)/.exec(url)?.[0]));
  browser = { ...b, served: recordings, lab: (query, ready) => b.open(`picture-lab.html?${query}`, 'pictureLab', ready) };
  return browser;
}

/** The picture side of one conversion: `v1` recorded in today's renderer (the lab's `plain` scene with `v1`'s settings
 *  in its place) and replayed through the frozen v1 renderer (`legacy`) and the v2 picture graph (`current`), at each
 *  precision; their screens compared frame by frame. Needs a version with the picture graph (632/picture-graph's C5
 *  and after) and its dev server at --base. */
async function pictureCheck(v1, v2, name) {
  const differs = labStandsFor(v1, v2);
  if (differs.length) return { ok: false, error: `the lab's v2 patch is not this one's: ${differs.join('; ')}` };
  const b = await openLab();
  const tag = `migration-${name.replace(/[^a-z0-9]+/gi, '-').toLowerCase()}`.slice(0, 60);
  // The settings as a recall leaves them, the fewest points (SwiftShader draws on the CPU), thirty frames a second.
  const kept = { ...copy(v1), quality: 0, fps: 30, keptForm: 3, pins: [...new Set([...v1.pins, ...PRESET_KEYS, ...BEHAVIOUR_PINS, ...BAND_PINS])].filter(isPinnable) };
  const { identifier } = (await b.send('Page.addScriptToEvaluateOnNewDocument', {
    source: `(() => { const set = Storage.prototype.setItem; const kept = ${JSON.stringify(JSON.stringify(kept))};
      Storage.prototype.setItem = function (k, v) { return set.call(this, k, k === 'cratarr-visuals' && /record=/.test(location.search) && !window.__migrationSet ? (window.__migrationSet = true, kept) : v); }; })();`,
  })).result;
  try {
    await b.lab(`record=plain&frames=${frames}`, 'l.request?.done');
    const text = await b.js('JSON.stringify(window.pictureLab.request.recording)');
    b.served.set(tag, gzipSync(text));
    // The recording drew these settings, not the scene's: the page keeps what it shows.
    const shown = JSON.parse((await b.js("localStorage.getItem('cratarr-visuals')")) ?? '{}');
    const wrong = ['form', 'symmetry', 'final', 'intensity', 'speed', 'tunnel', 'fx1'].filter((k) => shown[k] !== v1[k]);
    if (wrong.length) return { ok: false, error: `the recording did not draw these settings (${wrong.map((k) => `${k} ${shown[k]}, not ${v1[k]}`).join('; ')})` };
  } finally {
    await b.send('Page.removeScriptToEvaluateOnNewDocument', { identifier });
  }
  const results = [];
  for (const precision of bits) {
    await b.lab(`replay=${tag}&a=legacy&b=current&bits=${precision}`, 'l.result');
    results.push(await b.js('window.pictureLab.result'));
  }
  return { ok: results.every((r) => r.max === 0), results };
}

// ---- Run ------------------------------------------------------------------------------------------------------

const CHECKS = { dropped: 'nothing dropped', dangling: 'no dangling cable ends', fit: 'types fit', cycles: 'cycles closed' };
const report = [];
let bad = 0;
console.log(`v2 conversion check: ${inputs.length} input(s); picture wiring ${wiring ? 'installed' : 'not in this version'}`);
for (const input of inputs) {
  const r = check(input);
  const line = Object.entries(CHECKS).map(([k, label]) => `${r.fails[k].length ? '✗' : '✓'} ${label}`).join('  ');
  console.log(`\n${input.name}: ${r.counts.modules} modules, ${r.counts.cables} cables (${r.counts.pictureCables} picture)\n  ${line}`);
  for (const k of Object.keys(CHECKS)) for (const x of r.fails[k]) console.log(`    ✗ ${CHECKS[k]}: ${x}`);
  for (const x of r.notes) console.log(`    · ${x}`);
  for (const x of r.reading) console.log(`    · read before the conversion: ${x}`);
  if (Object.values(r.fails).some((x) => x.length)) bad++;
  const entry = { name: input.name, kind: input.kind, counts: r.counts, fails: r.fails, notes: r.notes, reading: r.reading };
  if (withPicture && (!only || only.split(',').some((o) => input.name.toLowerCase().includes(o.toLowerCase())))) {
    if (!wiring) entry.picture = { ok: false, error: 'this version has no picture graph' };
    else
      try {
        entry.picture = await pictureCheck(input.v1, r.v2, input.name);
      } catch (e) {
        entry.picture = { ok: false, error: String(e?.message ?? e) };
      }
    const pc = entry.picture;
    if (pc.error) console.log(`  ✗ picture: ${pc.error}`);
    for (const x of pc.results ?? []) {
      const psnr = Number.isFinite(x.psnr) ? `${x.psnr.toFixed(1)} dB` : '∞';
      console.log(`  ${x.max === 0 ? '✓' : '✗'} picture at ${x.precision.join('/')}: legacy vs current, ${x.frames} frames: max ${x.max}/255${x.max ? ` (frame ${x.worstFrame}, ${x.framesDiffering} differ)` : ''}, mean ${x.mean.toFixed(4)}, PSNR ${psnr}; lit ${x.lit.toFixed(1)}; tiles ${x.tiles.matched} matched, max ${x.tiles.max.toFixed(4)}; ${x.seconds.toFixed(0)} s`);
    }
    if (!pc.ok) bad++;
  }
  report.push(entry);
}
browser?.close();
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(report, null, 2));
console.log(`\n${bad ? `${bad} failing` : 'all passing'}`);
process.exit(bad ? 1 : 0);
