<script lang="ts">
  // Development only (graph-lab.html): PatchGraph alone on a sample patch held here, every callback applied, so the
  // graph can be edited as the panel would. ?cratarr shows the Cratarr chain (today's pipeline) instead; ?off starts
  // with the flame layer off (blended at nothing). A tapped cable shows a stand-in for its configuration.
  import PatchGraph from './PatchGraph.svelte';
  import { cratarrPatch, cratarrTypes, type Cable, type Knob, type Module, type ModuleType, type Patch } from './patch';
  import { MODULATORS } from './modulators/modulators';
  import { DEFAULTS, RANGES } from './settings.svelte';
  import type { XY } from './patchgraph';

  const k = (id: string, label: string, min: number, max: number, def: number, kind: Knob['kind'] = 'level', neutral = min): Knob => ({ id, label, min, max, neutral, default: def, kind });
  // Sources and modules the pipeline has not yet (step 4), for the sample.
  const extra: ModuleType[] = [
    { type: 'shapes', label: 'Shapes', place: 'source', knobs: [k('sides', 'Sides', 3, 12, 5, 'step', 3), k('spin', 'Spin', -1, 1, 0.2, 'swing', 0), k('size', 'Size', 0, 1, 0.5)] },
    { type: 'kaleidoscope', label: 'Kaleidoscope', place: 'module', knobs: [k('amount', 'Amount', 0, 1, 0.8), k('segments', 'Segments', 1, 12, 6, 'step', 1), k('turn', 'Turn', 0, 1, 0.35, 'swing', 0.35), k('zoom', 'Zoom', 0.5, 2, 1, 'swing', 1)] },
    { type: 'keep', label: 'Keep', place: 'loop', knobs: [k('decay', 'Decay', 0, 1, 0.9)] },
    { type: 'colour', label: 'Colour', place: 'finish', knobs: [k('hue', 'Hue', 0, 1, 0.2, 'swing', 0.2), k('vivid', 'Vivid', 0, 2, 1, 'swing', 1)] },
  ];
  const types: Record<string, ModuleType> = Object.fromEntries(
    [...cratarrTypes(RANGES as Record<string, [number, number]>, DEFAULTS), ...extra, ...Object.values(MODULATORS)].map((t) => [t.type, t]),
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
      modulators: [mod('beats', 'counter'), mod('bass', 'fast'), mod('drift', 'drift')],
      cables: [
        cable('c1', { signal: 'beat' }, 'beats', 'in', 1),
        cable('c2', { signal: 'level', band: 0 }, 'bass', 'in', 1),
        cable('c3', { modulator: 'beats' }, 'kaleido', 'turn', 0.4),
        cable('c4', { modulator: 'bass' }, 'tunnel', 'tunnel', 0.35, 'level'),
        cable('c5', { modulator: 'bass' }, 'fluid', 'stir', 0.3),
        { ...cable('c6', { modulator: 'drift' }, 'colour', 'hue', 0.15), off: true } as Cable,
        cable('c7', { signal: 'loudness', speed: 'slow' }, 'flame', 'hue', -0.2),
      ],
    };
  }

  const query = new URLSearchParams(location.search);
  const first = query.has('cratarr') ? cratarrPatch(DEFAULTS) : sample();
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
    positions = { ...positions, [id]: { x, y } };
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
</script>

<div class="lab">
  <PatchGraph
    {patch}
    {types}
    {positions}
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
  />
  {#if openCable}
    <aside><span>{openCable}</span><button aria-label="Close" onclick={() => (openCable = null)}>×</button></aside>
  {/if}
  <pre>{last}</pre>
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
