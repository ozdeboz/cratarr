<script lang="ts">
  // Development only (graph-lab.html): PatchGraph alone on a v2 patch held here in the patch store (patch-store.ts) as
  // the truth, as the app holds it: every edit a step, Ctrl+Z and Ctrl+Shift+Z. Today's settings as a v2 patch
  // (graph-lab.ts labV2); ?wires every wire type (wiresV2) with the control sources' live faces (an LFO run as the
  // engine runs it; the ADSRs and the beat stood in for here) and "New ▸ LFO / ADSR" from a knob dragged into the
  // empty. &open=show starts with Show open here; &layout=hand the hand layout in place of ELK's (to compare);
  // &cables=elk modulation cables on ELK's routes too; ?sheet a sheet over the bottom 40 %, as the page's (the graph
  // fits clear of it). A tapped cable shows a stand-in for its configuration.
  import { onMount } from 'svelte';
  import PatchGraph from './PatchGraph.svelte';
  import type { FrameInfo } from './patch';
  import type { PatchV2 } from './ports';
  import { MODULATORS } from './modulators/modulators';
  import { change, newStore, redo, undo } from './patch-store';
  import { helpFor } from './module-help';
  import { type PickRequest, type XY } from './patchgraph';
  import { applyEdit } from './patch-edits';
  import { LiveFaces } from './faces.svelte';
  import { labTypes, labV2, wiresV2 } from './graph-lab';
  import { splitMusic } from './music-signals';

  const query = new URLSearchParams(location.search);
  const types = labTypes();
  // ?split: Music's hub split into a module a signal beside what each feeds (what toV2 will do).
  // ?empty: an empty rack (its "+" the way in).
  const first = query.has('empty') ? { v: 2 as const, modules: [], cables: [], macros: [], pins: [] } : query.has('wires') ? wiresV2() : labV2();
  let store = $state.raw(newStore(query.has('split') ? splitMusic(first) : first));
  let at = $state<string[]>([]);
  let positions = $state.raw<Record<string, XY>>({});
  let openCable = $state<string | null>(null);
  let last = $state('');

  function onchange(next: PatchV2, key?: string) {
    store = change(store, () => next, key ?? null);
    last = `change${key ? ` ${key}` : ''} · ${store.past.length} back`;
  }
  function keys(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'z') return;
    e.preventDefault();
    store = e.shiftKey ? redo(store) : undo(store);
    last = `${e.shiftKey ? 'redo' : 'undo'} · ${store.past.length} back, ${store.future.length} on`;
  }

  let screenH = $state(typeof innerHeight === 'number' ? innerHeight : 800);
  const sheetH = $derived(query.has('sheet') ? Math.round(screenH * 0.4) : 0);

  // A knob dragged into the empty: "New ▸ LFO / ADSR" (the app's is the SourcePicker): the module where it was
  // let go, cabled into the knob, one step.
  let picking = $state<{ r: PickRequest; at: XY } | null>(null);
  function onpick(r: PickRequest, where: XY) {
    picking = { r, at: where };
    last = `pick for ${r.module.join('/')}.${r.knob} at ${where.x},${where.y}`;
  }
  function pickNew(type: string) {
    if (!picking) return;
    const { r, at: where } = picking;
    const scope = r.module.slice(0, -1);
    store = change(store, (p) => {
      applyEdit(p, { kind: 'add', scope, type }, types);
      const inside = scope.reduce<PatchV2>((q, id) => (q.modules.find((x) => x.id === id) as unknown as { patch: PatchV2 }).patch, p);
      const made = inside.modules[inside.modules.length - 1];
      positions = { ...positions, [`${at.join('/')}|${[...scope.slice(at.length), made.id].join('/')}`]: where };
      return applyEdit(p, { kind: 'connect', scope, from: { module: made.id, port: 'out' }, to: { module: r.module[r.module.length - 1], port: r.knob } }, types);
    });
    picking = null;
    last = `new ${type} → ${r.module.join('/')}.${r.knob}`;
  }

  // The thumbnails stood in for (the renderer draws them in the app): a wash of each module's own hue, moving.
  function standInThumb(module: string, canvas: HTMLCanvasElement): string | null {
    const g = canvas.getContext('2d');
    if (!g) return 'No canvas.';
    const hue = [...module].reduce((h, ch) => (h * 31 + ch.charCodeAt(0)) % 360, 7);
    const t = performance.now() / 1000;
    const grad = g.createLinearGradient(0, 0, canvas.width, canvas.height);
    grad.addColorStop(0, `hsl(${hue} 70% 18%)`);
    grad.addColorStop(0.5 + 0.4 * Math.sin(t + hue), `hsl(${(hue + 40) % 360} 85% 55%)`);
    grad.addColorStop(1, `hsl(${(hue + 80) % 360} 70% 12%)`);
    g.fillStyle = grad;
    g.fillRect(0, 0, canvas.width, canvas.height);
    return null;
  }

  // The live faces: the beat at 120 a minute, the LFO stepped as the engine steps it, the ADSRs stood in for (a gate on
  // each beat; one band's hit a beat, in turn).
  const live = new LiveFaces();
  onMount(() => {
    const lfo = MODULATORS.lfo.create();
    const t0 = performance.now();
    let before = t0;
    let raf = 0;
    const env = (since: number, k: Record<string, number>, held = 0.12) => {
      const a = Math.max(1e-3, k.attack);
      const d = Math.max(1e-3, k.decay);
      const curve = (t: number) => (t < a ? t / a : t < a + d ? 1 - (1 - k.sustain) * ((t - a) / d) : k.sustain);
      if (since < held) return curve(since);
      return curve(held) * Math.max(0, 1 - (since - held) / Math.max(1e-3, k.release));
    };
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const t = (now - t0) / 1000;
      const dt = (now - before) / 1000;
      before = now;
      const beats = t * 2;
      const index = Math.floor(beats);
      const beat: FrameInfo['beat'] = { index, since: (beats - index) / 2, length: 0.5, strength: 1 };
      // Every other module's outputs stood in for (a slow wave each, its own pace), so meters, scopes and lights move.
      const outputs: Record<string, number> = {};
      for (const [i, m] of store.now.modules.entries()) {
        if (m.type === 'lfo' || m.type === 'adsr') continue;
        const w = 0.5 + 0.5 * Math.sin(t * (0.7 + (i % 5) * 0.37) + i);
        for (const port of ['out', 'level', 'in', 'reduction', 'gain']) outputs[`${m.id}.${port}`] = port === 'reduction' ? 1 - 0.6 * w : w;
        for (const port of ['beat', 'bar', 'phrase', 'newtrack', 'downbeat', 'section']) outputs[`${m.id}.${port}`] = port === 'beat' ? (beat.since < 0.06 ? 1 : 0) : port === 'bar' ? (index % 4 === 0 && beat.since < 0.06 ? 1 : 0) : 0;
      }
      for (const m of store.now.modules) {
        const k = { amount: 1, ...Object.fromEntries((types[m.type]?.knobs ?? []).map((x) => [x.id, x.default])), ...m.knobs };
        if (m.type === 'lfo') outputs[m.id] = lfo.step(dt, () => 0, k);
        if (m.type !== 'adsr') continue;
        if (m.id === 'bands') for (let c = 0; c < 6; c++) outputs[`${m.id}.out.${c}`] = env(((beats - c / 3 + 60) % 2) / 2, k, 0.08);
        else outputs[m.id] = env(beat.since, k, 0.2);
      }
      // A Grid's points stood in for (the engine's frame gives them in the app): a lattice, breathing.
      const points: Record<string, { x: number; y: number }[]> = {};
      for (const m of store.now.modules)
        if (m.type === 'grid') points[m.id] = Array.from({ length: 64 }, (_, i) => ({ x: 0.5 + ((i % 8) - 3.5) * 0.11 * (1 + 0.15 * Math.sin(t)), y: 0.5 + (Math.floor(i / 8) - 3.5) * 0.11 * (1 + 0.15 * Math.cos(t)) }));
      live.offer(now, () => ({ time: t, beat, outputs, points }));
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

<svelte:window onkeydown={keys} bind:innerHeight={screenH} />
<div class="lab" style:--sheet-h="{sheetH}px">
  <PatchGraph
    patch={store.now}
    {onchange}
    {types}
    bind:at
    {positions}
    onmove={(key, x, y) => {
      positions = { ...positions, [key]: { x, y } };
      last = `move ${key} ${x},${y}`;
    }}
    onopen={(module) => (last = `open ${module.join('/')}`)}
    onopencable={(cable) => {
      openCable = cable.join('/');
      last = `open cable ${openCable}`;
    }}
    onopenknob={(module, knob) => (last = `open knob ${module.join('/')}.${knob}`)}
    {onpick}
    {live}
    thumbnail={standInThumb}
    openAtStart={query.get('open')?.split(',') ?? []}
    sheetHeight={sheetH}
    layout={query.get('layout') === 'hand' ? 'hand' : 'elk'}
    routeCables={query.get('cables') === 'elk'}
    onarrange={() => {
      positions = {};
      last = 'arranged';
    }}
    {helpFor}
    onedges={(list) => ((window as unknown as { __edges: unknown }).__edges = list)}
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
