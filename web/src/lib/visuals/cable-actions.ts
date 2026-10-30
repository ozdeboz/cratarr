// What can be done to cables (decision 0056; the owner after 70: each line "clickable, removable", a long press "to
// delete or reroute"): pure, over the patch's two kinds of cable alike, today's routes (their id the route key, `signal|band|
// aspect`) and the extras' (their own id). The graph and the rack both call these; Visuals.svelte keeps the result.

import { type RouteValue } from './curves';
import { ASPECT_KNOB, type Cable, type Module, type ModuleType, type PatchExtra, type SignalRef } from './patch';
import { LEVEL_ASPECTS, SIGNAL_LABEL, routeKey, type Aspect, type Signal } from './routes';

/** The six bands' names (the server's bands, low to high). */
export const BAND_NAMES = ['Sub', 'Bass', 'Low mid', 'Mid', 'Upper mid', 'Air'] as const;

/** The cables as the settings hold them: the routes and the extras. */
export type Cables = { routes: Record<string, RouteValue>; extra: PatchExtra };

/** Whether an id is a route's (`signal|band|aspect`), not an extra cable's. */
export const isRouteId = (id: string) => id.split('|').length === 3;

function mapIds(c: Cables, ids: ReadonlySet<string>, route: (v: RouteValue) => RouteValue, extra: (x: Cable) => Cable | null): Cables {
  const routes = { ...c.routes };
  for (const id of ids) if (isRouteId(id) && id in routes) routes[id] = route(routes[id]);
  const cables = c.extra.cables.flatMap((x) => (ids.has(x.id) ? (extra(x) ?? []) : [x]));
  return { routes, extra: { ...c.extra, cables } };
}

/** Taken out. A route is set to 0, not dropped: the settings keep routes over the defaults, so a dropped default one
 *  would come back. */
export const removeCables = (c: Cables, ids: Iterable<string>): Cables => mapIds(c, new Set(ids), () => 0, () => null);

/** What a cable comes from, picked (SourcePicker): a signal, a running modulator, a new one of a type. */
export type Source = { signal: string; band?: number } | { modulator: string } | { newModulator: string };

/** A new cable from `from` into `to` at half its range: a route where it can be one (a signal, with its reference and
 *  speed as routes have them, into one of the routes' columns), else an extra cable (`id`: a new one). `kind`: the
 *  knob's (a level's cable lets through; every other swings). */
export function newCable(from: SignalRef | { modulator: string }, to: { module: string; knob: string }, kind: 'level' | 'swing' | 'step', id: string): Cable {
  const aspect = (Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]).find(([, [m, k]]) => m === to.module && k === to.knob)?.[0];
  const plain = 'signal' in from && !from.reference && !from.speed;
  const mode: Cable['mode'] = kind === 'level' ? 'level' : 'swing';
  if (aspect && plain) return { id: routeKey(from.signal as Signal, from.band ?? null, aspect), from, to, value: 0.5, mode };
  return { id, from, to, value: 0.5, mode };
}

/** An id for a new extra cable, not yet taken. */
export function freeCableId(c: Cables): string {
  const taken = new Set(c.extra.cables.map((x) => x.id));
  let n = c.extra.cables.length + 1;
  while (taken.has(`cable-${n}`)) n++;
  return `cable-${n}`;
}

/** A source's name: a signal's (with its band's), a modulator's (its label, else its type's). */
export function sourceLabel(from: Cable['from'], modulators: readonly Module[], types: Record<string, ModuleType>): string {
  if ('modulator' in from) {
    const m = modulators.find((x) => x.id === from.modulator);
    return m ? (types[m.type]?.label ?? m.type) : from.modulator;
  }
  const name = SIGNAL_LABEL[from.signal as Signal] ?? from.signal;
  return from.band !== undefined ? `${name} · ${BAND_NAMES[from.band] ?? from.band + 1}` : name;
}

/** A knob's name: its module's and its own (`Flame · Hue`), a band's knob with its band. */
export function knobLabel(to: Cable['to'], types: Record<string, ModuleType>, modules: readonly Module[] = []): string {
  const type = types[to.module] ?? types[modules.find((m) => m.id === to.module)?.type ?? ''];
  const k = type?.knobs.find((x) => x.id === to.knob);
  return `${type?.label ?? to.module} · ${k?.label ?? to.knob}`;
}

/** A cable by its id, a route given as a cable too (from its key: its signal and band into its column's knob). */
export function cableOf(c: Cables, id: string): Cable | null {
  if (!isRouteId(id)) return c.extra.cables.find((x) => x.id === id) ?? null;
  const v = c.routes[id];
  if (v === undefined) return null;
  const [signal, band, aspect] = id.split('|');
  const to = ASPECT_KNOB[aspect as Aspect];
  if (!to) return null;
  const from: SignalRef = band === '-' ? { signal } : { signal, band: Number(band) };
  return { id, from, to: { module: to[0], knob: to[1] }, value: v, mode: LEVEL_ASPECTS.includes(aspect as Aspect) ? 'level' : 'swing' };
}

/** One end of a cable moved (the owner: a long press, "to delete or reroute"): `from` to another source, `to` to another
 *  knob, its value and mode kept. It may become a route, or stop being one (newCable's rule); the old one is taken out.
 *  The cables after it, and the cable's id now. */
export function reroute(c: Cables, id: string, end: 'from' | 'to', next: SignalRef | { modulator: string } | { module: string; knob: string }, kind: 'level' | 'swing' | 'step'): { cables: Cables; id: string } | null {
  const was = cableOf(c, id);
  if (!was) return null;
  const from = end === 'from' ? (next as Cable['from']) : was.from;
  const to = end === 'to' ? (next as Cable['to']) : was.to;
  const out = removeCables(c, [id]);
  const made = newCable(from, to, kind, isRouteId(id) ? freeCableId(out) : id);
  if (isRouteId(made.id)) out.routes = { ...out.routes, [made.id]: was.value };
  else out.extra = { ...out.extra, cables: [...out.extra.cables, { ...made, value: was.value, mode: kind === 'level' ? 'level' : was.mode }] };
  return { cables: out, id: made.id };
}
