// The shipped devices named by what they carry (the owner, 2026-10-29: the panel's rows said "Cv, In" twenty-two
// times, "VCA 1" to "VCA 22", and told nothing). A device's exposed port takes the name of what comes through it, and
// a module inside one the name of what it does there:
// - Loudness Follow: each VCA by the row of Music it follows (its input and its output so named); the loudness that
//   keys them all one input, Loudness, led to every one (a fan-out, as Bloom's picture);
// - Calm: each Ducker by the busyness it holds (its key, its output), its threshold's VCA beside it; the loudness one
//   input to them all;
// - Pause Dim: its input Playing, its output Dim, its parts by their part;
// - Show: its knobs' inputs by the knob (Waves, Smoothing …), not by the module behind them;
// and the rack's macros by their names (Accent, not Flash: 0057 §7e item 9).
// Pure, in place. A patch named again is as it was, so a kept one is named as it is loaded (visuals-patch.svelte.ts).

import type { ModuleType } from './patch';
import { isDevice, portsOfModule, SHOW_PARTS, type Device, type PatchV2, type PortRef } from './ports';

const BANDS = ['Sub', 'Bass', 'Low mid', 'Mid', 'Upper mid', 'Air'];
/** The rack's macros by name where the setting's says otherwise (0057 §7e item 9: Accent, not Flash, so "flash" is only
 *  the Screen's flash guard). */
export const RACK_NAMES: Readonly<Record<string, string>> = { flash: 'Accent' };

type Types = Record<string, ModuleType>;

/** What a cable's source gives: its port's name (its module's, where the port is only its Out), a band's after it. */
function sourceName(p: PatchV2, r: PortRef, types: Types): string | null {
  const m = p.modules.find((x) => x.id === r.module);
  const port = m ? portsOfModule(types)(m)?.out.find((x) => x.id === r.port) : undefined;
  if (!m || !port) return null;
  const what = port.label === 'Out' ? (m.label ?? types[m.type]?.label ?? m.type) : port.label;
  return r.channel !== undefined ? `${what} · ${BANDS[r.channel] ?? r.channel + 1}` : what;
}

/** The cables at the device's level into one of its exposed inputs. */
const into = (p: PatchV2, d: Device, as: string) => p.cables.filter((c) => c.to.module === d.id && c.to.port === as);

/** Every exposed port of an inner module's port, named. */
function name(d: Device, module: string, port: string, dir: 'in' | 'out', label: string) {
  for (const e of dir === 'in' ? d.inputs : d.outputs) if (e.module === module && e.port === port) e.label = label;
}

/** The inner modules' `port` inputs made one exposed input (`as`, `label`) where every one is fed by the same source
 *  by the same amount and depth: one cable in, led to them all. */
function oneInput(p: PatchV2, d: Device, port: string, as: string, label: string) {
  const list = d.inputs.filter((e) => e.port === port);
  if (list.length < 2 || list.every((e) => e.as === as) || d.inputs.some((e) => e.as === as && e.port !== port)) return;
  const feeds = list.map((e) => into(p, d, e.as));
  if (feeds.some((f) => f.length !== 1)) return;
  const [first] = feeds[0];
  const same = (r: PortRef) => r.module === first.from.module && r.port === first.from.port && r.channel === first.from.channel;
  const alike = (c: (typeof first)) => JSON.stringify(c.value) === JSON.stringify(first.value) && JSON.stringify(c.depth ?? null) === JSON.stringify(first.depth ?? null);
  if (!feeds.every(([c]) => same(c.from) && alike(c))) return;
  const gone = new Set(feeds.map(([c]) => c.id));
  for (const e of list) {
    e.as = as;
    e.label = label;
  }
  p.cables = [...p.cables.filter((c) => !gone.has(c.id)), { ...first, to: { module: d.id, port: as } }];
  p.pins = p.pins.filter((x) => !(x.startsWith('cable:') && gone.has(x.slice(6)) && x.slice(6) !== first.id));
}

function loudnessFollow(p: PatchV2, d: Device, types: Types) {
  for (const v of d.patch.modules) {
    if (v.type !== 'vca') continue;
    const e = d.inputs.find((x) => x.module === v.id && x.port === 'in');
    const [c] = e ? into(p, d, e.as) : [];
    const what = c ? sourceName(p, c.from, types) : null;
    if (!what) continue;
    v.label = what;
    name(d, v.id, 'in', 'in', what);
    name(d, v.id, 'out', 'out', what);
  }
  oneInput(p, d, 'cv', 'loudness', 'Loudness');
}

function calm(p: PatchV2, d: Device, types: Types) {
  for (const k of d.patch.modules) {
    if (k.type !== 'ducker') continue;
    const e = d.inputs.find((x) => x.module === k.id && x.port === 'key');
    const [c] = e ? into(p, d, e.as) : [];
    const what = c ? sourceName(p, c.from, types) : null;
    if (!what) continue;
    k.label = what;
    name(d, k.id, 'key', 'in', what);
    name(d, k.id, 'out', 'out', what);
    // Its threshold's VCA (toV2's `<ducker>-activity`): the threshold scaled by the loudness.
    const vca = d.patch.modules.find((x) => x.id === `${k.id}-activity`);
    if (vca) vca.label = `${what}: threshold`;
  }
  oneInput(p, d, 'cv', 'loudness', 'Loudness');
}

const PAUSE_PARTS: Record<string, string> = { 'pause-invert': 'Paused', 'pause-follow': 'Fade', 'pause-dim': 'Dim' };
function pauseDim(_p: PatchV2, d: Device) {
  name(d, 'pause-invert', 'cv', 'in', 'Playing');
  name(d, 'pause-dim', 'out', 'out', 'Dim');
  for (const m of d.patch.modules) if (PAUSE_PARTS[m.id]) m.label = PAUSE_PARTS[m.id];
}

function show(_p: PatchV2, d: Device, types: Types) {
  for (const part of SHOW_PARTS) {
    const k = types[part.type]?.knobs.find((x) => x.id === part.knob);
    if (!k) continue;
    for (const e of d.inputs) {
      if (e.module !== part.type) continue;
      if (e.port === part.knob) e.label = k.label;
      else if (e.port === `${part.knob}:gain`) e.label = `${k.label}: gain`;
    }
  }
}

/** The shipped devices, by the ids toV2 gives them. */
const NAMERS: Record<string, (p: PatchV2, d: Device, types: Types) => void> = { 'loudness-follow': loudnessFollow, calm, pause: pauseDim, show };

/** `p`'s shipped devices named by what they carry (in place), and `p`. */
export function namedDevices(p: PatchV2, types: Types): PatchV2 {
  for (const m of p.modules) if (isDevice(m)) NAMERS[m.id]?.(p, m, types);
  for (const x of p.macros) if (RACK_NAMES[x.id] && x.on === 'rack') x.label = RACK_NAMES[x.id];
  return p;
}
