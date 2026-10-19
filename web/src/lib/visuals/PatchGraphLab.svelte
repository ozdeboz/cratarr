<script lang="ts">
  // Development only (graph-lab.html): PatchGraph alone on a sample patch held here, every callback applied, so the
  // graph can be edited as the panel would. ?cratarr shows the Cratarr chain (today's pipeline) instead; ?off starts
  // with the flame layer off (blended at nothing). A tapped cable shows a stand-in for its configuration. ?wires: every
  // wire type, the normals and the control sources' live faces (an LFO run as the engine runs it; the ADSRs and the
  // beat stood in for here until claude-5f's engine gives them), and "New ▸ LFO / ADSR" from a knob dragged into the
  // empty. ?v2: today's settings as a v2 patch (v2.ts toV2), held in the patch store (patch-store.ts) as the truth,
  // drawn by the v2 graph: Show a device (open it here, or go into it), every edit a step, Ctrl+Z and Ctrl+Shift+Z;
  // &open=show starts with Show open here. ?sheet: a sheet over the bottom 40 %, as the page's (the graph fits clear of it).
  import { onMount } from 'svelte';
  import PatchGraph from './PatchGraph.svelte';
  import { cratarrPatch, cratarrTypes, type Cable, type Knob, type Module, type ModuleType, type Patch } from './patch';
  import { MODULATORS } from './modulators/modulators';
  import { ADSR, CLOCK, ENGINE_MODULES, ENVELOPE, GRAPH_MODULES, SEQUENCER, showPartTypes, type PatchV2 } from './ports';
  import { EXTRA_TYPES } from './registry';
  import { toV2 } from './v2';
  import { change, newStore, redo, undo } from './patch-store';
  import { helpFor } from './module-help';
  import { WAVES } from './sources/waves';
  import { DEFAULTS, RANGES } from './settings.svelte';
  import { applyV2, editKey, withModulator, type PickRequest, type V2Edit, type XY } from './patchgraph';
  import { LiveFaces } from './faces.svelte';
  import type { FrameInfo } from './patch';

  const k = (id: string, label: string, min: number, max: number, def: number, kind: Knob['kind'] = 'level', neutral = min): Knob => ({ id, label, min, max, neutral, default: def, kind });
  // Sources and modules the pipeline has not yet (step 4), for the sample.
  const extra: ModuleType[] = [
    { type: 'shapes', label: 'Shapes', place: 'source', knobs: [k('sides', 'Sides', 3, 12, 5, 'step', 3), k('spin', 'Spin', -1, 1, 0.2, 'swing', 0), k('size', 'Size', 0, 1, 0.5)] },
    { type: 'kaleidoscope', label: 'Kaleidoscope', place: 'module', knobs: [k('amount', 'Amount', 0, 1, 0.8), k('segments', 'Segments', 1, 12, 6, 'step', 1), k('turn', 'Turn', 0, 1, 0.35, 'swing', 0.35), k('zoom', 'Zoom', 0.5, 2, 1, 'swing', 1)] },
    { type: 'keep', label: 'Keep', place: 'loop', knobs: [k('decay', 'Decay', 0, 1, 0.9)] },
    { type: 'colour', label: 'Colour', place: 'finish', knobs: [k('hue', 'Hue', 0, 1, 0.2, 'swing', 0.2), k('vivid', 'Vivid', 0, 2, 1, 'swing', 1)] },
  ];
  const types: Record<string, ModuleType> = Object.fromEntries(
    [...cratarrTypes(RANGES as Record<string, [number, number]>, DEFAULTS), ...extra, ...Object.values(MODULATORS), WAVES, CLOCK, ADSR, ENVELOPE, SEQUENCER].map((t) => [t.type, t]),
  );
  const mod = (id: string, type: string, knobs: Record<string, number> = {}): Module => ({ id, type, knobs: { ...Object.fromEntries(types[type].knobs.map((x) => [x.id, x.default])), ...knobs } });
  const cable = (id: string, from: Cable['from'], module: string, knob: string, value: number, mode: Cable['mode'] = 'swing'): Cable => ({ id, from, to: { module, knob }, value, mode });

  function sample(): Patch {
    const base = cratarrPatch(DEFAULTS);
    return {
      ...base,
      layers: [
        { id: 'flame', source: base.layers[0].source, modules: [], loop: [], blend: { mode: 'add', amount: 1 } },
        { id: 'shapes', source: mod('shapes', 'shapes'), modules: [mod('kaleido', 'kaleidoscope')], loop: [], blend: { mode: 'screen', amount: 0.8 } },
      ],
      loop: [mod('fluid', 'fluid', { stir: 0.5, flow: 0.4 }), mod('tunnel', 'tunnel', { tunnel: 0.3 }), mod('keep', 'keep')],
      finish: [mod('colour', 'colour')],
      modulators: [mod('beats', 'counter'), mod('bass', 'fast'), mod('random', 'random')],
      cables: [
        cable('c1', { signal: 'beat' }, 'beats', 'in', 1),
        cable('c2', { signal: 'level', band: 0 }, 'bass', 'in', 1),
        cable('c3', { modulator: 'beats' }, 'kaleido', 'turn', 0.4),
        cable('c4', { modulator: 'bass' }, 'tunnel', 'tunnel', 0.35, 'level'),
        cable('c5', { modulator: 'bass' }, 'fluid', 'stir', 0.3),
        { ...cable('c6', { modulator: 'random' }, 'colour', 'hue', 0.15), off: true } as Cable,
        cable('c7', { signal: 'loudness', speed: 'slow' }, 'flame', 'hue', -0.2),
      ],
    };
  }

  // Every wire type: the flame's light into Show, pictures into the blend, the sound into Waves (its normal, a double
  // line), values dashed, triggers dotted, the bands' hits at once "· 6"; the Clock's jacks into a Sequencer by their
  // normals; an LFO, a single ADSR and one on the six bands, each with its face.
  function wires(): Patch {
    const base = cratarrPatch(DEFAULTS);
    return {
      ...base,
      layers: [
        { id: 'flame', source: base.layers[0].source, modules: base.layers[0].modules, loop: [], blend: { mode: 'add', amount: 1 } },
        { id: 'waves', source: mod('waves', 'waves'), modules: [], loop: [], blend: { mode: 'screen', amount: 0.8 } },
      ],
      loop: [],
      finish: [],
      modulators: [
        mod('clock', 'clock'),
        mod('lfo', 'lfo', { rate: 0.5, shape: 0.3 }),
        mod('adsr', 'adsr', { attack: 0.06, decay: 0.25, sustain: 0.45, release: 0.35 }),
        mod('bands', 'adsr', { attack: 0.02, decay: 0.2, sustain: 0.3, release: 0.3 }),
        mod('seq', 'sequencer'),
      ],
      cables: [
        cable('c1', { signal: 'beat' }, 'adsr', 'gate', 1),
        cable('c2', { signal: 'hits' }, 'bands', 'gate', 1),
        cable('c3', { modulator: 'lfo' }, 'waves', 'hue', 0.3),
        cable('c4', { modulator: 'adsr' }, 'show', 'glow', 0.4),
        cable('c5', { modulator: 'bands' }, 'flame', 'lighting.0', 0.5),
        cable('c6', { signal: 'level', band: 1 }, 'waves', 'amplitude', 0.4),
      ],
    };
  }

  const query = new URLSearchParams(location.search);

  // ---- ?v2: the store's patch ---------------------------------------------------------------------------------------
  const v2mode = query.has('v2');
  const chain = cratarrTypes(RANGES as Record<string, [number, number]>, DEFAULTS);
  const v2types: Record<string, ModuleType> = Object.fromEntries(
    [...chain, ...EXTRA_TYPES, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]),
  );
  // Today's settings as a v2 patch; the picture's wiring a stand-in here (the flame's light into Show, Show's picture
  // through the effects) until claude-632's pictureWiring gives it.
  function labV2(): PatchV2 {
    const p = toV2(DEFAULTS);
    const has = (id: string) => p.modules.some((m) => m.id === id);
    const chainOf = [['flame', 'light', 'show', 'light'], ['show', 'picture', 'fx1', 'picture'], ['fx1', 'picture', 'fx2', 'picture'], ['fx2', 'picture', 'fx3', 'picture']];
    chainOf.forEach(([a, ap, b, bp], i) => {
      if (has(a) && has(b)) p.cables.push({ id: `lab-${i + 1}`, from: { module: a, port: ap }, to: { module: b, port: bp }, value: 1, mode: 'swing' });
    });
    return p;
  }
  let store = $state.raw(newStore(v2mode ? labV2() : { v: 2, modules: [], cables: [], macros: [], pins: [] }));
  let at = $state<string[]>([]);
  let v2positions = $state.raw<Record<string, XY>>({});
  const level = $derived(at.join('/'));
  const here = $derived(Object.fromEntries(Object.entries(v2positions).flatMap(([k, v]) => (k.startsWith(`${level}|`) ? [[k.slice(level.length + 1), v]] : []))));
  function onedit(e: V2Edit) {
    store = change(store, (p) => applyV2(p, e, v2types), editKey(e));
    last = `${e.kind} ${JSON.stringify(e).slice(0, 120)} · ${store.past.length} back`;
  }
  const v2 = $derived(v2mode ? { patch: store.now, at, onedit, onenter: (next: string[]) => (at = next) } : undefined);
  function keys(e: KeyboardEvent) {
    if (!v2mode || !(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'z') return;
    e.preventDefault();
    store = e.shiftKey ? redo(store) : undo(store);
    last = `${e.shiftKey ? 'redo' : 'undo'} · ${store.past.length} back, ${store.future.length} on`;
  }

  let screenH = $state(typeof innerHeight === 'number' ? innerHeight : 800);
  const sheetH = $derived(query.has('sheet') ? Math.round(screenH * 0.4) : 0);

  const first = query.has('cratarr') ? cratarrPatch(DEFAULTS) : query.has('wires') ? wires() : sample();
  if (query.has('off')) first.layers[0] = { ...first.layers[0], blend: { ...first.layers[0].blend, amount: 0 } };
  let patch = $state.raw<Patch>(first);
  let openCable = $state<string | null>(null);
  let positions = $state.raw<Record<string, XY>>({});
  let last = $state('');
  let n = 0;

  const turn = (m: Module, id: string, knob: string, value: number): Module => (m.id === id ? { ...m, knobs: { ...m.knobs, [knob]: value } } : m);
  function onknob(id: string, knob: string, value: number) {
    const t = (m: Module) => turn(m, id, knob, value);
    patch = {
      ...patch,
      frame: t(patch.frame),
      layers: patch.layers.map((l) => ({ ...l, source: t(l.source), modules: l.modules.map(t), loop: l.loop.map(t) })),
      loop: patch.loop.map(t),
      finish: patch.finish.map(t),
      modulators: patch.modulators.map(t),
    };
    last = `knob ${id}.${knob} = ${value.toFixed(3)}`;
  }
  function onconnect(from: Cable['from'], to: Cable['to']) {
    const all = new Map([patch.frame, ...patch.layers.flatMap((l) => [l.source, ...l.modules, ...l.loop]), ...patch.loop, ...patch.finish, ...patch.modulators].map((m) => [m.id, m]));
    const kind = types[all.get(to.module)?.type ?? '']?.knobs.find((x) => x.id === to.knob)?.kind;
    patch = { ...patch, cables: [...patch.cables, cable(`new${++n}`, from, to.module, to.knob, 0.5, kind === 'level' ? 'level' : 'swing')] };
    last = `connect ${JSON.stringify(from)} → ${to.module}.${to.knob}`;
  }
  function ondisconnect(id: string) {
    patch = { ...patch, cables: patch.cables.filter((c) => c.id !== id) };
    last = `disconnect ${id}`;
  }
  function onmove(id: string, x: number, y: number) {
    if (v2mode) v2positions = { ...v2positions, [`${level}|${id}`]: { x, y } };
    else positions = { ...positions, [id]: { x, y } };
    last = `move ${id} ${x},${y}`;
  }
  const onopen = (id: string) => (last = `open ${id}`);
  function onopencable(id: string) {
    openCable = id;
    last = `open cable ${id}`;
  }
  const onopenknob = (id: string, knob: string) => (last = `open knob ${id}.${knob}`);
  // Cables and modules removed; a module with every cable to or from it.
  function onremove(ids: string[]) {
    const gone = new Set(ids);
    const keep = (m: Module) => !gone.has(m.id);
    patch = {
      ...patch,
      layers: patch.layers.filter((l) => !gone.has(l.source.id)).map((l) => ({ ...l, modules: l.modules.filter(keep), loop: l.loop.filter(keep) })),
      loop: patch.loop.filter(keep),
      finish: patch.finish.filter(keep),
      modulators: patch.modulators.filter(keep),
      cables: patch.cables.filter((c) => !gone.has(c.id) && !gone.has(c.to.module) && !('modulator' in c.from && gone.has(c.from.modulator))),
    };
    if (openCable && gone.has(openCable)) openCable = null;
    last = `remove ${ids.join(', ')}`;
  }
  function onreroute(id: string, end: 'from' | 'to', next: Cable['from'] | Cable['to']) {
    patch = { ...patch, cables: patch.cables.map((c) => (c.id === id ? { ...c, [end]: next } : c)) };
    last = `reroute ${id} ${end} → ${JSON.stringify(next)}`;
  }
  function onaddlayer(type: string) {
    const off = patch.layers.find((l) => l.id === type && l.blend.amount <= 0);
    if (off) patch = { ...patch, layers: patch.layers.map((l) => (l === off ? { ...l, blend: { ...l.blend, amount: 1 } } : l)) };
    else {
      const id = `${type}${++n}`;
      patch = { ...patch, layers: [...patch.layers, { id, source: mod(`${id}-source`, type), modules: [], loop: [], blend: { mode: 'add', amount: 1 } }] };
    }
    last = `add layer ${type}`;
  }
  function onaddmodule(where: string, type: string) {
    const m = mod(`${type}${++n}`, type);
    patch = where === 'loop' ? { ...patch, loop: [...patch.loop, m] } : { ...patch, layers: patch.layers.map((l) => (l.id === where ? { ...l, modules: [...l.modules, m] } : l)) };
    last = `add ${type} to ${where}`;
  }
  function onaddmodulator(type: string) {
    patch = { ...patch, modulators: [...patch.modulators, mod(`${type}${++n}`, type)] };
    last = `add modulator ${type}`;
  }
  // A knob dragged into the empty: "New ▸ LFO / ADSR" (the app's is claude-5f's SourcePicker), made as Visuals makes it.
  let picking = $state<{ r: PickRequest; at: XY } | null>(null);
  function onpick(r: PickRequest, at: XY) {
    picking = { r, at };
    last = `pick for ${r.module}.${r.knob} at ${at.x},${at.y}`;
  }
  function pickNew(type: string) {
    if (!picking) return;
    const { r, at } = picking;
    const id = `${type}${++n}`;
    const all = new Map([patch.frame, ...patch.layers.flatMap((l) => [l.source, ...l.modules, ...l.loop]), ...patch.loop, ...patch.finish, ...patch.modulators].map((m) => [m.id, m]));
    const kind = types[all.get(r.module)?.type ?? '']?.knobs.find((x) => x.id === r.knob)?.kind ?? 'swing';
    const made = withModulator({ routes: {}, extra: { layers: [], loop: [], modulators: patch.modulators, cables: patch.cables, macros: [] } }, mod(id, type), types[type], { module: r.module, knob: r.knob }, kind);
    patch = { ...patch, modulators: made.cables.extra.modulators, cables: made.cables.extra.cables };
    positions = { ...positions, [id]: at };
    picking = null;
    last = `new ${type} → ${r.module}.${r.knob}`;
  }

  // The live faces: the beat at 120 a minute, the LFO stepped as the engine steps it, the ADSRs stood in for (a gate on
  // each beat; one band's hit a beat, in turn).
  const live = new LiveFaces();
  onMount(() => {
    if (!query.has('wires')) return;
    const lfo = MODULATORS.lfo.create();
    const t0 = performance.now();
    let before = t0;
    let raf = 0;
    const env = (since: number, k: Record<string, number>, held = 0.12) => {
      const a = Math.max(1e-3, k.attack);
      const d = Math.max(1e-3, k.decay);
      const at = (t: number) => (t < a ? t / a : t < a + d ? 1 - (1 - k.sustain) * ((t - a) / d) : k.sustain);
      if (since < held) return at(since);
      return at(held) * Math.max(0, 1 - (since - held) / Math.max(1e-3, k.release));
    };
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const t = (now - t0) / 1000;
      const dt = (now - before) / 1000;
      before = now;
      const beats = t * 2;
      const index = Math.floor(beats);
      const beat: FrameInfo['beat'] = { index, since: (beats - index) / 2, length: 0.5, strength: 1 };
      const knobsOf = (id: string) => patch.modulators.find((m) => m.id === id)?.knobs ?? {};
      const outputs: Record<string, number> = {};
      for (const m of patch.modulators) {
        const k = { amount: 1, ...m.knobs };
        if (m.type === 'lfo') outputs[m.id] = lfo.step(dt, () => 0, knobsOf(m.id));
        if (m.type !== 'adsr') continue;
        if (m.id === 'bands') for (let c = 0; c < 6; c++) outputs[`${m.id}.out.${c}`] = env(((beats - c / 3 + 60) % 2) / 2, k, 0.08);
        else outputs[m.id] = env(beat.since, k, 0.2);
      }
      live.offer(now, () => ({ time: t, beat, outputs }));
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

<svelte:window onkeydown={keys} bind:innerHeight={screenH} />
<div class="lab" style:--sheet-h="{sheetH}px">
  <PatchGraph
    patch={v2mode ? undefined : patch}
    {v2}
    types={v2mode ? v2types : types}
    positions={v2mode ? here : positions}
    openAtStart={query.get('open')?.split(',') ?? []}
    sheetHeight={sheetH}
    {helpFor}
    {onconnect}
    {ondisconnect}
    {onknob}
    {onmove}
    {onopen}
    {onopencable}
    {onopenknob}
    {onremove}
    {onreroute}
    {onaddlayer}
    {onaddmodule}
    {onaddmodulator}
    {onpick}
    {live}
  />
  {#if picking}
    <aside class="pick">
      <button onclick={() => pickNew('lfo')}>New ▸ LFO</button><button onclick={() => pickNew('adsr')}>New ▸ ADSR</button><button aria-label="Close" onclick={() => (picking = null)}>×</button>
    </aside>
  {/if}
  {#if openCable}
    <aside><span>{openCable}</span><button aria-label="Close" onclick={() => (openCable = null)}>×</button></aside>
  {/if}
  <pre>{last}</pre>
  {#if sheetH}<div class="sheet" style:height="{sheetH}px"></div>{/if}
</div>

<style>
  .lab {
    position: fixed;
    inset: 16px;
    border: 1px solid var(--border);
    border-radius: 20px;
    overflow: hidden;
  }
  aside {
    position: absolute;
    top: 48px;
    right: 16px;
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 16px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--muted);
    font-size: 13px;
  }
  aside button {
    border: 0;
    background: none;
    color: inherit;
    font-size: 16px;
    cursor: pointer;
  }
  .sheet {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    border-top: 1px solid var(--border);
    border-radius: 16px 16px 0 0;
    background: var(--surface);
  }
  pre {
    position: absolute;
    left: 16px;
    bottom: 8px;
    margin: 0;
    color: var(--muted);
    font-size: 12px;
    pointer-events: none;
  }
</style>
