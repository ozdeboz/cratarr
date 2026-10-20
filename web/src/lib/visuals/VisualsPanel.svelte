<script lang="ts">
  // The visuals' panel over the patch (decision 0057, the switch, S3): every thing a row, each opening its own page
  // (the settings' North Star). The top: the preset, then the five categories (Sources, Effects, Mixers, Modulators,
  // In / Out), each pinnable whole, each with its modules and a "+ Add"; all the cables; this device. A module's page:
  // its knobs (a slider, its pin, what moves it), its other inputs, Randomize, Remove; a device's: its macros and the
  // modules in it. A port's page: the cables into it and "+ Add". A cable's page: its ends (each tapped to move it),
  // its channel, what scales it, its curve, its pin, Delete. Every change goes to the store as one edit (patch-edits.ts).
  import Icon from '../Icon.svelte';
  import { longpress } from '../longpress';
  import { held } from './held.svelte';
  import CurveEditor from './CurveEditor.svelte';
  import KnobCables from './KnobCables.svelte';
  import Face from './face/Face.svelte';
  import { faceOf } from './face/face';
  import type { LiveFrame } from './faces';
  import { curveOf, valueOf as kept, type Curve } from './curves';
  import { panelKey, type PanelEdit } from './patch-edits';
  import { categoryPin, labelOf, patchAt, typesOf } from './patchgraph';
  import { fits, isDevice, type CableV2, type Device, type PatchV2, type Port, type PortRef } from './ports';
  import { byCategory, CATEGORIES, CATEGORY_LABEL, deviceCategory, portsFrom, type Category } from './rack';
  import { DEVICES } from './devices';
  import { RANGES, saveVisualSettings, visualSettings as s } from './settings.svelte';
  import type { Module } from './patch';
  import type { V2Type } from './v2-types';

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
  }: {
    patch: PatchV2;
    types: Record<string, V2Type>;
    /** An edit, and the store's key for it (a knob dragged: one undo step). */
    onedit: (e: PanelEdit, key: string | null) => void;
    presets: readonly { id: string; label: string; line: string }[];
    /** The preset the patch was last set to, if any. */
    preset?: string | null;
    onpreset: (id: string) => void;
    /** Randomize a module (its path) or a category's whole. */
    onrandomize: (what: { module: string[] } | { category: Category }) => void;
    /** The engine's values now (four times a second), for a module's face. */
    live?: LiveFrame | null;
    /** The pages open, the latest last (a page's name: see `go`'s callers). */
    path?: string[];
  } = $props();

  const BAND_ROWS = ['Sub', 'Bass', 'Low mid', 'Mid', 'Upper mid', 'Air'];
  const here = $derived(path.at(-1) ?? '');
  const go = (page: string) => (path = [...path, page]);
  const back = (steps = 1) => (path = path.slice(0, Math.max(0, path.length - steps)));
  const edit = (e: PanelEdit) => onedit(e, panelKey(e));

  // Pages are named `kind|…` (ids hold dots and slashes): `m|a/b` a module at its path, `in|a/b|port` the cables into a
  // port, `c|scope|id` a cable, `from|scope|module|port` a source for a new cable, `move|end|scope|id` a cable's end
  // moved, `depth|scope|id` what scales it, `add|scope|category` a module added, and `presets`, `cables`, `device`.
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
  /** A module's name: a device's own, else its type's, numbered where its level has more than one. */
  function nameOf(m: Module, level: PatchV2): string {
    if (isDevice(m)) return (m as Device).label ?? 'Device';
    const t = types[m.type];
    const base = t ? labelOf(t) : m.type;
    const same = level.modules.filter((x) => x.type === m.type);
    return same.length > 1 ? `${base} ${same.indexOf(m) + 1}` : base;
  }
  const portName = (m: Module, port: string, dir: 'in' | 'out') => portsOf(m)?.[dir].find((p) => p.id === port)?.label ?? port;
  function endName(level: PatchV2, r: PortRef, dir: 'in' | 'out'): string {
    const m = level.modules.find((x) => x.id === r.module);
    if (!m) return r.module;
    const band = r.channel !== undefined ? ` · ${BAND_ROWS[r.channel] ?? r.channel + 1}` : '';
    return `${nameOf(m, level)} · ${portName(m, r.port, dir)}${band}`;
  }
  const cablesInto = (level: PatchV2, module: string, port: string) => level.cables.filter((c) => c.to.module === module && c.to.port === port);
  const pinned = (level: PatchV2, pin: string) => level.pins.includes(pin);

  const percent = (v: number) => `${Math.round(v * 100)}%`;
  const knobText = (k: { kind: string; min: number; max: number }, v: number) =>
    k.kind === 'step' ? `${Math.round(v)}` : k.min === 0 && k.max === 1 ? percent(v) : k.min === -1 && k.max === 1 ? `${v > 0 ? '+' : ''}${Math.round(v * 100)}%` : v.toFixed(2);
  // The inputs that are no knob's: a picture, light, points, a flow, sound, a trigger, a value of its own.
  const ownInputs = (m: Module, t: V2Type | undefined): Port[] =>
    (portsOf(m)?.in ?? []).filter((p) => !t?.knobs.some((k) => k.id === p.id) && !p.id.endsWith(':gain') && !p.id.endsWith(':next'));

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
    if (kind === 'c') return 'Cable';
    if (kind === 'from') return 'From';
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

{#snippet knobRow(at: string[], level: PatchV2, m: Module, k: V2Type['knobs'][number])}
  {@const v = m.knobs[k.id] ?? k.default}
  {@const n = cablesInto(level, m.id, k.id).length}
  <div class="knob">
    <label class="row pinnable">
      <span>{k.label}</span>
      <input type="range" min={k.min} max={k.max} step={k.kind === 'step' ? 1 : (k.max - k.min) / 100} value={v} oninput={(e) => edit({ kind: 'knob', module: at, knob: k.id, value: Number(e.currentTarget.value) })} />
      <output>{knobText(k, v)}</output>
      {@render pinButton(level, at.slice(0, -1), `${m.id}.${k.id}`, k.label)}
    </label>
    <button class="moves" class:on={n > 0} aria-label="What moves {k.label}{n ? ` (${n})` : ''}" use:held={'moves'} onclick={() => go(`in|${at.join('/')}|${k.id}`)}
      >{#if n}<span>{n}</span>{/if}<Icon name="chevronRight" size={16} /></button
    >
  </div>
{/snippet}

<div class="vpanel">
  {#if path.length}
    <div class="crumbs">
      <button class="back" aria-label="Back" onclick={() => back()}><Icon name="back" size={20} /></button>
      <span class="trail">{#each path as page, i (i)}{#if i}<span class="sep">›</span>{/if}<span class:here={i === path.length - 1}>{pageLabel(page)}</span>{/each}</span>
    </div>
  {/if}

  {#if !path.length}
    {#if patch.macros.some((m) => (m as { on?: string }).on === 'rack')}
      <div class="rows">
        <h3>Rack</h3>
        {#each patch.macros.filter((m) => (m as { on?: string }).on === 'rack') as x (x.id)}
          {@const v = (x as { value: number }).value}
          <label class="row">
            <span>{(x as { label: string }).label}</span>
            <input type="range" min="0" max="1" step="0.01" value={v} oninput={(e) => edit({ kind: 'macro', scope: [], id: x.id, value: Number(e.currentTarget.value) })} />
            <output>{percent(v)}</output>
          </label>
        {/each}
      </div>
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
    <div class="rows">
      {#each presets as p (p.id)}<button class="go" class:on={preset === p.id} onclick={() => (onpreset(p.id), (path = []))}><span class="name">{p.label}<small>{p.line}</small></span></button>{/each}
    </div>
  {:else if here === 'device'}
    <div class="rows">
      {#each DEVICE_ROWS as r (r.key)}
        <label class="row">
          <span>{r.label}</span>
          <input type="range" min={RANGES[r.key][0]} max={RANGES[r.key][1]} step={r.step} value={s[r.key]} oninput={(e) => setDevice(r.key, Number(e.currentTarget.value))} />
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
        {#if d.macros.length}
          <div class="rows">
            <h3>Macros</h3>
            {#each d.macros as x (x.id)}
              <label class="row">
                <span>{x.label}</span>
                <input type="range" min="0" max="1" step="0.01" value={x.value} oninput={(e) => edit({ kind: 'knob', module: at, knob: `macro:${x.id}`, value: Number(e.currentTarget.value) })} />
                <output>{percent(x.value)}</output>
              </label>
            {/each}
          </div>
        {/if}
        <div class="rows">
          <div class="head">
            <h3>In it</h3>
            {@render pinButton(level, scope, `module:${m.id}`, nameOf(m, level))}
          </div>
          {#each d.patch.modules as inner (inner.id)}{@render goRow(nameOf(inner, d.patch), '', `m|${[...at, inner.id].join('/')}`)}{/each}
        </div>
        <div class="rows">
          <button class="go" onclick={() => onrandomize({ module: at })}><span class="name">Randomize</span></button>
          <button class="go" onclick={() => (edit({ kind: 'unmake', scope, device: m.id }), back())}><span class="name">Unmake device</span></button>
          <button class="go danger" onclick={() => (edit({ kind: 'removeModule', scope, id: m.id }), back())}><span class="name">Remove</span></button>
        </div>
      {:else}
        {@const face = faceOf(t, portsOf(m))}
        {#if face.length}
          <div class="rows face"><Face parts={face} {live} path={at.join('/')} knobs={m.knobs} width={300} /></div>
        {/if}
        {@const knobs = (t?.knobs ?? []).filter((k) => !k.device)}
        {#if knobs.length}
          <div class="rows">
            <div class="head">
              <h3>Knobs</h3>
              {@render pinButton(level, scope, `module:${m.id}`, nameOf(m, level))}
            </div>
            {#each knobs as k (k.id)}{@render knobRow(at, level, m, k)}{/each}
          </div>
        {/if}
        {@const inputs = ownInputs(m, t)}
        {#if inputs.length}
          <div class="rows">
            <h3>Inputs</h3>
            {#each inputs as p (p.id)}
              {@const n = cablesInto(level, m.id, p.id).length}
              {@render goRow(p.label, n ? `${n} ${n === 1 ? 'cable' : 'cables'}` : p.normal ? 'As normalled' : 'Nothing', `in|${at.join('/')}|${p.id}`)}
            {/each}
          </div>
        {/if}
        <div class="rows">
          <button class="go" onclick={() => onrandomize({ module: at })}><span class="name">Randomize</span></button>
          {#if m.type !== 'screen'}<button class="go danger" onclick={() => (edit({ kind: 'removeModule', scope, id: m.id }), back())}><span class="name">Remove</span></button>{/if}
        </div>
      {/if}
    {/if}
  {:else if parts(here)[0] === 'in'}
    {@const [, a, port] = parts(here)}
    {@const at = pathOf(a)}
    {@const found = moduleAt(at)}
    {#if found}
      {@const { scope, level, m } = found}
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
    color: var(--accent);
  }

  .danger {
    color: var(--danger, #ff6b6b);
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
    color: var(--accent);
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
    color: var(--accent);
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
    color: var(--danger, #ff6b6b);
    font: inherit;
    cursor: pointer;
  }
</style>
