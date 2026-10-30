<script lang="ts">
  // The visuals' panel over the patch (decision 0057, the switch, S3): every thing a row, each opening its own page
  // (the settings' North Star). The top: the preset, then the five categories (Sources, Effects, Mixers, Modulators,
  // In / Out), each pinnable whole, each with its modules and a "+ Add"; all the cables; this device. A module's page:
  // its knobs (a slider, its pin, what moves it), its other inputs, Randomize, Remove; a device's: its macros and the
  // modules in it. A port's page: the cables into it and "+ Add". A cable's page: its ends (each tapped to move it),
  // its channel, what scales it, its curve, its pin, Delete. Every change goes to the store as one edit (patch-edits.ts).
  // Step 9 (the owner, the standard module page): where it is (crumbs, each a tap, the graph at its level); its picture
  // while the graph is hidden; its knobs; its inputs (← what comes in); its outputs (→ where it goes: a tap follows the
  // cable to its module, "+ Add" cables it somewhere new); Randomize, Remove.
  import { panSafe } from '../panSafe';
  import Icon from '../Icon.svelte';
  import Seg from '../Seg.svelte';
  import SwitchTrack from '../SwitchTrack.svelte';
  import ChoiceSheet from '../settings/ChoiceSheet.svelte';
  import { longpress } from '../longpress';
  import { held } from './held.svelte';
  import CurveEditor from './CurveEditor.svelte';
  import KnobCables from './KnobCables.svelte';
  import Face from './face/Face.svelte';
  import { faceOf, previewOf, Thumbs, type Thumbnailer } from './face/face';
  import Preview from './face/Preview.svelte';
  import { untrack } from 'svelte';
  import type { LiveFrame } from './faces';
  import { curveOf, valueOf as kept, type Curve } from './curves';
  import { faceAt, freeMacro, panelKey, type RackEdit } from './patch-edits';
  import { categoryPin, knobText, knobViews, labelOf, moduleName, patchAt, polyTurned, typesOf, type KnobView } from './patchgraph';
  import { fits, isDevice, type CableV2, type Device, type PatchV2, type Port, type PortRef } from './ports';
  import { byCategory, CATEGORIES, CATEGORY_LABEL, deviceCategory, portsFrom, type Category } from './rack';
  import { DEVICES } from './devices';
  import { RANGES, saveVisualSettings, visualSettings as s } from './settings.svelte';
  import { flatten, macroNode, mappingOf } from './v2';
  import type { Module } from './patch';
  import type { V2Type } from './v2-types';
  import { LIBRARY_GROUPS, type LibraryGroup } from './library';

  let {
    patch,
    types,
    onedit,
    presets,
    preset = null,
    onpreset,
    onrandomize,
    live = null,
    path = $bindable([]),
    thumbnail,
    previewShown = false,
    onlevel,
  }: {
    patch: PatchV2;
    types: Record<string, V2Type>;
    /** An edit, and the store's key for it (a knob dragged: one undo step). */
    onedit: (e: RackEdit, key: string | null) => void;
    /** The snapshot library (library.ts), each under its group's heading. */
    presets: readonly { id: string; label: string; line: string; group?: LibraryGroup }[];
    /** The preset the patch was last set to, if any. */
    preset?: string | null;
    onpreset: (id: string) => void;
    /** Randomize a module (its path) or a category's whole. */
    onrandomize: (what: { module: string[] } | { category: Category }) => void;
    /** The engine's values now (four times a second), for a module's face. */
    live?: LiveFrame | null;
    /** The pages open, the latest last (a page's name: see `go`'s callers). */
    path?: string[];
    /** Draws a module's picture into a canvas (the renderer's thumbnail), for its page's preview. */
    thumbnail?: Thumbnailer;
    /** The graph hidden: a module's page shows its picture itself. */
    previewShown?: boolean;
    /** The level the page is at (a module's device, or a device's inside), for the graph to follow. */
    onlevel?: (scope: string[]) => void;
  } = $props();

  const BAND_ROWS = ['Sub', 'Bass', 'Low mid', 'Mid', 'Upper mid', 'Air'];
  const here = $derived(path.at(-1) ?? '');
  const go = (page: string) => (path = [...path, page]);
  const back = (steps = 1) => (path = path.slice(0, Math.max(0, path.length - steps)));
  const edit = (e: RackEdit) => onedit(e, panelKey(e));

  // Pages are named `kind|…` (ids hold dots and slashes): `m|a/b` a module at its path, `in|a/b|port` the cables into a
  // port, `c|scope|id` a cable, `from|scope|module|port` a source for a new cable, `move|end|scope|id` a cable's end
  // moved, `depth|scope|id` what scales it, `add|scope|category` a module added, `k|a/b|knob` a band knob's six, and
  // `presets`, `cables`, `device`.
  const parts = (page: string) => page.split('|');
  const pathOf = (s: string) => (s ? s.split('/') : []);

  const portsOf = $derived(portsFrom(types));
  const levelAt = (scope: string[]) => patchAt(patch, scope);
  function moduleAt(at: string[]): { scope: string[]; level: PatchV2; m: Module } | null {
    const scope = at.slice(0, -1);
    const level = levelAt(scope);
    const m = level?.modules.find((x) => x.id === at[at.length - 1]);
    return level && m ? { scope, level, m } : null;
  }
  /** A module's name, as the graph gives it (patchgraph.ts moduleName). */
  const nameOf = (m: Module, level: PatchV2) => moduleName(m, level, types);
  const portName = (m: Module, port: string, dir: 'in' | 'out') => portsOf(m)?.[dir].find((p) => p.id === port)?.label ?? port;
  function endName(level: PatchV2, r: PortRef, dir: 'in' | 'out'): string {
    const m = level.modules.find((x) => x.id === r.module);
    if (!m) return r.module;
    const band = r.channel !== undefined ? ` · ${BAND_ROWS[r.channel] ?? r.channel + 1}` : '';
    return `${nameOf(m, level)} · ${portName(m, r.port, dir)}${band}`;
  }
  const cablesInto = (level: PatchV2, module: string, port: string) => level.cables.filter((c) => c.to.module === module && c.to.port === port);
  const cablesFrom = (level: PatchV2, module: string, port: string) => level.cables.filter((c) => c.from.module === module && c.from.port === port);
  /** A port's cables in a row's line: the first two ends' names, and how many more. */
  function endsLine(level: PatchV2, ends: PortRef[], dir: 'in' | 'out', arrow: string): string {
    const names = ends.slice(0, 2).map((r) => endName(level, r, dir));
    return `${arrow} ${names.join(', ')}${ends.length > 2 ? ` +${ends.length - 2}` : ''}`;
  }

  // ---- Where a page is: Rack › each device › its module › its port (each crumb a tap there) ------------------------
  /** The module path a page is about, and its own last crumb (a port's, a cable's) if any. */
  function placeOf(page: string): { at: string[]; last: string | null } | null {
    const [kind, a, b] = parts(page);
    if (kind === 'm') return { at: pathOf(a), last: null };
    if (kind === 'in' || kind === 'out') {
      const at = pathOf(a);
      const f = moduleAt(at);
      return { at, last: f ? portName(f.m, b, kind) : b };
    }
    if (kind === 'from' || kind === 'to') return { at: [...pathOf(a), b], last: kind === 'from' ? 'From' : 'To' };
    if (kind === 'k') return { at: pathOf(a), last: knobAt(pathOf(a), b)?.label ?? b };
    if (kind === 'x' || kind === 'xt' || kind === 'xmap') return { at: pathOf(a), last: faceAt(patch, pathOf(a))?.macros.find((y) => y.id === b)?.label ?? b };
    if (kind === 'xpick') return { at: pathOf(a), last: 'Macro' };
    return null;
  }
  const crumbs = $derived.by((): { label: string; page: string | null }[] => {
    const place = placeOf(here);
    if (!place) return [{ label: 'Rack', page: null }, ...path.map((p) => ({ label: pageLabel(p), page: p }))];
    const out: { label: string; page: string | null }[] = [{ label: 'Rack', page: null }];
    for (let i = 1; i <= place.at.length; i++) {
      const f = moduleAt(place.at.slice(0, i));
      if (f) out.push({ label: nameOf(f.m, f.level), page: `m|${place.at.slice(0, i).join('/')}` });
    }
    if (place.last) out.push({ label: place.last, page: here });
    return out;
  });
  /** A crumb tapped: that place's page, the way back from it as its crumbs go. */
  function goCrumb(page: string | null) {
    if (page === here) return;
    path = page ? [page] : [];
  }
  // The graph follows the page: at a module's level (a device's page: inside it).
  $effect(() => {
    const place = placeOf(here);
    if (!place || !onlevel) return;
    const f = moduleAt(place.at);
    const scope = f && isDevice(f.m) && parts(here)[0] === 'm' ? place.at : place.at.slice(0, -1);
    untrack(() => onlevel(scope));
  });

  // ---- A module's picture on its page while the graph is hidden (one live frame, one thumbnail) --------------------
  const thumbs = new Thumbs();
  $effect(() => {
    if (live && previewShown) untrack(() => thumbs.next(thumbnail));
  });
  const pinned = (level: PatchV2, pin: string) => level.pins.includes(pin);

  const percent = (v: number) => `${Math.round(v * 100)}%`;
  /** A module's knobs as the graph draws them (patchgraph.ts knobViews): a band's six as one, at their mean; a step
   *  with its choices' names. */
  const knobsOf = (level: PatchV2, m: Module, t: V2Type | undefined, at?: string[]): KnobView[] =>
    t
      ? knobViews(m, t, {
          open: new Set(),
          room: Infinity,
          cabled: (k) => cablesInto(level, m.id, k).length > 0,
          cablesInto: (k) => cablesInto(level, m.id, k),
          mapped: at ? (k) => mappingOf(flat, at.join('/'), k, types) : undefined,
        }).knobs
      : [];
  /** The patch as the engine runs it: what a macro sets (0077), by a module's path from the top. */
  const flat = $derived(flatten(patch));
  /** A module's knob by its path and id, as drawn. */
  function knobAt(at: string[], id: string): KnobView | undefined {
    const f = moduleAt(at);
    return f ? knobsOf(f.level, f.m, types[f.m.type], at).find((k) => k.id === id) : undefined;
  }
  /** A macro's target named: its module's knob, or a device's macro. */
  function targetName(level: PatchV2, t: { module: string; knob: string }): string {
    const m = level.modules.find((y) => y.id === t.module);
    if (!m) return t.module;
    if (isDevice(m) && t.knob.startsWith('macro:')) return `${nameOf(m, level)} · ${m.macros.find((y) => y.id === t.knob.slice('macro:'.length))?.label ?? t.knob}`;
    return `${nameOf(m, level)} · ${types[m.type]?.knobs.find((k) => k.id === t.knob)?.label ?? t.knob}`;
  }
  /** What a target's range is in: its knob (a device's macro: a share). */
  function targetKnob(level: PatchV2, t: { module: string; knob: string }): Pick<KnobView, 'kind' | 'min' | 'max' | 'options'> {
    const m = level.modules.find((y) => y.id === t.module);
    const k = m && !isDevice(m) ? types[m.type]?.knobs.find((y) => y.id === t.knob) : undefined;
    return k ? { kind: k.kind, min: k.min, max: k.max, ...(k.options ? { options: k.options } : {}) } : { kind: 'level', min: 0, max: 1 };
  }
  /** The macro at a level that maps a knob (0077: one a knob). */
  const ownerAt = (macros: readonly { id: string; label: string; targets: readonly { module: string; knob: string }[] }[], module: string, knob: string) =>
    macros.find((y) => y.targets.some((t) => t.module === module && t.knob === knob));
  /** A new macro at a level, its page opened. */
  function newMacro(scope: string[]) {
    const face = faceAt(patch, scope);
    if (!face) return;
    const n = freeMacro(face.macros);
    edit({ kind: 'newMacro', scope, id: n.id, label: n.label });
    go(`x|${scope.join('/')}|${n.id}`);
  }
  /** The level a module or a device's macro is in, its macros there: its device's page, or the rack's (the top). */
  const goLevel = (at: string[]) => (at.length > 1 ? (path = [`m|${at.slice(0, -1).join('/')}`]) : (path = []));
  // The inputs that are no knob's: a picture, light, points, a flow, sound, a trigger, a value of its own.
  function ownInputs(level: PatchV2, m: Module, t: V2Type | undefined): Port[] {
    const knobs = knobsOf(level, m, t);
    return (portsOf(m)?.in ?? []).filter((p) => !knobs.some((k) => k.id === p.id) && !p.id.endsWith(':gain') && !p.id.endsWith(':next'));
  }
  /** A knob turned: one edit, or a band's six each moved as far (the graph's polyTurned), one undo step either way. */
  function turnKnob(at: string[], k: KnobView, value: number) {
    if (!k.channels) return edit({ kind: 'knob', module: at, knob: k.id, value });
    const key = `knob:${at.join('/')}.${k.id}`;
    for (const c of polyTurned(k, value)) onedit({ kind: 'knob', module: at, knob: c.id, value: c.value }, key);
  }
  /** A step that is a yes or a no (its choices Off and On): a switch. */
  const isSwitch = (k: KnobView) => k.options?.length === 2 && k.options[0] === 'Off';
  /** The step whose choices are open in a sheet. */
  let choosing = $state<{ at: string[]; k: KnobView } | null>(null);

  // ---- A new cable's source, a moved end, what scales it: the ports at its level that fit -------------------------
  function candidates(level: PatchV2, want: Port, dir: 'out' | 'in'): { ref: PortRef; label: string }[] {
    const out: { ref: PortRef; label: string }[] = [];
    for (const m of level.modules) {
      const ports = portsOf(m)?.[dir] ?? [];
      for (const p of ports) {
        const ok = dir === 'out' ? fits(p.type, want.type, want.polymorphic || p.polymorphic) : fits(want.type, p.type, want.polymorphic || p.polymorphic);
        if (ok) out.push({ ref: { module: m.id, port: p.id }, label: `${nameOf(m, level)} · ${p.label}` });
      }
    }
    return out;
  }
  const VALUE: Port = { id: 'depth', label: 'Depth', type: 'value' };
  function inPort(level: PatchV2, r: PortRef): Port | undefined {
    const m = level.modules.find((x) => x.id === r.module);
    return m ? portsOf(m)?.in.find((p) => p.id === r.port) : undefined;
  }
  function outPort(level: PatchV2, r: PortRef): Port | undefined {
    const m = level.modules.find((x) => x.id === r.module);
    return m ? portsOf(m)?.out.find((p) => p.id === r.port) : undefined;
  }
  /** The id `applyV2` gives a module added of `type` at a level. */
  function nextId(level: PatchV2, type: string): string {
    const taken = new Set(level.modules.map((m) => m.id));
    let n = 1;
    while (taken.has(`${type}-${n}`)) n++;
    return `${type}-${n}`;
  }

  // ---- This device's own settings (not the patch's) -----------------------------------------------------------------
  const DEVICE_ROWS = [
    { key: 'quality', label: 'Quality', step: 0.05, text: percent },
    { key: 'fps', label: 'FPS', step: 5, text: (v: number) => `${v}` },
    { key: 'brightness', label: 'Brightness', step: 0.05, text: percent },
  ] as const;
  function setDevice(key: (typeof DEVICE_ROWS)[number]['key'], v: number) {
    s[key] = v;
    saveVisualSettings();
  }

  /** A page's name in the trail. */
  function pageLabel(page: string): string {
    const [kind, a, b, c] = parts(page);
    if (kind === 'm') {
      const at = moduleAt(pathOf(a));
      return at ? nameOf(at.m, at.level) : a;
    }
    if (kind === 'in') {
      const at = moduleAt(pathOf(a));
      return at ? portName(at.m, b, 'in') : b;
    }
    if (kind === 'k') return knobAt(pathOf(a), b)?.label ?? b;
    if (kind === 'x') return faceAt(patch, pathOf(a))?.macros.find((y) => y.id === b)?.label ?? b;
    if (kind === 'xt') return 'Range';
    if (kind === 'xmap') return 'Map a knob';
    if (kind === 'xpick') return 'Macro';
    if (kind === 'c') return 'Cable';
    if (kind === 'from') return 'From';
    if (kind === 'out') {
      const at = moduleAt(pathOf(a));
      return at ? portName(at.m, b, 'out') : b;
    }
    if (kind === 'to') return 'To';
    if (kind === 'move') return a === 'from' ? 'From' : 'To';
    if (kind === 'depth') return 'Scaled by';
    if (kind === 'add') return `Add to ${CATEGORY_LABEL[c as Category] ?? ''}`;
    return { presets: 'Presets', cables: 'Cables', device: 'This device' }[kind] ?? kind;
  }

  const groups = $derived(byCategory(patch, types));

  // ---- Several picked at once (the owner: modules, devices and cables, not knobs): pinned or unpinned together, the
  // cables deleted together. A long press on a row (or Select) starts it; Done ends it, as does leaving the view.
  let selecting = $state<'modules' | 'cables' | null>(null);
  let picked = $state<string[]>([]);
  const togglePick = (key: string) => (picked = picked.includes(key) ? picked.filter((k) => k !== key) : [...picked, key]);
  function startSelect(kind: 'modules' | 'cables', first?: string) {
    selecting = kind;
    picked = first ? [first] : [];
  }
  const endSelect = () => ((selecting = null), (picked = []));
  $effect(() => {
    void here;
    endSelect();
  });
  /** Each picked one's level and pin (`module:<id>`, `cable:<id>`); a key is `<scope>|<id>`. */
  const pickedPins = () =>
    picked.map((k) => {
      const [sc, id] = k.split('|');
      return { scope: pathOf(sc), id, pin: `${selecting === 'cables' ? 'cable' : 'module'}:${id}` };
    });
  function pinPicked(on: boolean) {
    for (const x of pickedPins()) edit({ kind: 'pinOne', scope: x.scope, pin: x.pin, on });
    endSelect();
  }
  function deletePicked() {
    if (selecting === 'modules') for (const x of pickedPins()) edit({ kind: 'removeModule', scope: x.scope, id: x.id });
    else {
      const by = new Map<string, string[]>();
      for (const x of pickedPins()) by.set(x.scope.join('/'), [...(by.get(x.scope.join('/')) ?? []), x.id]);
      for (const [sc, ids] of by) edit({ kind: 'remove', scope: pathOf(sc), cables: ids });
    }
    endSelect();
  }
  /** The picked modules made one device (at the top: the panel picks there), its macros chosen later in it. */
  function makePicked() {
    edit({ kind: 'make', scope: [], modules: pickedPins().map((x) => x.id), macros: [] });
    endSelect();
  }
  /** One picked: its own view (a module) or its end moved (a cable). */
  function openPicked() {
    const [x] = pickedPins();
    const page = selecting === 'modules' ? `m|${[...x.scope, x.id].join('/')}` : `move|to|${x.scope.join('/')}|${x.id}`;
    endSelect();
    go(page);
  }
  const allCables = $derived(patch.cables.length);
</script>

{#snippet goRow(label: string, line: string, page: string)}
  <button class="go" onclick={() => go(page)}><span class="name">{label}{#if line}<small>{line}</small>{/if}</span><Icon name="chevronRight" size={18} /></button>
{/snippet}

{#snippet pinButton(level: PatchV2, scope: string[], pin: string, label: string)}
  <button class="pin" class:on={pinned(level, pin)} aria-pressed={pinned(level, pin)} aria-label={pinned(level, pin) ? `Unpin ${label}` : `Pin ${label}`} use:held={'pin'} onclick={() => edit({ kind: 'pinOne', scope, pin, on: !pinned(level, pin) })}
    ><Icon name="pin" size={16} /></button
  >
{/snippet}

{#snippet portRows(at: string[], level: PatchV2, m: Module, t: V2Type | undefined)}
{@const inputs = ownInputs(level, m, t)}
{#if inputs.length}
  <div class="rows">
    <h3>Inputs</h3>
    {#each inputs as p (p.id)}
      {@const into = cablesInto(level, m.id, p.id)}
      {@render goRow(p.label, into.length ? endsLine(level, into.map((c) => c.from), 'out', '←') : p.normal ? 'As normalled' : 'Nothing', `in|${at.join('/')}|${p.id}`)}
    {/each}
  </div>
{/if}
{@const outputs = portsOf(m)?.out ?? []}
{#if outputs.length}
  <div class="rows">
    <h3>Outputs</h3>
    {#each outputs as p (p.id)}
      {@const from = cablesFrom(level, m.id, p.id)}
      {@render goRow(p.label, from.length ? endsLine(level, from.map((c) => c.to), 'in', '→') : 'Nothing', `out|${at.join('/')}|${p.id}`)}
    {/each}
  </div>
{/if}
{/snippet}

{#snippet knobRow(at: string[], level: PatchV2, m: Module, k: KnobView)}
  {@const n = cablesInto(level, m.id, k.id).length}
  {@const pin = `${m.id}.${k.id}`}
  <div class="knob">
    {#if k.macro}
      <!-- Its macro's (0077: one a knob): as the macro sets it, named; a tap goes to the macro's level. -->
      <div class="row pinnable">
        <span>{k.label}</span>
        <button class="choice open" onclick={() => goLevel(at)}><span>{k.macro.label} · {knobText(k, k.value)}</span><Icon name="chevronRight" size={16} /></button>
        {@render pinButton(level, at.slice(0, -1), pin, k.label)}
      </div>
    {:else if k.kind === 'step' && k.options && isSwitch(k)}
      <!-- A yes or a no: a switch (the choices' rule, the owner 2026-10-25). -->
      <div class="row pinnable">
        <span>{k.label}</span>
        <button class="toggle" role="switch" aria-checked={k.value > k.min} aria-label={k.label} onclick={() => edit({ kind: 'knob', module: at, knob: k.id, value: k.value > k.min ? k.min : k.max })}><SwitchTrack on={k.value > k.min} /></button>
        {@render pinButton(level, at.slice(0, -1), pin, k.label)}
      </div>
    {:else if k.kind === 'step' && k.options && k.options.length <= 3}
      <!-- Two or three: a slim track beside its name. -->
      <div class="row pinnable">
        <span>{k.label}</span>
        <span class="choice"><Seg label={k.label} options={k.options.map((o, i) => ({ value: String(k.min + i), label: o }))} bind:value={() => String(Math.round(k.value)), (x) => edit({ kind: 'knob', module: at, knob: k.id, value: Number(x) })} /></span>
        {@render pinButton(level, at.slice(0, -1), pin, k.label)}
      </div>
    {:else if k.kind === 'step' && k.options}
      <!-- More: the row says the choice; a tap opens them in a sheet. -->
      <div class="row pinnable">
        <span>{k.label}</span>
        <button class="choice open" onclick={() => (choosing = { at, k })}><span>{knobText(k, k.value)}</span><Icon name="chevronRight" size={16} /></button>
        {@render pinButton(level, at.slice(0, -1), pin, k.label)}
      </div>
    {:else}
      <!-- A value; a band's six at their mean, each moved as far (its page has a slider a band). -->
      <label class="row pinnable">
        <span>{k.label}</span>
        <input type="range" use:panSafe min={k.min} max={k.max} step={k.kind === 'step' ? 1 : (k.max - k.min) / 100} value={k.value} oninput={(e) => turnKnob(at, k, Number(e.currentTarget.value))} />
        <output>{knobText(k, k.value)}</output>
        {@render pinButton(level, at.slice(0, -1), pin, k.label)}
      </label>
    {/if}
    <button
      class="moves"
      class:on={n > 0}
      aria-label="{k.channels ? `${k.label}: its bands and what moves it` : `What moves ${k.label}`}{n ? ` (${n})` : ''}"
      use:held={'moves'}
      onclick={() => go(`${k.channels ? 'k' : 'in'}|${at.join('/')}|${k.id}`)}>{#if n}<span>{n}</span>{/if}<Icon name="chevronRight" size={16} /></button
    >
  </div>
{/snippet}

<div class="vpanel">
  {#if path.length}
    <div class="crumbs">
      <button class="back" aria-label="Back" onclick={() => back()}><Icon name="back" size={20} /></button>
      <span class="trail"
        >{#each crumbs as c, i (i)}{#if i}<span class="sep">›</span>{/if}<button class="crumb" class:here={i === crumbs.length - 1} onclick={() => goCrumb(c.page)}>{c.label}</button>{/each}</span
      >
    </div>
  {/if}

  {#if !path.length}
    {#if patch.macros.length}
      <!-- The rack's face (0077): its macros, every one. -->
      <div class="rows">
        <h3>Macros</h3>
        {#each patch.macros as x (x.id)}
          {@const v = x.value}
          <div class="knob">
            <label class="row">
              <span>{x.label}</span>
              <input type="range" use:panSafe min="0" max="1" step="0.01" value={v} oninput={(e) => edit({ kind: 'macro', scope: [], id: x.id, value: Number(e.currentTarget.value) })} />
              <output>{percent(v)}</output>
            </label>
            <button class="moves" class:on={x.targets.length > 0} aria-label="{x.label}: what it maps" onclick={() => go(`x||${x.id}`)}
              >{#if x.targets.length}<span>{x.targets.length}</span>{/if}<Icon name="chevronRight" size={16} /></button
            >
          </div>
        {/each}
        {#if !selecting}<button class="go add" onclick={() => newMacro([])}><span class="name">+ New macro</span></button>{/if}
      </div>
    {:else}
      <div class="rows"><button class="go add" onclick={() => newMacro([])}><span class="name">+ New macro</span></button></div>
    {/if}
    <div class="rows">
      <h3>Preset</h3>
      {@render goRow(presets.find((p) => p.id === preset)?.label ?? 'Your own', presets.find((p) => p.id === preset)?.line ?? 'Changed since a preset was chosen', 'presets')}
    </div>
    {#each CATEGORIES as cat (cat)}
      <div class="rows">
        <div class="head">
          <h3>{CATEGORY_LABEL[cat]}</h3>
          <button class="pin" class:on={pinned(patch, categoryPin(cat))} aria-pressed={pinned(patch, categoryPin(cat))} aria-label="Pin all of {CATEGORY_LABEL[cat]}" use:held={'pin'} onclick={() => edit({ kind: 'pin', scope: [], category: cat, on: !pinned(patch, categoryPin(cat)) })}
            ><Icon name="pin" size={16} /></button
          >
        </div>
        {#each groups[cat] as m (m.id)}
          {#if selecting === 'modules'}
            <button class="go pick" class:on={picked.includes(`|${m.id}`)} aria-pressed={picked.includes(`|${m.id}`)} onclick={() => togglePick(`|${m.id}`)}
              ><span class="name">{nameOf(m, patch)}</span>{#if pinned(patch, `module:${m.id}`)}<Icon name="pin" size={14} />{/if}</button
            >
          {:else}
            <button class="go" use:longpress={() => startSelect('modules', `|${m.id}`)} onclick={() => go(`m|${m.id}`)}
              ><span class="name">{nameOf(m, patch)}</span>{#if pinned(patch, `module:${m.id}`)}<Icon name="pin" size={14} />{/if}<Icon name="chevronRight" size={18} /></button
            >
          {/if}
        {/each}
        {#if !selecting}<button class="go add" onclick={() => go(`add||${cat}`)}><span class="name">+ Add</span></button>{/if}
      </div>
    {/each}
    {#if !selecting}
      <div class="rows"><button class="go" onclick={() => startSelect('modules')}><span class="name">Select modules and devices</span></button></div>
    {/if}
    <div class="rows">
      {@render goRow('Cables', `${allCables} in all`, 'cables')}
      {@render goRow('This device', 'Quality, frame rate, brightness', 'device')}
    </div>
  {:else if here === 'presets'}
    {#each LIBRARY_GROUPS.map((g) => ({ ...g, list: presets.filter((p) => (p.group ?? 'saved') === g.id) })).filter((g) => g.list.length) as g (g.id)}
      <h3>{g.label}</h3>
      <div class="rows">
        {#each g.list as p (p.id)}<button class="go" class:on={preset === p.id} onclick={() => (onpreset(p.id), (path = []))}><span class="name">{p.label}<small>{p.line}</small></span></button>{/each}
      </div>
    {/each}
  {:else if here === 'device'}
    <div class="rows">
      {#each DEVICE_ROWS as r (r.key)}
        <label class="row">
          <span>{r.label}</span>
          <input type="range" use:panSafe min={RANGES[r.key][0]} max={RANGES[r.key][1]} step={r.step} value={s[r.key]} oninput={(e) => setDevice(r.key, Number(e.currentTarget.value))} />
          <output>{r.text(s[r.key])}</output>
        </label>
      {/each}
    </div>
  {:else if here === 'cables'}
    <div class="rows">
      {#each patch.modules as m (m.id)}
        {#each portsOf(m)?.in ?? [] as p (p.id)}
          {@const n = cablesInto(patch, m.id, p.id).length}
          {#if n}{@render goRow(`${nameOf(m, patch)} · ${p.label}`, `${n} ${n === 1 ? 'cable' : 'cables'}`, `in|${m.id}|${p.id}`)}{/if}
        {/each}
      {/each}
    </div>
  {:else if parts(here)[0] === 'm'}
    {@const at = pathOf(parts(here)[1])}
    {@const found = moduleAt(at)}
    {#if found}
      {@const { scope, level, m } = found}
      {@const t = types[m.type]}
      {#if isDevice(m)}
        {@const d = m as Device}
        <div class="rows">
          <h3>Macros</h3>
          {#each d.macros as x (x.id)}
            {@const g = mappingOf(flat, macroNode(at.join('/'), x.id), 'value')}
            <div class="knob">
              {#if g}
                <!-- Mapped by its level's macro (0077: one level down): as that sets it; a tap goes there. -->
                <div class="row">
                  <span>{x.label}</span>
                  <button class="choice open" onclick={() => goLevel(at)}><span>{g.label} · {percent(g.value)}</span><Icon name="chevronRight" size={16} /></button>
                </div>
              {:else}
                <label class="row">
                  <span>{x.label}</span>
                  <input type="range" use:panSafe min="0" max="1" step="0.01" value={x.value} oninput={(e) => edit({ kind: 'knob', module: at, knob: `macro:${x.id}`, value: Number(e.currentTarget.value) })} />
                  <output>{percent(x.value)}</output>
                </label>
              {/if}
              <button class="moves" class:on={x.targets.length > 0} aria-label="{x.label}: what it maps" onclick={() => go(`x|${at.join('/')}|${x.id}`)}
                >{#if x.targets.length}<span>{x.targets.length}</span>{/if}<Icon name="chevronRight" size={16} /></button
              >
            </div>
          {/each}
          <button class="go add" onclick={() => newMacro(at)}><span class="name">+ New macro</span></button>
        </div>
        <div class="rows">
          <div class="head">
            <h3>In it</h3>
            {@render pinButton(level, scope, `module:${m.id}`, nameOf(m, level))}
          </div>
          {#each d.patch.modules as inner (inner.id)}{@render goRow(nameOf(inner, d.patch), '', `m|${[...at, inner.id].join('/')}`)}{/each}
        </div>
        {@render portRows(at, level, m, t)}
        <div class="rows">
          <button class="go" onclick={() => onrandomize({ module: at })}><span class="name">Randomize</span></button>
          <button class="go" onclick={() => (edit({ kind: 'unmake', scope, device: m.id }), back())}><span class="name">Unmake device</span></button>
          <button class="go danger" onclick={() => (edit({ kind: 'removeModule', scope, id: m.id }), back())}><span class="name">Remove</span></button>
        </div>
      {:else}
        <!-- Its picture while the graph is hidden (the graph shows it otherwise); the live parts of its face. -->
        {@const kind = previewOf(portsOf(m))}
        {#if previewShown && kind}
          {#key at.join('/')}<div class="rows preview"><Preview module={at.join('/')} label={nameOf(m, level)} {kind} {live} thumbs={thumbnail ? thumbs : null} /></div>{/key}
        {/if}
        {@const face = faceOf(t, portsOf(m)).filter((p) => p.kind !== 'thumb')}
        {#if face.length}
          <div class="rows face"><Face parts={face} {live} path={at.join('/')} knobs={m.knobs} width={300} /></div>
        {/if}
        {@const knobs = knobsOf(level, m, t, at)}
        {#if knobs.length}
          <div class="rows">
            <div class="head">
              <h3>Knobs</h3>
              {@render pinButton(level, scope, `module:${m.id}`, nameOf(m, level))}
            </div>
            {#each knobs as k (k.id)}{@render knobRow(at, level, m, k)}{/each}
          </div>
        {/if}
        {@render portRows(at, level, m, t)}
        <div class="rows">
          <button class="go" onclick={() => onrandomize({ module: at })}><span class="name">Randomize</span></button>
          {#if m.type !== 'screen'}<button class="go danger" onclick={() => (edit({ kind: 'removeModule', scope, id: m.id }), back())}><span class="name">Remove</span></button>{/if}
        </div>
      {/if}
    {/if}
  {:else if parts(here)[0] === 'k'}
    <!-- A band's six (a polyphonic knob, 0057 §4b): a slider a band; what moves it under them. -->
    {@const [, a, id] = parts(here)}
    {@const at = pathOf(a)}
    {@const found = moduleAt(at)}
    {@const k = knobAt(at, id)}
    {#if found && k?.channels}
      {@const n = cablesInto(found.level, found.m.id, k.id).length}
      <div class="rows">
        <div class="head">
          <h3>Bands</h3>
          {@render pinButton(found.level, found.scope, `${found.m.id}.${k.id}`, k.label)}
        </div>
        {#each k.channels as c, ch (c.id)}
          <label class="row">
            <span>{BAND_ROWS[ch] ?? `${ch + 1}`}</span>
            <input type="range" use:panSafe min={k.min} max={k.max} step={k.kind === 'step' ? 1 : (k.max - k.min) / 100} value={c.value} oninput={(e) => edit({ kind: 'knob', module: at, knob: c.id, value: Number(e.currentTarget.value) })} />
            <output>{knobText(k, c.value)}</output>
          </label>
        {/each}
      </div>
      <div class="rows">
        {@render goRow('What moves it', n ? `${n} ${n === 1 ? 'cable' : 'cables'}` : 'Nothing', `in|${a}|${k.id}`)}
      </div>
    {/if}
  {:else if parts(here)[0] === 'in'}
    {@const [, a, port] = parts(here)}
    {@const at = pathOf(a)}
    {@const found = moduleAt(at)}
    {#if found}
      {@const { scope, level, m } = found}
      {#if !isDevice(m) && types[m.type]?.knobs.some((k) => k.id === port)}
        <!-- What moves it: its macro (0077: one a knob), then its cables. -->
        {@const owner = ownerAt(faceAt(patch, scope)?.macros ?? [], m.id, port)}
        <div class="rows">
          {#if owner}{@render goRow('Macro', owner.label, `x|${scope.join('/')}|${owner.id}`)}{:else}{@render goRow('Macro', 'None: map it to one', `xpick|${at.join('/')}|${port}`)}{/if}
        </div>
      {/if}
      {#if selecting === 'cables'}
        <div class="rows">
          {#each cablesInto(level, m.id, port) as c (c.id)}
            {@const key = `${scope.join('/')}|${c.id}`}
            <button class="go pick" class:on={picked.includes(key)} aria-pressed={picked.includes(key)} onclick={() => togglePick(key)}
              ><span class="name">{endName(level, c.from, 'out')}</span>{#if pinned(level, `cable:${c.id}`)}<Icon name="pin" size={14} />{/if}</button
            >
          {/each}
        </div>
      {:else}
        <KnobCables
          cables={cablesInto(level, m.id, port)}
          label={(c: CableV2) => endName(level, c.from, 'out')}
          onopen={(id) => go(`c|${scope.join('/')}|${id}`)}
          onlongpress={(id) => startSelect('cables', `${scope.join('/')}|${id}`)}
          onadd={() => go(`from|${scope.join('/')}|${m.id}|${port}`)}
        />
      {/if}
    {/if}
  {:else if parts(here)[0] === 'x'}
    <!-- A macro (0077): its value, kept through the dice by its pin, what it maps (each to its range), Remove. -->
    {@const [, sc, id] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const face = faceAt(patch, scope)}
    {@const x = face?.macros.find((y) => y.id === id)}
    {#if face && x}
      {@const g = scope.length ? mappingOf(flat, macroNode(scope.join('/'), x.id), 'value') : undefined}
      <div class="rows">
        <div class="head">
          <h3>{x.label}</h3>
          {@render pinButton(face.level, scope, `macro:${x.id}`, x.label)}
        </div>
        {#if g}
          <div class="row">
            <span>Value</span>
            <button class="choice open" onclick={() => goLevel(scope)}><span>{g.label} · {percent(g.value)}</span><Icon name="chevronRight" size={16} /></button>
          </div>
        {:else}
          <label class="row">
            <span>Value</span>
            <input type="range" use:panSafe min="0" max="1" step="0.01" value={x.value} oninput={(e) => edit({ kind: 'macro', scope, id: x.id, value: Number(e.currentTarget.value) })} />
            <output>{percent(x.value)}</output>
          </label>
        {/if}
      </div>
      <div class="rows">
        <h3>Maps</h3>
        {#each x.targets as t (`${t.module}.${t.knob}`)}
          {@const k = targetKnob(face.level, t)}
          {@render goRow(targetName(face.level, t), `${knobText(k, t.from)} – ${knobText(k, t.to)}`, `xt|${sc}|${id}|${t.module}|${t.knob}`)}
        {/each}
        <button class="go add" onclick={() => go(`xmap|${sc}|${id}`)}><span class="name">+ Map a knob</span></button>
      </div>
      <div class="rows">
        <button class="go danger" onclick={() => (edit({ kind: 'removeMacro', scope, id: x.id }), back())}><span class="name">Remove macro</span></button>
      </div>
    {/if}
  {:else if parts(here)[0] === 'xt'}
    <!-- A knob a macro maps: the range it moves it over, from the macro's least to its most; Let go. -->
    {@const [, sc, id, module, knob] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const face = faceAt(patch, scope)}
    {@const t = face?.macros.find((y) => y.id === id)?.targets.find((u) => u.module === module && u.knob === knob)}
    {#if face && t}
      {@const k = targetKnob(face.level, t)}
      {@const lo = Math.min(k.min, t.from, t.to)}
      {@const hi = Math.max(k.max, t.from, t.to)}
      {@const step = k.kind === 'step' ? 1 : (k.max - k.min) / 100}
      <div class="rows">
        <h3>{targetName(face.level, t)}</h3>
        <label class="row">
          <span>From</span>
          <input type="range" use:panSafe min={lo} max={hi} {step} value={t.from} oninput={(e) => edit({ kind: 'macroRange', scope, id, module, knob, from: Number(e.currentTarget.value) })} />
          <output>{knobText(k, t.from)}</output>
        </label>
        <label class="row">
          <span>To</span>
          <input type="range" use:panSafe min={lo} max={hi} {step} value={t.to} oninput={(e) => edit({ kind: 'macroRange', scope, id, module, knob, to: Number(e.currentTarget.value) })} />
          <output>{knobText(k, t.to)}</output>
        </label>
      </div>
      <div class="rows">
        <button class="go danger" onclick={() => (edit({ kind: 'unmapKnob', scope, id, module, knob }), back())}><span class="name">Let go</span></button>
      </div>
    {/if}
  {:else if parts(here)[0] === 'xmap'}
    <!-- A knob of the macro's level mapped to it (one a knob: one another macro has, it takes over), or a device's macro. -->
    {@const [, sc, id] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const face = faceAt(patch, scope)}
    {#if face}
      <div class="rows">
        {#each face.level.modules as m (m.id)}
          {#if isDevice(m)}
            {#each m.macros as y (y.id)}
              {@const owner = ownerAt(face.macros, m.id, `macro:${y.id}`)}
              {#if owner?.id !== id}
                <button class="go" onclick={() => (edit({ kind: 'mapKnob', scope, id, module: m.id, knob: `macro:${y.id}` }), back())}
                  ><span class="name">{nameOf(m, face.level)} · {y.label}{#if owner}<small>{owner.label}'s now</small>{/if}</span></button
                >
              {/if}
            {/each}
          {:else}
            {@const t = types[m.type]}
            {#each t ? knobsOf(face.level, m, t).filter((k) => !k.channels) : [] as k (k.id)}
              {@const owner = ownerAt(face.macros, m.id, k.id)}
              {#if owner?.id !== id}
                <button class="go" onclick={() => (edit({ kind: 'mapKnob', scope, id, module: m.id, knob: k.id }), back())}
                  ><span class="name">{nameOf(m, face.level)} · {k.label}{#if owner}<small>{owner.label}'s now</small>{/if}</span></button
                >
              {/if}
            {/each}
          {/if}
        {/each}
      </div>
    {/if}
  {:else if parts(here)[0] === 'xpick'}
    <!-- A knob mapped to one of its level's macros, or to a new one. -->
    {@const [, a, knob] = parts(here)}
    {@const at = pathOf(a)}
    {@const scope = at.slice(0, -1)}
    {@const module = at[at.length - 1]}
    {@const face = faceAt(patch, scope)}
    {#if face}
      <div class="rows">
        {#each face.macros as y (y.id)}
          <button class="go" onclick={() => (edit({ kind: 'mapKnob', scope, id: y.id, module, knob }), back())}><span class="name">{y.label}</span></button>
        {/each}
        <button
          class="go add"
          onclick={() => {
            const n = freeMacro(face.macros);
            edit({ kind: 'newMacro', scope, id: n.id, label: n.label });
            edit({ kind: 'mapKnob', scope, id: n.id, module, knob });
            path = [...path.slice(0, -1), `x|${scope.join('/')}|${n.id}`];
          }}><span class="name">+ New macro</span></button
        >
      </div>
    {/if}
  {:else if parts(here)[0] === 'out'}
    <!-- An output's cables: a tap follows one to its module; its amount opens the cable; "+ Add" cables it anew. -->
    {@const [, a, port] = parts(here)}
    {@const at = pathOf(a)}
    {@const found = moduleAt(at)}
    {#if found}
      {@const { scope, level, m } = found}
      <div class="rows">
        {#each cablesFrom(level, m.id, port) as c (c.id)}
          <div class="follow">
            <button class="go" onclick={() => go(`m|${[...scope, c.to.module].join('/')}`)}><span class="name">→ {endName(level, c.to, 'in')}</span><Icon name="chevronRight" size={18} /></button>
            <button class="cable" aria-label="Its cable" onclick={() => go(`c|${scope.join('/')}|${c.id}`)}><Icon name="sliders" size={16} /></button>
          </div>
        {/each}
        <button class="go add" onclick={() => go(`to|${scope.join('/')}|${m.id}|${port}`)}><span class="name">+ Add</span></button>
      </div>
    {/if}
  {:else if parts(here)[0] === 'to'}
    {@const [, sc, module, port] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const level = levelAt(scope)}
    {@const have = level ? outPort(level, { module, port }) : undefined}
    {#if level && have}
      <div class="rows">
        {#each candidates(level, have, 'in').filter((x) => x.ref.module !== module) as x (`${x.ref.module}.${x.ref.port}`)}
          <button class="go" onclick={() => (edit({ kind: 'connect', scope, from: { module, port }, to: x.ref }), back())}><span class="name">{x.label}</span></button>
        {/each}
      </div>
    {/if}
  {:else if parts(here)[0] === 'from'}
    {@const [, sc, module, port] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const level = levelAt(scope)}
    {@const want = level ? inPort(level, { module, port }) : undefined}
    {#if level && want}
      <div class="rows">
        {#each candidates(level, want, 'out').filter((x) => x.ref.module !== module) as x (`${x.ref.module}.${x.ref.port}`)}
          <button class="go" onclick={() => (edit({ kind: 'connect', scope, from: x.ref, to: { module, port } }), back())}><span class="name">{x.label}</span></button>
        {/each}
      </div>
    {/if}
  {:else if parts(here)[0] === 'move' || parts(here)[0] === 'depth'}
    {@const [kind, endOrScope, ...rest] = parts(here)}
    {@const end = kind === 'move' ? (endOrScope as 'from' | 'to') : null}
    {@const scope = pathOf(kind === 'move' ? rest[0] : endOrScope)}
    {@const id = kind === 'move' ? rest[1] : rest[0]}
    {@const level = levelAt(scope)}
    {@const c = level?.cables.find((x) => x.id === id)}
    {#if level && c}
      {@const want = end === 'from' ? inPort(level, c.to) : end === 'to' ? outPort(level, c.from) : VALUE}
      <div class="rows">
        {#if !end}
          <button class="go" onclick={() => (edit({ kind: 'cable', scope, id, depth: null }), back())}><span class="name">Nothing</span></button>
        {/if}
        {#if want}
          {#each candidates(level, want, end === 'to' ? 'in' : 'out') as x (`${x.ref.module}.${x.ref.port}`)}
            <button class="go" onclick={() => (end ? edit({ kind: 'reroute', scope, id, end, to: x.ref }) : edit({ kind: 'cable', scope, id, depth: x.ref }), back())}><span class="name">{x.label}</span></button>
          {/each}
        {/if}
      </div>
    {/if}
  {:else if parts(here)[0] === 'c'}
    {@const [, sc, id] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const level = levelAt(scope)}
    {@const c = level?.cables.find((x) => x.id === id)}
    {#if level && c}
      {@const from = outPort(level, c.from)}
      <div class="sheet">
        <div class="ends">
          <button class="end" onclick={() => go(`move|from|${sc}|${id}`)}>{endName(level, c.from, 'out')}</button>
          <span class="arrow" aria-hidden="true">→</span>
          <button class="end" onclick={() => go(`move|to|${sc}|${id}`)}>{endName(level, c.to, 'in')}</button>
        </div>
        {#if (from?.channels ?? 1) > 1}
          <div class="seg" role="radiogroup" aria-label="Band">
            <button role="radio" aria-checked={c.from.channel === undefined} class:on={c.from.channel === undefined} onclick={() => edit({ kind: 'cable', scope, id, channel: null })}>All</button>
            {#each BAND_ROWS as band, b (band)}
              <button role="radio" aria-checked={c.from.channel === b} class:on={c.from.channel === b} onclick={() => edit({ kind: 'cable', scope, id, channel: b })}>{band}</button>
            {/each}
          </div>
        {/if}
        {@render goRow('Scaled by', c.depth ? endName(level, c.depth, 'out') : 'Nothing', `depth|${sc}|${id}`)}
        <CurveEditor
          curve={curveOf(c.value)}
          mixed={false}
          live={null}
          density={null}
          walks={false}
          pinned={pinned(level, `cable:${id}`)}
          onchange={(k: Curve) => edit({ kind: 'cable', scope, id, value: kept({ h1: [...k.h1], h2: [...k.h2], h: k.h }) })}
          onpin={(on) => edit({ kind: 'pinOne', scope, pin: `cable:${id}`, on })}
        />
        <button class="remove" onclick={() => (edit({ kind: 'remove', scope, cables: [id] }), back())}>Delete</button>
      </div>
    {/if}
  {:else if parts(here)[0] === 'add'}
    {@const [, sc, cat] = parts(here)}
    {@const scope = pathOf(sc)}
    {@const level = levelAt(scope)}
    {#if level}
      <div class="rows">
        {#each DEVICES.filter((d) => { const ports = portsOf(d); return ports && deviceCategory(ports) === cat; }) as d (d.id)}
          <button
            class="go"
            onclick={() => {
              const taken = new Set(level.modules.map((m) => m.id));
              let id = d.id;
              for (let n = 2; taken.has(id); n++) id = `${d.id}-${n}`;
              edit({ kind: 'addDevice', scope, device: d });
              path = [...path.slice(0, -1), `m|${[...scope, id].join('/')}`];
            }}><span class="name">{d.label}<small>Device</small></span></button
          >
        {/each}
        {#each typesOf(cat as Category, types) as t (t.type)}
          <button
            class="go"
            onclick={() => {
              const id = nextId(level, t.type);
              edit({ kind: 'add', scope, type: t.type });
              path = [...path.slice(0, -1), `m|${[...scope, id].join('/')}`];
            }}><span class="name">{labelOf(t)}</span></button
          >
        {/each}
      </div>
    {/if}
  {/if}
  {#if selecting}
    <div class="selectbar" role="toolbar" aria-label="Picked">
      <span>{picked.length} picked</span>
      <button disabled={!picked.length} onclick={() => pinPicked(true)}>Pin</button>
      <button disabled={!picked.length} onclick={() => pinPicked(false)}>Unpin</button>
      <button class="danger" disabled={!picked.length} onclick={deletePicked}>Delete</button>
      {#if selecting === 'modules'}<button disabled={picked.length < 2} onclick={makePicked}>Make device</button>{/if}
      {#if picked.length === 1}<button onclick={openPicked}>{selecting === 'modules' ? 'Open' : 'Reroute'}</button>{/if}
      <button onclick={endSelect}>Done</button>
    </div>
  {/if}
  {#if choosing}
    {@const { at, k } = choosing}
    <ChoiceSheet
      title={k.label}
      options={(k.options ?? []).map((o, i) => ({ value: String(k.min + i), label: o }))}
      value={String(Math.round(k.value))}
      onchange={(x) => edit({ kind: 'knob', module: at, knob: k.id, value: Number(x) })}
      onclose={() => (choosing = null)}
    />
  {/if}
</div>

<style>
  .go.pick {
    padding-left: 10px;
  }

  .go.pick.on {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .selectbar {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding: 8px 4px;
    background: rgb(20 20 24 / 0.96);
    border-top: 1px solid rgb(255 255 255 / 0.12);
  }

  .selectbar span {
    flex: 1;
    font-size: 0.85rem;
    color: rgb(255 255 255 / 0.7);
  }

  .selectbar button {
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 16px;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .selectbar button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .vpanel {
    display: grid;
    gap: 0;
  }

  .rows + .rows {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgb(255 255 255 / 0.12);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .crumb {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .follow {
    display: flex;
    align-items: center;
  }
  .follow .go {
    flex: 1;
    min-width: 0;
  }
  .follow .cable {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 20px;
    background: none;
    color: rgb(255 255 255 / 0.7);
    cursor: pointer;
  }
  .preview {
    display: grid;
    justify-items: center;
  }

  .crumbs {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    margin-bottom: 4px;
    font-size: 0.85rem;
    color: rgb(255 255 255 / 0.6);
  }

  .crumbs .back {
    display: grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
    cursor: pointer;
  }

  .trail {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trail .sep {
    margin: 0 5px;
  }

  .trail .here {
    color: #fff;
    font-weight: 600;
  }

  .go {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    padding: 6px 4px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: #fff;
    font: inherit;
    font-size: 0.92rem;
    text-align: left;
    cursor: pointer;
  }

  .go:hover {
    background: rgb(255 255 255 / 0.06);
  }

  .go.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .go .name {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .go small {
    color: rgb(255 255 255 / 0.55);
    font-size: 0.8rem;
  }

  .go.add .name {
    color: var(--accent-ink);
  }

  .danger {
    color: var(--danger-ink);
  }

  h3 {
    margin: 0;
    min-height: 32px;
    display: flex;
    align-items: center;
    font-size: 0.85rem;
    font-weight: 600;
    color: rgb(255 255 255 / 0.6);
  }

  .knob {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .knob > .row {
    flex: 1;
    min-width: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 6.2em 1fr 3.4em;
    align-items: center;
    gap: 12px;
    min-height: 38px;
    font-size: 0.92rem;
  }

  .row.pinnable {
    grid-template-columns: 5.4em 1fr 4em 1.4em;
    gap: 8px;
  }

  .row input {
    width: 100%;
    min-width: 0;
    margin: 0;
    accent-color: var(--accent);
  }

  .row .toggle,
  .row .choice {
    grid-column: 2 / 4;
    justify-self: end;
    min-width: 0;
  }

  .toggle {
    display: flex;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .choice.open {
    display: flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    padding: 4px 0 4px 8px;
    border: 0;
    background: none;
    color: rgb(255 255 255 / 0.75);
    font: inherit;
    cursor: pointer;
  }

  .choice.open span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row output {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.75);
  }

  .moves {
    display: flex;
    align-items: center;
    height: 30px;
    padding: 0 2px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: rgb(255 255 255 / 0.35);
    font: inherit;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .moves.on {
    color: var(--accent-ink);
  }

  .pin {
    display: grid;
    place-items: center;
    width: 1.4em;
    height: 28px;
    padding: 0;
    border: 0;
    background: none;
    color: rgb(255 255 255 / 0.25);
    cursor: pointer;
  }

  .pin.on {
    color: var(--accent-ink);
  }

  .sheet {
    display: grid;
    gap: 10px;
  }

  .ends {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .end {
    min-width: 0;
    padding: 6px 8px;
    overflow: hidden;
    border: 0;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.06);
    color: inherit;
    font: inherit;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .arrow {
    color: rgb(255 255 255 / 0.5);
  }

  .seg {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 2px;
    border-radius: 10px;
    background: rgb(255 255 255 / 0.06);
  }

  .seg button {
    flex: 1;
    min-width: 3.5em;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: rgb(255 255 255 / 0.75);
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .seg button.on {
    background: rgb(255 255 255 / 0.14);
    color: #fff;
  }

  .remove {
    justify-self: start;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--danger-ink);
    font: inherit;
    cursor: pointer;
  }
</style>
