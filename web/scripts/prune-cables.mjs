// Fewer cables in converted patches (decision 0057): each input's v2 patch through the prune lab (prune-lab.html:
// the per-band copies of a route merged, each cable judged by the picture without it, dropped weakest first while the
// whole stays within 2/255 and 45 dB of the whole patch's picture), driven in a headless browser over a running dev
// server.
//
//   node --experimental-strip-types --import ./tests/ts-resolve.mjs scripts/prune-cables.mjs <file>… [--chains]
//        [--defaults] [--only name,name] [--base http://localhost:5173] [--json out.json] [--sheet out.png --sheet-of name]
//
// Files as check-migration.mjs takes them (visuals-inputs.mjs). --sheet: the input named by --sheet-of, a few of its
// frames before and after, side by side, labelled with the counts only. Nothing is written but --json's and --sheet's.
import { writeFileSync } from 'node:fs';
import { openBrowser, readInputs } from './visuals-inputs.mjs';

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
const only = option('only', '');
const jsonOut = option('json', '');
const sheetOut = option('sheet', '');
const sheetOf = option('sheet-of', '');
const inputs = readInputs(args, { chains: flag('chains'), defaults: flag('defaults') }).filter((x) => !only || only.split(',').some((o) => x.name.toLowerCase().includes(o.toLowerCase())));

const b = await openBrowser(base);
const out = [];
let bad = 0;
try {
  for (const input of inputs) {
    const t0 = Date.now();
    const sheet = !!sheetOut && !!sheetOf && input.name.toLowerCase().includes(sheetOf.toLowerCase());
    const given = { settings: input.v1, ...(sheet ? { shots: [10, 30, 55, 80] } : {}) };
    const { identifier } = (await b.send('Page.addScriptToEvaluateOnNewDocument', { source: `window.pruneLab = { input: ${JSON.stringify(given)} };` })).result;
    let r;
    try {
      await b.open('prune-lab.html', 'pruneLab', 'l.result', { minutes: 60, progress: (p) => process.stdout.write(`\r  ${input.name}: ${p}`.slice(0, 160).padEnd(160)) });
      r = await b.js('window.pruneLab.result');
    } catch (e) {
      console.log(`\n✗ ${input.name}: ${e.message}`);
      bad++;
      continue;
    } finally {
      await b.send('Page.removeScriptToEvaluateOnNewDocument', { identifier });
    }
    // A PSNR of ∞ (no difference) comes over the protocol as null.
    const db = (d) => d.psnr ?? Infinity;
    const ok = r.diff.max <= 2 && db(r.diff) >= 45;
    if (!ok) bad++;
    const psnr = (d) => (Number.isFinite(db(d)) ? `${db(d).toFixed(1)} dB` : '∞');
    const mergedKept = r.keep.filter((k) => k.merged).length;
    console.log(
      `\r${ok ? '✓' : '✗'} ${input.name}: ${r.before.cables} cables (${r.before.groups} with each route's six bands as one) → ${r.after.cables} (${r.after.groups}; ${mergedKept} of them six bands), ${r.deleted.length} deleted; ` +
        `against the whole: max ${r.diff.max}/255, mean ${r.diff.mean.toFixed(4)}, PSNR ${psnr(r.diff)}, lit ${r.diff.lit.toFixed(1)}; ${r.renders} renders, ${((Date.now() - t0) / 1000).toFixed(0)} s`.padEnd(160),
    );
    for (const k of r.keep) console.log(`    kept ${k.id}${k.merged ? ' (six bands)' : ''}: ${k.protectedBy ? `unjudged, acts through ${k.protectedBy}` : k.why === 'elsewhere' ? 'acts outside the drawn seconds' : k.cost ? `alone ${k.cost.max}/255, mean ${k.cost.mean.toFixed(4)}, ${psnr(k.cost)}` : k.why}`);
    if (sheet && r.sheet) {
      writeFileSync(sheetOut, Buffer.from(r.sheet.split(',')[1], 'base64'));
      console.log(`    sheet: ${sheetOut}`);
    }
    const { sheet: _, ...rest } = r;
    out.push({ name: input.name, kind: input.kind, ...rest });
  }
} finally {
  b.close();
}
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(out, null, 2));
process.exit(bad ? 1 : 0);
