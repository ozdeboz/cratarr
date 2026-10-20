// Fewer cables in a converted patch (decision 0057; the owner: "150 routes is way too much"). The pure part of the
// pruner (prune-lab.ts renders): which of a flattened v2 patch's cables are candidates, the per-band copies of one route
// as one polyphonic cable where the ports allow it, the cables the engine replay cannot measure (they act through the
// visuals' own state: the look's and the behaviour's walks, the moments), and the fluid's pushes as the cables give them.
import { heightOf, shapeOf } from './evaluate';
import { MOMENT_ASPECTS } from './routes';
import { portsOf, type CableV2, type PatchV2, type PortRef, type Ports } from './ports';
import { ASPECT_KNOB, type ModuleType } from './patch';

const BANDS = 6;
type TypeOf = (type: string) => (ModuleType & { ports?: Ports }) | undefined;

/** One cable to keep or drop: a route's or an extra's, or a route's six per-band copies as one (`merged`). */
export type Group = {
  id: string;
  cables: CableV2[];
  /** The six bands' copies of one route: one polyphonic cable in what the patch would hold. */
  merged: boolean;
  /** Why the replay cannot tell what it does (kept as it is), where it cannot. */
  protectedBy?: string;
};

/** A cable that is the patch's structure, not a choice: the picture's wiring, a device's insides, an ADSR's gate. */
export function structural(c: CableV2, typeOfModule: (id: string) => string | undefined): boolean {
  return c.id.startsWith('picture:') || c.id.includes('/') || (typeOfModule(c.to.module) === 'adsr' && c.to.port === 'gate');
}

/** The visuals' own steps a column's cables act through, outside the engine and the renderer: the replay holds them. */
const OUTSIDE: Record<string, string> = {
  [ASPECT_KNOB.look.join('|')]: "the look's walk (drift.ts) and a new track's look",
  [ASPECT_KNOB.behaviour.join('|')]: "the behaviour's walk",
};
for (const a of MOMENT_ASPECTS) OUTSIDE[ASPECT_KNOB[a].join('|')] = 'a moment (a new setting as its cables rise past a half)';
const outsideOf = (c: CableV2): string | undefined => OUTSIDE[`${c.to.module}|${c.to.port}`];

/** A cable's route id with its band made `*` (`hits|2|lights` → `hits|*|lights`), or its own id with `*`. */
const mergedId = (c: CableV2) => (/^[^|]+\|\d\|[^|]+$/.test(c.id) ? c.id.replace(/\|\d\|/, '|*|') : `${c.id}*`);

/** The candidates of `f` (flattened), the six bands' copies of a route (the same source port, reading, target, value,
 *  mode and depth, channels 0–5) as one group. */
export function groupsOf(f: PatchV2): Group[] {
  const typeOf = new Map(f.modules.map((m) => [m.id, m.type]));
  const out: Group[] = [];
  const bands = new Map<string, CableV2[]>();
  for (const c of f.cables) {
    if (structural(c, (id) => typeOf.get(id))) continue;
    if (c.from.channel === undefined || c.to.channel !== undefined) {
      out.push({ id: c.id, cables: [c], merged: false });
      continue;
    }
    const { channel: _, ...from } = c.from;
    const key = JSON.stringify([from, c.to, c.value, c.mode, c.depth ?? 'normal']);
    (bands.get(key) ?? bands.set(key, []).get(key)!).push(c);
  }
  for (const list of bands.values()) {
    const channels = new Set(list.map((c) => c.from.channel));
    if (list.length === BANDS && channels.size === BANDS && [...channels].every((b) => b! >= 0 && b! < BANDS)) out.push({ id: mergedId(list[0]), cables: list, merged: true });
    else for (const c of list) out.push({ id: c.id, cables: [c], merged: false });
  }
  for (const g of out) {
    const why = outsideOf(g.cables[0]);
    if (why) g.protectedBy = why;
  }
  return out;
}

/** What a merged group would need to be one cable in the patch: its source port polyphonic (`channels: 6`), and its
 *  target's taking six channels (a part's column: each channel its part) or a rule for six channels into one (the
 *  bands' mean, as the routes give a band's cable into another column). */
export function polyphonyMissing(g: Group, f: PatchV2, typeOf: TypeOf): string[] {
  const c = g.cables[0];
  const type = (r: PortRef) => typeOf(f.modules.find((m) => m.id === r.module)?.type ?? '');
  const missing: string[] = [];
  const from = type(c.from) && portsOf(type(c.from)!).out.find((p) => p.id === c.from.port);
  const to = type(c.to) && portsOf(type(c.to)!).in.find((p) => p.id === c.to.port);
  if (!from || (from.channels ?? 1) < BANDS) missing.push(`${c.from.module}.${c.from.port} gives ${from ? (from.channels ?? 1) : 'no'} channel(s)`);
  if (!to || (to.channels ?? 1) < BANDS) missing.push(`${c.to.module}.${c.to.port} takes ${to ? (to.channels ?? 1) : 'no'} channel(s)`);
  return missing;
}

/** The cables of `f` but those of `drop` (group ids), and the ADSR gates no cable reads any more. */
export function without(f: PatchV2, groups: Group[], drop: ReadonlySet<string>): PatchV2 {
  const gone = new Set(groups.filter((g) => drop.has(g.id)).flatMap((g) => g.cables.map((c) => c.id)));
  const cables = f.cables.filter((c) => !gone.has(c.id));
  const typeOf = new Map(f.modules.map((m) => [m.id, m.type]));
  const read = new Set(cables.filter((c) => !(typeOf.get(c.to.module) === 'adsr' && c.to.port === 'gate')).map((c) => c.from.module));
  return { ...f, cables: cables.filter((c) => !(typeOf.get(c.to.module) === 'adsr' && c.to.port === 'gate' && !read.has(c.to.module))) };
}

/** `v2` with the cables `without` leaves out of `f` (its flattened form) deleted, and those cables as they were, for
 *  an undo. */
export function pruned(v2: PatchV2, f: PatchV2, groups: Group[], drop: ReadonlySet<string>): { patch: PatchV2; deleted: CableV2[] } {
  const left = new Set(without(f, groups, drop).cables.map((c) => c.id));
  const gone = new Set(f.cables.filter((c) => !left.has(c.id)).map((c) => c.id));
  return { patch: { ...v2, cables: v2.cables.filter((c) => !gone.has(c.id)) }, deleted: v2.cables.filter((c) => gone.has(c.id)) };
}

/** Which signal a cable reads (the routes' terms): Music's port and band, an ADSR's gate's, the Loudness's, the Track's. */
function signalOf(c: CableV2, f: PatchV2): { signal: string; band: number | null } | null {
  const type = f.modules.find((m) => m.id === c.from.module)?.type;
  if (type === 'music') return { signal: c.from.port, band: c.from.channel ?? null };
  if (type === 'envelope' && c.from.port === 'level') return { signal: 'loudness', band: null };
  if (type === 'track' && c.from.port === 'pulse') return { signal: 'track', band: null };
  if (type === 'adsr') {
    const gate = f.cables.find((x) => x.to.module === c.from.module && x.to.port === 'gate');
    return gate ? { signal: gate.from.port, band: c.from.channel ?? null } : null;
  }
  return null;
}

/** The fluid's pushes as `f`'s cables into Stir, Flow and Swirl give them (stirring.ts `gain`; Visuals reads the
 *  routes' matrix there, beside the engine): each cable's curve at the push's x, its column's scale in its amount. */
export function stirGain(f: PatchV2): (signal: string, band: number | null, aspect: string, x: number) => number {
  const by = new Map<string, { band: number | null; cable: CableV2 }[]>();
  for (const c of f.cables) {
    if (c.to.module !== 'fluid' || !['stir', 'flow', 'swirl'].includes(c.to.port)) continue;
    const s = signalOf(c, f);
    if (!s) continue;
    const key = `${s.signal}|${c.to.port}`;
    (by.get(key) ?? by.set(key, []).get(key)!).push({ band: s.band, cable: c });
  }
  return (signal, band, aspect, x) => {
    let sum = 0;
    for (const { band: b, cable } of by.get(`${signal}|${aspect}`) ?? []) if (b === band) sum += heightOf(shapeOf(cable.value), x);
    return sum;
  };
}

/** Every number in an engine frame's outcome, in order (for telling two runs apart). */
export function numbersOf(v: unknown, out: number[] = []): number[] {
  if (typeof v === 'number') out.push(v);
  else if (typeof v === 'boolean') out.push(v ? 1 : 0);
  else if (ArrayBuffer.isView(v)) out.push(...Array.from(v as unknown as ArrayLike<number>));
  else if (Array.isArray(v)) for (const x of v) numbersOf(x, out);
  else if (v && typeof v === 'object') for (const k of Object.keys(v).sort()) numbersOf((v as Record<string, unknown>)[k], out);
  return out;
}
