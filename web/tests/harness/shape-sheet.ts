// A sheet of every shape in the one set (shape-set.ts), inside white, as raw bytes for a look by eye:
// node --experimental-strip-types --import ./tests/ts-resolve.mjs tests/harness/shape-sheet.ts <out.raw>
// Writes 6 columns of 128-pixel cells; the size is printed.
import { writeFileSync } from 'node:fs';
import { IRREGULAR, irregularEdge, irregularFill, regularEdge, regularFill } from '../../src/lib/visuals/shape-set.ts';

const cell = 128;
const shapes: { name: string; edge: (x: number, y: number) => number; fill: (t: number, u: number) => [number, number] }[] = [
  { name: 'polygon 5', edge: (x, y) => regularEdge(Math.hypot(x, y), Math.atan2(y, x), 5, 0), fill: (t, u) => regularFill(t, u, 5, 0) },
  { name: 'star 5', edge: (x, y) => regularEdge(Math.hypot(x, y), Math.atan2(y, x), 5, 0.5), fill: (t, u) => regularFill(t, u, 5, 0.5) },
  { name: 'star 6.5', edge: (x, y) => regularEdge(Math.hypot(x, y), Math.atan2(y, x), 6.5, 0.8), fill: (t, u) => regularFill(t, u, 6.5, 0.8) },
  ...IRREGULAR.map((name) => ({ name, edge: (x: number, y: number) => irregularEdge(name, x, y), fill: (t: number, u: number) => irregularFill(name, t, u) })),
];
const cols = 6;
const rows = Math.ceil(shapes.length / cols) * 2;
const w = cols * cell;
const h = rows * cell;
const px = new Uint8Array(w * h);
const t0 = Date.now();
shapes.forEach((s, k) => {
  const ox = (k % cols) * cell;
  const oy = Math.floor(k / cols) * 2 * cell;
  for (let j = 0; j < cell; j++)
    for (let i = 0; i < cell; i++) {
      const x = -1 + ((i + 0.5) * 2) / cell;
      const y = 1 - ((j + 0.5) * 2) / cell;
      const e = s.edge(x, y);
      px[(oy + j) * w + ox + i] = e < 0 ? 200 : Math.abs(e) < 0.02 ? 90 : 20;
    }
  // Below it, its fill's points.
  for (let a = 0; a < 200; a++)
    for (let b = 0; b < 100; b++) {
      const [x, y] = s.fill((a + 0.5) / 200, (b + 0.5) / 100);
      const i = Math.floor(((x + 1) / 2) * cell);
      const j = Math.floor(((1 - y) / 2) * cell);
      if (i >= 0 && j >= 0 && i < cell && j < cell) px[(oy + cell + j) * w + ox + i] = 255;
    }
});
writeFileSync(process.argv[2], px);
console.log(JSON.stringify({ w, h, ms: Date.now() - t0, names: shapes.map((s) => s.name) }));
