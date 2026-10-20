// The graph builder's mapping (decision 0056 step 3, patchgraph.ts): a patch as nodes and edges, picture cables in the
// patch's order, every cable an edge onto its knob, the knobs shown and their rings, connections checked; a layer that is
// off, muted cables, the adds, a cable's end rerouted, the cables' sag (after VCV Rack's): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrPatch, cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { MODULATORS } = await import('../src/lib/visuals/modulators/modulators.ts');
const { ADD_LAYER, ADD_MODULATOR, BLEND, SCREEN, LOOP, amountLabel, cableOf, drawnOpacity, graphOf, labelOf, layerBox, nearestEnd, nextOpacity, rerouted, ringOf, saggingPath, signalKey, slumpOf, turned } = await import('../src/lib/visuals/patchgraph.ts');
type Patch = import('../src/lib/visuals/patch.ts').Patch;
type ModuleType = import('../src/lib/visuals/patch.ts').ModuleType;
type Module = import('../src/lib/visuals/patch.ts').Module;
type ModuleData = import('../src/lib/visuals/patchgraph.ts').ModuleData;

const knob = (id: string, kind: 'level' | 'swing' | 'step' = 'level', device = false) => ({ id, label: id, min: 0, max: 1, neutral: 0, default: 0.5, kind, ...(device ? { device } : {}) });
const types: Record<string, ModuleType> = {
  frame: { type: 'frame', label: 'Whole frame', place: 'frame', knobs: [knob('intensity'), knob('quality', 'level', true)] },
  src: { type: 'src', label: 'Source', place: 'source', knobs: [knob('a'), knob('b'), knob('c'), knob('d'), knob('e')] },
  fx: { type: 'fx', label: 'Effect', place: 'module', knobs: [knob('amount'), knob('turn', 'swing')] },
  move: { type: 'move', label: 'Move', place: 'loop', knobs: [knob('amount')] },
  ...MODULATORS,
};
const mod = (id: string, type: string): Module => ({ id, type, knobs: {} });

function sample(): Patch {
  return {
    v: 1,
    frame: mod('frame', 'frame'),
    layers: [
      { id: 'one', source: mod('s1', 'src'), modules: [mod('k1', 'fx')], loop: [], blend: { mode: 'add', amount: 1 } },
      { id: 'two', source: mod('s2', 'src'), modules: [], loop: [mod('m1', 'move'), mod('m2', 'move')], blend: { mode: 'screen', amount: 0.5 } },
    ],
    loop: [mod('l1', 'move'), mod('l2', 'move')],
    finish: [mod('f1', 'fx'), mod('f2', 'fx')],
    modulators: [mod('lfo', 'lfo'), mod('fol', 'fast')],
    cables: [
      { id: 'c1', from: { modulator: 'lfo' }, to: { module: 'k1', knob: 'turn' }, value: 0.4, mode: 'swing' },
      { id: 'c2', from: { signal: 'level', band: 0 }, to: { module: 'fol', knob: 'in' }, value: 1, mode: 'swing' },
      { id: 'c3', from: { modulator: 'fol' }, to: { module: 's1', knob: 'e' }, value: 0.5, mode: 'level' },
      { id: 'c4', from: { signal: 'beat' }, to: { module: 'gone', knob: 'x' }, value: 1, mode: 'swing' },
    ],
    macros: [],
    pins: [],
  };
}

function freeze<T>(o: T): T {
  if (o && typeof o === 'object') {
    for (const v of Object.values(o)) freeze(v);
    Object.freeze(o);
  }
  return o;
}

test('engine modules are named by their engine, as the panel names them; others by their type', () => {
  assert.equal(labelOf({ type: 'kaleidoscope-engine', label: 'Effect 1' }), 'Kaleidoscope');
  assert.equal(labelOf({ type: 'eyedo-engine', label: 'Effect 2' }), 'EYEDO');
  assert.equal(labelOf({ type: 'hydra-engine', label: 'Effect 3' }), 'Hydra');
  assert.equal(labelOf({ type: 'shapes', label: 'Shapes' }), 'Shapes');
});

test('rings: none uncabled, a swing above, a level its share under the setting, clamped', () => {
  assert.equal(ringOf(0.5, []), null);
  assert.deepEqual(ringOf(0.5, [{ value: 0.4, mode: 'swing' }]), [0.5, 0.9]);
  assert.deepEqual(ringOf(0.5, [{ value: -0.2, mode: 'swing' }]), [0.3, 0.5]);
  assert.deepEqual(ringOf(0.8, [{ value: 0.5, mode: 'level' }]), [0.4, 0.8]);
  assert.deepEqual(ringOf(0.9, [{ value: 0.5, mode: 'swing' }, { value: { h1: [0.3, 0.1], h2: [0.6, 0.2], h: 0.5 }, mode: 'swing' }]), [0.9, 1]);
});

test('amounts: a swing signed, a level a share, a curve by its end', () => {
  assert.equal(amountLabel(0.4, 'swing'), '+40%');
  assert.equal(amountLabel(-0.15, 'swing'), '−15%');
  assert.equal(amountLabel(0.35, 'level'), '35%');
  assert.equal(amountLabel({ h1: [0.2, 0.5], h2: [0.5, 0.9], h: 1 }, 'swing'), '+100%');
});

test('picture cables run in the patch order, into the blend, through the loop and finish to the screen', () => {
  const { edges } = graphOf({ patch: freeze(sample()), types });
  const pic = edges.filter((e) => e.class === 'picture').map((e) => `${e.source}>${e.target}`);
  assert.deepEqual(pic.sort(), ['s1>k1', 'k1>@blend', 's2>m1', 'm1>m2', 'm2>@blend', '@blend>l1', 'l1>l2', 'l2>f1', 'f1>f2', 'f2>@screen'].sort());
  assert.ok(edges.filter((e) => e.class !== 'cable').every((e) => !e.deletable));
  assert.equal(edges.find((e) => e.id === 'pic:m2>@blend')!.label, 'screen 50%');
  assert.equal(edges.find((e) => e.id === 'pic:k1>@blend')!.label, undefined);
  // Each loop's last back to its first.
  assert.deepEqual(edges.filter((e) => e.class === 'feedback').map((e) => `${e.source}>${e.target}`), ['m2>m1', 'l2>l1']);
});

test('layers and the loop are boxes holding their modules; the frame, signals and modulators stand alone', () => {
  const { nodes } = graphOf({ patch: sample(), types });
  const by = new Map(nodes.map((n) => [n.id, n]));
  for (const [id, box] of [['s1', layerBox('one')], ['k1', layerBox('one')], ['s2', layerBox('two')], ['m2', layerBox('two')], ['l1', LOOP]]) assert.equal(by.get(id)!.parentId, box);
  for (const id of ['frame', 'f1', BLEND, SCREEN, 'lfo', signalKey({ signal: 'beat' })]) assert.equal(by.get(id)?.parentId, undefined, id);
  assert.equal(by.get(layerBox('one'))!.data.kind, 'box');
  // A signal only a cable to nothing names is still drawn (it is in the patch); its cable is not.
  assert.ok(by.has(signalKey({ signal: 'beat' })));
  assert.equal(nodes.length, new Set(nodes.map((n) => n.id)).size);
});

test('every cable onto its knob or port, dashed and deletable; one to nothing is left out', () => {
  const { edges } = graphOf({ patch: sample(), types });
  const cables = edges.filter((e) => e.class === 'cable');
  assert.deepEqual(
    cables.map((e) => [e.id, e.source, e.sourceHandle, e.target, e.targetHandle, e.deletable]),
    [
      ['c1', 'lfo', 'mod', 'k1', 'k:turn', true],
      ['c2', signalKey({ signal: 'level', band: 0 }), 'mod', 'fol', 'k:in', true],
      ['c3', 'fol', 'mod', 's1', 'k:e', true],
    ],
  );
  assert.equal(cables[0].data!.amount, '+40%');
});

test('knobs: a few shown, the cabled ones always, never the device’s; opened out, all; rings from the cables', () => {
  const g = graphOf({ patch: sample(), types });
  const data = (id: string) => g.nodes.find((n) => n.id === id)!.data as ModuleData;
  assert.deepEqual(data('s1').knobs.map((k) => k.id), ['a', 'b', 'c', 'e']);
  assert.equal(data('s1').hidden, 1);
  assert.deepEqual(data('s1').knobs.find((k) => k.id === 'e')!.ring, [0.25, 0.5]);
  assert.equal(data('s1').knobs[0].ring, null);
  assert.deepEqual(data('frame').knobs.map((k) => k.id), ['intensity']);
  // A modulator shows only its cabled knobs until opened; its ports always.
  assert.deepEqual(data('lfo').knobs, []);
  assert.deepEqual(data('fol').ports, [{ id: 'in', label: 'Input', type: 'value', cabled: true }]);
  const open = graphOf({ patch: sample(), types, open: new Set(['s1', 'lfo']) });
  const o = (id: string) => open.nodes.find((n) => n.id === id)!.data as ModuleData;
  assert.equal(o('s1').knobs.length, 5);
  assert.equal(o('s1').hidden, 0);
  assert.deepEqual(o('lfo').knobs.map((k) => k.id), ['rate', 'shape', 'offset', 'ratio', 'amount']);
});

test('signals and modulators sit under what they move, a port’s feed beside its modulator', () => {
  const { nodes } = graphOf({ patch: sample(), types });
  const by = new Map(nodes.map((n) => [n.id, n]));
  const k1 = by.get('k1')!;
  const box = by.get(layerBox('one'))!;
  const turn = box.position.x + k1.position.x + 10 + 1 * 44 + 22;
  const lfo = by.get('lfo')!;
  assert.ok(Math.abs(lfo.position.x + 75 - turn) < 1, `lfo at ${lfo.position.x}, Turn at ${turn}`);
  const level = by.get(signalKey({ signal: 'level', band: 0 }))!;
  assert.ok(level.position.x < by.get('fol')!.position.x);
  assert.ok(Math.min(lfo.position.y, level.position.y) > Math.max(...nodes.filter((n) => n.data.kind === 'module' && !(n.data as ModuleData).modulator && !n.parentId).map((n) => n.position.y)));
});

test('positions given are kept, a box grows round a module moved inside it', () => {
  const auto = graphOf({ patch: sample(), types });
  const box = auto.nodes.find((n) => n.id === layerBox('one'))!;
  const moved = graphOf({ patch: sample(), types, positions: { k1: { x: 900, y: 300 }, [BLEND]: { x: 5, y: 6 } } });
  const by = new Map(moved.nodes.map((n) => [n.id, n]));
  assert.deepEqual(by.get('k1')!.position, { x: 900, y: 300 });
  assert.deepEqual(by.get(BLEND)!.position, { x: 5, y: 6 });
  assert.ok(by.get(layerBox('one'))!.width! > Math.max(box.width!, 900));
  assert.ok(by.get(layerBox('one'))!.height! > 300);
});

test('signals asked for are drawn once, beside those cabled', () => {
  const g = graphOf({ patch: sample(), types, signals: [{ signal: 'beat' }, { signal: 'tempo' }, { signal: 'level', band: 0, speed: 'slow' }] });
  const ids = g.nodes.filter((n) => n.data.kind === 'signal').map((n) => n.id);
  assert.deepEqual(ids.sort(), ['signal:beat|-|track|fast', 'signal:level|0|track|fast', 'signal:level|0|track|slow', 'signal:tempo|-|track|fast']);
});

test('connections: from an output to a knob or a port, never to a picture handle, itself, or twice', () => {
  const p = sample();
  const { nodes } = graphOf({ patch: p, types });
  const c = (source: string, target: string, targetHandle: string, sourceHandle = 'mod') => cableOf({ source, target, sourceHandle, targetHandle }, nodes, p.cables);
  assert.deepEqual(c('lfo', 'f1', 'k:amount'), { from: { modulator: 'lfo' }, to: { module: 'f1', knob: 'amount' } });
  assert.deepEqual(c(signalKey({ signal: 'beat' }), 'fol', 'k:in'), { from: { signal: 'beat' }, to: { module: 'fol', knob: 'in' } });
  assert.equal(c('lfo', 'k1', 'k:turn'), null);
  assert.equal(c('lfo', 'f1', 'in'), null);
  assert.equal(c('fol', 'fol', 'k:in'), null);
  assert.equal(c('s1', 'f1', 'k:amount', 'out'), null);
  assert.equal(c('lfo', BLEND, 'k:in'), null);
});

test('turning: clamped to the range, a step by whole steps', () => {
  const k = { min: 0, max: 1, kind: 'level' as const };
  assert.equal(turned(k, 0.5, 80), 1);
  assert.equal(turned(k, 0.5, -40), 0.25);
  assert.equal(turned({ min: 1, max: 12, kind: 'step' }, 6, 20), 7);
  assert.equal(turned({ min: 1, max: 12, kind: 'step' }, 6, 2), 6);
});

test('the Cratarr chain draws whole: every cable onto a knob, no node twice', () => {
  const s = JSON.parse(JSON.stringify(DEFAULTS));
  const patch = cratarrPatch(s);
  const all = Object.fromEntries(cratarrTypes(RANGES as Record<string, [number, number]>, s).map((t: ModuleType) => [t.type, t]));
  const g = graphOf({ patch, types: all });
  assert.equal(g.edges.filter((e) => e.class === 'cable').length, patch.cables.length);
  assert.equal(g.nodes.length, new Set(g.nodes.map((n) => n.id)).size);
  const pic = g.edges.filter((e) => e.class === 'picture').map((e) => e.target);
  assert.deepEqual(pic.slice(-4), ['fx1', 'fx2', 'fx3', SCREEN]);
});

test('a layer blended at nothing is one row, "off": none of its modules, no picture into the blend, no cables into it', () => {
  const p = sample();
  p.layers[0] = { ...p.layers[0], blend: { ...p.layers[0].blend, amount: 0 } };
  const { nodes, edges } = graphOf({ patch: p, types });
  const row = nodes.find((n) => n.id === layerBox('one'))!;
  assert.deepEqual(row.data, { kind: 'off', layer: 'one', label: 'Source' });
  assert.ok(!nodes.some((n) => n.id === 's1' || n.id === 'k1'));
  assert.ok(!edges.some((e) => e.target === BLEND && e.source === 'k1'));
  assert.deepEqual(edges.filter((e) => e.target === BLEND).map((e) => e.source), ['m2']);
  // c1 (into k1) and c3 (into s1) have nothing to go into.
  assert.deepEqual(edges.filter((e) => e.class === 'cable').map((e) => e.id), ['c2']);
});

test('a muted cable is drawn as one, and moves no ring', () => {
  const p = sample();
  p.cables = p.cables.map((c) => (c.id === 'c3' ? { ...c, off: true } : c));
  const g = graphOf({ patch: p, types });
  assert.equal(g.edges.find((e) => e.id === 'c3')!.data!.muted, true);
  assert.equal(g.edges.find((e) => e.id === 'c1')!.data!.muted, undefined);
  const e = (g.nodes.find((n) => n.id === 's1')!.data as ModuleData).knobs.find((k) => k.id === 'e')!;
  assert.equal(e.ring, null);
});

test('adds: "+ Add layer" under the layers, "+ Add modulator" by the modulators, a box’s "+" where it takes one', () => {
  assert.ok(!graphOf({ patch: sample(), types }).nodes.some((n) => n.data.kind === 'add'));
  const g = graphOf({ patch: sample(), types, adds: true, addTo: (w) => w !== 'two' });
  const by = new Map(g.nodes.map((n) => [n.id, n]));
  assert.equal(by.get(ADD_LAYER)!.data.kind, 'add');
  assert.ok(by.get(ADD_LAYER)!.position.y > by.get(layerBox('two'))!.position.y);
  assert.ok(by.get(ADD_MODULATOR)!.position.y > by.get('lfo')!.position.y);
  assert.equal((by.get(layerBox('one'))!.data as { add?: string }).add, 'one');
  assert.equal((by.get(layerBox('two'))!.data as { add?: string }).add, undefined);
  assert.equal((by.get(LOOP)!.data as { add?: string }).add, 'loop');
});

test('rerouting: the nearer end; the knob end onto another knob, the source end onto another output; checked', () => {
  assert.equal(nearestEnd({ x: 10, y: 10 }, { x: 0, y: 0 }, { x: 100, y: 100 }), 'from');
  assert.equal(nearestEnd({ x: 90, y: 80 }, { x: 0, y: 0 }, { x: 100, y: 100 }), 'to');
  const p = sample();
  const { nodes } = graphOf({ patch: p, types });
  const c1 = p.cables[0]; // lfo → k1.turn
  const now = (source: string, target: string, targetHandle: string) => rerouted(c1, { source, sourceHandle: 'mod', target, targetHandle }, nodes, p.cables);
  assert.deepEqual(now('lfo', 'f1', 'k:amount'), { end: 'to', next: { module: 'f1', knob: 'amount' } });
  assert.deepEqual(now('fol', 'k1', 'k:turn'), { end: 'from', next: { modulator: 'fol' } });
  assert.deepEqual(now(signalKey({ signal: 'beat' }), 'k1', 'k:turn'), { end: 'from', next: { signal: 'beat' } });
  // Back where it was, onto a picture handle, or both ends at once: nothing.
  assert.equal(now('lfo', 'k1', 'k:turn'), null);
  assert.equal(now('lfo', 'f1', 'in'), null);
  assert.equal(now('fol', 'f1', 'k:amount'), null);
  // Not onto a knob already cabled from the same source.
  const more = [...p.cables, { id: 'c5', from: { modulator: 'lfo' }, to: { module: 'f1', knob: 'amount' }, value: 0.2, mode: 'level' as const }];
  assert.equal(rerouted(c1, { source: 'lfo', sourceHandle: 'mod', target: 'f1', targetHandle: 'k:amount' }, nodes, more), null);
});

test('cables sag as VCV Rack’s do: through the midpoint lowered by (1 − tension)(150 + length)', () => {
  assert.deepEqual(slumpOf({ x: 0, y: 0 }, { x: 100, y: 0 }, 1), { x: 50, y: 0 });
  assert.deepEqual(slumpOf({ x: 0, y: 0 }, { x: 100, y: 0 }, 0.5), { x: 50, y: 125 });
  const short = slumpOf({ x: 0, y: 0 }, { x: 10, y: 0 }).y;
  const long = slumpOf({ x: 0, y: 0 }, { x: 400, y: 0 }).y;
  assert.ok(long > short && short > 0);
  const [path, mid] = saggingPath({ x: 0, y: 0 }, { x: 100, y: 0 }, 0.5);
  assert.equal(path, 'M0,0 Q50,125 100,0');
  assert.deepEqual(mid, { x: 50, y: 62.5 });
});

test('cable opacity: 100, 50, 20 % in turn, drawn as Rack draws it (to the power 1.5)', () => {
  assert.equal(nextOpacity(1), 0.5);
  assert.equal(nextOpacity(0.5), 0.2);
  assert.equal(nextOpacity(0.2), 1);
  assert.equal(nextOpacity(0.7), 0.5);
  assert.equal(drawnOpacity(1), 1);
  assert.ok(Math.abs(drawnOpacity(0.25) - 0.125) < 1e-9);
});

// ---- Typed wires and ports, normals, bundles, faces, "New ▸ LFO / ADSR" (0057 §1, §4b) ------------------------------

const { WIRES, MUTED_DASH, wireStyle, bundleLabel, cableLabel, sourcePort, pictureFits, withModulator, CLOCK_BEAT } = await import('../src/lib/visuals/patchgraph.ts');
const { ADSR, CLOCK, ENVELOPE, SEQUENCER } = await import('../src/lib/visuals/ports.ts');
const { WAVES, soundOf } = await import('../src/lib/visuals/sources/waves.ts');
const { withInputs } = await import('../src/lib/visuals/compose.ts');
const faces = await import('../src/lib/visuals/faces.ts');
type GraphEdge = import('../src/lib/visuals/patchgraph.ts').GraphEdge;

const typed: Record<string, ModuleType> = { ...types, waves: WAVES, adsr: ADSR, clock: CLOCK, envelope: ENVELOPE, sequencer: SEQUENCER };
function wired(cables: Patch['cables'], modulators: Module[] = []): Patch {
  return {
    ...sample(),
    layers: [{ id: 'w', source: mod('waves', 'waves'), modules: [], loop: [], blend: { mode: 'add', amount: 1 } }],
    loop: [],
    finish: [],
    modulators: [mod('lfo', 'lfo'), mod('adsr', 'adsr'), mod('clock', 'clock'), mod('seq', 'sequencer'), mod('env', 'envelope'), ...modulators],
    cables,
  };
}
const edgeOf = (edges: GraphEdge[], id: string) => edges.find((e) => e.id === id);

test('wires by type: picture solid, light solid and thicker, value dashed, trigger dotted, audio a double line', () => {
  assert.equal(WIRES.picture.dash, null);
  assert.equal(WIRES.light.dash, null);
  assert.ok(WIRES.light.width > WIRES.picture.width);
  assert.equal(WIRES.value.dash, '6 5');
  const [on, off] = WIRES.trigger.dash!.split(' ').map(Number);
  assert.ok(on < 0.5 && off > on, 'dots: next to no dash, a gap');
  assert.ok(WIRES.audio.double && !WIRES.value.double && !WIRES.picture.double);
  assert.equal(wireStyle('value'), '--wire-w: 1.5px; --wire-dash: 6 5');
  assert.ok(wireStyle('trigger', true).endsWith(MUTED_DASH));
  const { edges } = graphOf({
    patch: wired([
      { id: 'b', from: { signal: 'beat' }, to: { module: 'adsr', knob: 'gate' }, value: 1, mode: 'swing' },
      { id: 'l', from: { modulator: 'lfo' }, to: { module: 'waves', knob: 'hue' }, value: 0.3, mode: 'swing' },
      { id: 't', from: { signal: 'beat' }, to: { module: 'waves', knob: 'width' }, value: 0.3, mode: 'level' },
    ]),
    types: typed,
  });
  assert.equal(edgeOf(edges, 'b')?.data?.wire, 'trigger');
  assert.equal(edgeOf(edges, 'l')?.data?.wire, 'value');
  assert.equal(edgeOf(edges, 't')?.data?.wire, 'trigger', 'a trigger into a value is still drawn as a trigger');
  assert.equal(edgeOf(edges, 'pic:waves>@blend')?.class, 'picture');
  // A source that gives light (the flame's, before Show) draws it as light.
  const lit = graphOf({ patch: sample(), types: { ...types, src: { ...types.src, ports: { in: [], out: [{ id: 'light', label: 'Light', type: 'light' }] } } as ModuleType } });
  assert.equal(edgeOf(lit.edges, 'pic:s1>k1')?.class, 'light');
  assert.equal(edgeOf(lit.edges, 'pic:k1>@blend')?.class, 'picture');
});

test('ports and handles by type; connections only where the types fit; a picture cycle only through a Feedback', () => {
  const { nodes } = graphOf({ patch: wired([]), types: typed });
  const d = (id: string) => nodes.find((n) => n.id === id)!.data as ModuleData;
  assert.deepEqual(d('waves').ports.map((p) => [p.id, p.type]), [['audio', 'audio']]);
  assert.deepEqual(d('adsr').ports.map((p) => [p.id, p.type, p.channels]), [['gate', 'trigger', 6]]);
  // Polyphonic as many channels as come in (VCV Rack's rule): nothing in, one.
  assert.deepEqual(d('adsr').gives, { type: 'value' });
  assert.deepEqual(
    d('clock').outs.map((o) => [o.id, o.type, o.handle]),
    [
      ['phase', 'value', 'mod'],
      ['rate', 'value', 'o:rate'],
      ['beat', 'trigger', 'o:beat'],
      ['bar', 'trigger', 'o:bar'],
      ['phrase', 'trigger', 'o:phrase'],
    ],
  );
  // Music's beat into the ADSR's gate fits; an LFO's value into it does not; a trigger into a knob does.
  const sig = graphOf({ patch: wired([]), types: typed, signals: [{ signal: 'beat' }] });
  const beat = signalKey({ signal: 'beat' });
  assert.ok(cableOf({ source: beat, target: 'adsr', sourceHandle: 'mod', targetHandle: 'k:gate' }, sig.nodes, []));
  assert.equal(cableOf({ source: 'lfo', target: 'adsr', sourceHandle: 'mod', targetHandle: 'k:gate' }, sig.nodes, []), null);
  assert.ok(cableOf({ source: beat, target: 'waves', sourceHandle: 'mod', targetHandle: 'k:width' }, sig.nodes, []));
  // Music's sound is no cable's source: its node takes none.
  const sound = nodes.find((n) => n.data.kind === 'signal' && n.data.type === 'audio')!;
  assert.equal(sound.connectable, false);
  assert.equal(cableOf({ source: sound.id, target: 'waves', sourceHandle: 'mod', targetHandle: 'k:audio' }, nodes, []), null);
  // Pictures: forward fine; back into the chain only through a Feedback.
  const chain = [
    { source: 'a', target: 'b', class: 'picture' },
    { source: 'b', target: 'c', class: 'picture w-picture' },
  ];
  const at = (id: string, type: string) => ({ id, data: { kind: 'module', type, module: id } as unknown as ModuleData });
  const ns = [at('a', 'src'), at('b', 'fx'), at('c', 'fx'), at('fb', 'feedback')];
  assert.ok(pictureFits({ source: 'c', target: '@screen', sourceHandle: 'out', targetHandle: 'in' }, ns, chain));
  assert.ok(!pictureFits({ source: 'c', target: 'a', sourceHandle: 'out', targetHandle: 'in' }, ns, chain));
  assert.ok(pictureFits({ source: 'fb', target: 'a', sourceHandle: 'out', targetHandle: 'in' }, ns, [...chain, { source: 'c', target: 'fb', class: 'picture' }]));
  assert.ok(!pictureFits({ source: 'c', target: 'a', sourceHandle: 'mod', targetHandle: 'k:x' }, ns, chain));
});

test("a port's normal is drawn faint until a cable is plugged into it, then it is gone", () => {
  const { edges, nodes } = graphOf({ patch: wired([]), types: typed });
  const normal = (id: string) => edgeOf(edges, `normal:${id}`);
  // Music's sound into Waves: a double line from the Sound; the Loudness from Music's loudness; the Clock's tempo.
  const sound = normal('waves.audio')!;
  assert.equal(sound.class, 'normal w-audio');
  assert.equal((nodes.find((n) => n.id === sound.source)!.data as { label: string }).label, 'Sound');
  assert.ok(sound.data?.normal && !sound.selectable && !sound.deletable);
  assert.equal(normal('env.in')?.source, signalKey({ signal: 'loudness' }));
  assert.equal(normal('clock.tempo')?.data?.wire, 'value');
  // From a module's jack: the Sequencer stepped by the Clock's beat, reset by its bar.
  assert.deepEqual([normal('seq.clock')?.source, normal('seq.clock')?.sourceHandle, normal('seq.clock')?.data?.wire], ['clock', 'o:beat', 'trigger']);
  assert.equal(normal('seq.reset')?.sourceHandle, 'o:bar');
  // A cable plugged into the Loudness's input breaks its normal; the others stay.
  const plugged = graphOf({ patch: wired([{ id: 'x', from: { signal: 'level', band: 0 }, to: { module: 'env', knob: 'in' }, value: 1, mode: 'swing' }]), types: typed });
  assert.equal(edgeOf(plugged.edges, 'normal:env.in'), undefined);
  assert.ok(edgeOf(plugged.edges, 'x') && edgeOf(plugged.edges, 'normal:waves.audio'));
  // A normal from a module not drawn is not drawn.
  const alone = graphOf({ patch: { ...wired([]), modulators: [mod('seq', 'sequencer')] }, types: typed });
  assert.equal(edgeOf(alone.edges, 'normal:seq.clock'), undefined);
});

test('a band’s six at once is one line marked "· 6"; one band is one channel', () => {
  assert.equal(bundleLabel(6), '· 6');
  assert.equal(bundleLabel(1), '');
  assert.equal(bundleLabel(undefined), '');
  assert.equal(cableLabel({ amount: '50%', channels: 6 }, false), '· 6');
  assert.equal(cableLabel({ amount: '50%', channels: 6 }, true), '50% · 6');
  assert.equal(cableLabel({ amount: '50%' }, true), '50%');
  assert.equal(cableLabel({ amount: '50%' }, false), '');
  assert.deepEqual(sourcePort({ signal: 'hits' }, () => undefined), { type: 'trigger', channels: 6 });
  assert.deepEqual(sourcePort({ signal: 'hits', band: 2 }, () => undefined), { type: 'trigger' });
  assert.deepEqual(sourcePort({ signal: 'loudness' }, () => undefined), { type: 'value' });
  const { edges, nodes } = graphOf({
    patch: wired([
      { id: 'all', from: { signal: 'hits' }, to: { module: 'adsr', knob: 'gate' }, value: 1, mode: 'swing' },
      { id: 'one', from: { signal: 'level', band: 3 }, to: { module: 'waves', knob: 'hue' }, value: 0.2, mode: 'swing' },
      { id: 'poly', from: { modulator: 'adsr' }, to: { module: 'waves', knob: 'width' }, value: 0.2, mode: 'level' },
    ]),
    types: typed,
  });
  assert.equal(edges.filter((e) => e.id === 'all').length, 1, 'one line');
  assert.equal(edgeOf(edges, 'all')?.data?.channels, 6);
  assert.equal(edgeOf(edges, 'one')?.data?.channels, undefined);
  assert.equal(edgeOf(edges, 'poly')?.data?.channels, 6);
  // Fed six channels, the ADSR's face has a lane each.
  assert.equal((nodes.find((n) => n.id === 'adsr')!.data as ModuleData).face?.lanes, 6);
  const mono = graphOf({ patch: wired([{ id: 'b', from: { signal: 'beat' }, to: { module: 'adsr', knob: 'gate' }, value: 1, mode: 'swing' }]), types: typed });
  assert.equal((mono.nodes.find((n) => n.id === 'adsr')!.data as ModuleData).face?.lanes, 1);
  const monoOut = graphOf({ patch: wired([{ id: 'b', from: { signal: 'beat' }, to: { module: 'adsr', knob: 'gate' }, value: 1, mode: 'swing' }, { id: 'o', from: { modulator: 'adsr' }, to: { module: 'waves', knob: 'width' }, value: 0.2, mode: 'level' }]), types: typed });
  assert.equal(edgeOf(monoOut.edges, 'o')?.data?.channels, undefined, 'one gate in, one envelope out');
});

test("faces: an ADSR's stage from its output, an LFO's phase, the Clock's lights", () => {
  const k = { attack: 0.1, decay: 0.2, sustain: 0.5, release: 0.3, amount: 1 };
  let m = faces.newAdsrMemo();
  const seen: string[] = [];
  for (const out of [0, 0.3, 0.8, 1, 0.8, 0.6, 0.5, 0.5, 0.5, 0.4, 0.2, 0.05, 0]) {
    m = faces.adsrStep(m, k, out);
    if (seen.at(-1) !== m.stage) seen.push(m.stage);
  }
  assert.deepEqual(seen, ['idle', 'attack', 'decay', 'sustain', 'release', 'idle']);
  // The dot: along the attack by the level, the release by how far from where it began.
  const spans = faces.adsrSpans(k, 100);
  assert.ok(spans.attack[0] === 0 && Math.abs(spans.release[1] - 100) < 1e-9 && spans.decay[0] === spans.attack[1]);
  assert.equal(faces.adsrDot({ stage: 'attack', level: 0.5, from: 0 }, k, 100, 10)!.x, (spans.attack[0] + spans.attack[1]) / 2);
  assert.equal(faces.adsrDot({ stage: 'release', level: 0.25, from: 0.5 }, k, 100, 10)!.x, (spans.release[0] + spans.release[1]) / 2);
  assert.equal(faces.adsrDot({ stage: 'idle', level: 0, from: 0 }, k, 100, 10), null);
  assert.equal(faces.adsrStagePath(k, 'attack', 100, 10), `M0,10 L${Math.round(spans.attack[1] * 10) / 10},0`);
  // A channel's output by `module.port.n`, else `module.port`, else (its out) the module's own.
  assert.equal(faces.outputOf({ 'a.out.2': 0.4, 'a.out': 0.1 }, 'a', 'out', 2), 0.4);
  assert.equal(faces.outputOf({ a: 0.7 }, 'a'), 0.7);
  assert.equal(faces.outputOf({}, 'a'), null);
  // The LFO's phase from its value and its way: a sine's and a triangle's exactly, a mix of them near enough.
  for (const shape of [0, 1, 0.4])
    for (const p of [0.05, 0.2, 0.4, 0.6, 0.8, 0.95]) {
      const v = faces.lfoWave(p, shape);
      const rising = p < 0.25 || p > 0.75;
      const tol = shape === 0 || shape === 1 ? 1e-9 : 0.06;
      assert.ok(Math.abs(faces.lfoPhase(v, rising, shape) - p) < tol, `${shape} ${p}`);
    }
  assert.equal(faces.lfoStep({ value: 0.2, rising: true }, 0.1).rising, false);
  // The Clock: the beat flashes and fades; a bar's first beat lights Bar; a phrase's, Phrase too.
  assert.deepEqual(faces.clockLights(null), { beat: 0, bar: 0, phrase: 0 });
  assert.deepEqual(faces.clockLights({ index: 32, since: 0, length: 0.5, strength: 1 }), { beat: 1, bar: 1, phrase: 1 });
  const third = faces.clockLights({ index: 6, since: 0.06, length: 0.5, strength: 1 });
  assert.ok(third.beat > 0 && third.beat < 1 && third.bar === 0 && third.phrase === 0);
  assert.equal(faces.clockLights({ index: 4, since: 0, length: 0.5, strength: 1 }).phrase, 0);
});

test('"New ▸ ADSR" from a knob: the module, the cable into the knob and its gate from the beat, in one change', () => {
  const before = { routes: {}, extra: { layers: [], loop: [], modulators: [mod('lfo', 'lfo')], cables: [{ id: 'cable-1', from: { modulator: 'lfo' }, to: { module: 's1', knob: 'a' }, value: 0.5, mode: 'swing' as const }], macros: [] } };
  const made = withModulator(before, { id: 'adsr-1', type: 'adsr', knobs: { attack: 0.01 } }, ADSR, { module: 's1', knob: 'b' }, 'level');
  assert.deepEqual(made.cables.extra.modulators.map((m) => m.id), ['lfo', 'adsr-1']);
  const [sync, into] = made.cables.extra.cables.slice(1);
  assert.deepEqual([sync.from, sync.to, sync.value], [CLOCK_BEAT, { module: 'adsr-1', knob: 'gate' }, 1]);
  assert.deepEqual([into.from, into.to, into.mode], [{ modulator: 'adsr-1' }, { module: 's1', knob: 'b' }, 'level']);
  assert.equal(made.cable, into);
  assert.equal(new Set(made.cables.extra.cables.map((c) => c.id)).size, 3, 'ids free');
  assert.equal(before.extra.cables.length, 1, 'the cables before, untouched');
  // The clocked LFO: its clock from the beat; its reset normalled to the Clock's bar, left so.
  const lfo = withModulator(before, { id: 'lfo-2', type: 'lfo', knobs: {} }, MODULATORS.lfo, { module: 's1', knob: 'b' }, 'swing').cables.extra.cables.slice(1);
  assert.deepEqual(lfo.map((c) => [c.from, c.to]), [
    [CLOCK_BEAT, { module: 'lfo-2', knob: 'clock' }],
    [{ modulator: 'lfo-2' }, { module: 's1', knob: 'b' }],
  ]);
  // A port normalled from the Clock stays so (a Sequencer's clock and reset).
  assert.equal(withModulator(before, { id: 'seq-1', type: 'sequencer', knobs: {} }, SEQUENCER, { module: 's1', knob: 'b' }, 'swing').cables.extra.cables.length, 2);
});

test("Waves takes the sound by its audio port, normalled from Music's; the frame's samples where nothing passes it", () => {
  const audio = WAVES.ports.in.find((p) => p.id === 'audio')!;
  assert.deepEqual([audio.type, audio.normal], ['audio', { module: 'music', port: 'audio' }]);
  const wave = Float32Array.from([0, 0.5, -0.5]);
  const other = Float32Array.from([1, -1]);
  const frame = { dt: 0, time: 0, beat: null, signals: () => 0, waveform: wave };
  assert.equal(withInputs('waves', frame).inputs?.audio, wave);
  assert.equal(withInputs('shapes', frame), frame, 'a source with no audio port: the frame as it is');
  assert.equal(soundOf({ waveform: wave, inputs: { audio: other } }), other);
  assert.equal(soundOf({ waveform: wave, inputs: { audio: null } }), wave);
  assert.equal(soundOf({ waveform: wave }), wave);
  assert.equal(soundOf({ waveform: null }), null);
});

// ---- Points, polyphonic knobs, the grid modules (0057 §4b, §4c) ------------------------------------------------------

const { polyTurned } = await import('../src/lib/visuals/patchgraph.ts');
const { GRID, FOLD, INSTANCE } = await import('../src/lib/visuals/ports.ts');

test('points: a chain line (dash, dot); a Grid is an ordinary node with its jacks, its points into a Fold only', () => {
  const [dash, gap, dot] = WIRES.points.dash!.split(' ').map(Number);
  assert.ok(dash > 3 && dot < 0.5 && gap > 0, 'a dash and a dot');
  assert.ok(new Set(Object.values(WIRES).map((w) => `${w.dash}|${w.width}|${w.double}`)).size === Object.keys(WIRES).length, 'each type its own look');
  const t = { ...typed, grid: GRID, fold: FOLD, instance: INSTANCE };
  const p: Patch = {
    ...wired([{ id: 'g', from: { modulator: 'grid' }, to: { module: 'fold', knob: 'grid' }, value: 1, mode: 'swing' }], [mod('grid', 'grid')]),
    layers: [{ id: 'w', source: mod('waves', 'waves'), modules: [mod('fold', 'fold'), mod('inst', 'instance')], loop: [], blend: { mode: 'add', amount: 1 } }],
  };
  const { nodes, edges } = graphOf({ patch: p, types: t });
  const d = (id: string) => nodes.find((n) => n.id === id)!.data as ModuleData;
  assert.deepEqual(d('grid').gives, { type: 'points' });
  assert.deepEqual(d('grid').outs.map((o) => o.id), ['points', 'index', 'distance', 'angle', 'row', 'column']);
  assert.deepEqual(d('fold').ports.map((p) => [p.id, p.type]), [['grid', 'points']]);
  assert.deepEqual(d('inst').ports.map((p) => [p.id, p.type]), [['points', 'points']]);
  assert.equal(edgeOf(edges, 'g')?.data?.wire, 'points');
  assert.equal(cableOf({ source: 'grid', target: 'waves', sourceHandle: 'mod', targetHandle: 'k:hue' }, nodes, p.cables), null, 'points into a value: no');
  assert.ok(cableOf({ source: 'grid', target: 'inst', sourceHandle: 'mod', targetHandle: 'k:points' }, nodes, p.cables));
});

test("a band's knobs are one polyphonic knob, its six as ticks, opened out to six; turned as one they keep apart", () => {
  const knob6 = (id: string) => Array.from({ length: 6 }, (_, b) => ({ id: `${id}.${b}`, label: `Linger ${b + 1}`, min: 0, max: 1, neutral: 0, default: 0.5, kind: 'level' as const }));
  const t = { ...types, src: { ...types.src, knobs: [...knob6('lighting'), knob('a')] } };
  const s1 = { id: 's1', type: 'src', knobs: { 'lighting.0': 0.2, 'lighting.1': 0.4, 'lighting.2': 0.6, 'lighting.3': 0.8, 'lighting.4': 0.5, 'lighting.5': 0.5 } };
  const p = (cables: Patch['cables']): Patch => ({ ...sample(), layers: [{ id: 'one', source: s1, modules: [], loop: [], blend: { mode: 'add', amount: 1 } }], cables });
  const into2: Patch['cables'] = [{ id: 'b2', from: { signal: 'level', band: 2 }, to: { module: 's1', knob: 'lighting.2' }, value: 0.5, mode: 'swing' }];
  const { nodes, edges } = graphOf({ patch: p(into2), types: t });
  const d = nodes.find((n) => n.id === 's1')!.data as ModuleData;
  const poly = d.knobs.find((k) => k.id === 'lighting')!;
  assert.equal(poly.label, 'Linger');
  assert.deepEqual(poly.channels!.map((c) => c.id), ['lighting.0', 'lighting.1', 'lighting.2', 'lighting.3', 'lighting.4', 'lighting.5']);
  assert.ok(Math.abs(poly.value - 0.5) < 1e-9, 'its mean');
  assert.deepEqual(poly.channels!.map((c) => c.at), [0.2, 0.4, 0.6, 0.8, 0.5, 0.5]);
  assert.ok(!d.knobs.some((k) => k.id.startsWith('lighting.')), 'no channel knob drawn apart');
  assert.ok(!d.ports.some((x) => x.id === 'lighting'), 'nor as a port');
  assert.ok(poly.ring, 'a cable into one channel rings it');
  assert.equal(edgeOf(edges, 'b2')?.targetHandle, 'k:lighting', 'onto the one knob');
  // Opened out: six knobs, the cable onto its own channel's.
  const open = graphOf({ patch: p(into2), types: t, open: new Set(['s1.lighting']) });
  const o = open.nodes.find((n) => n.id === 's1')!.data as ModuleData;
  assert.deepEqual(o.knobs.filter((k) => k.id.startsWith('lighting')).map((k) => [k.id, k.label]).slice(0, 2), [['lighting.0', 'Linger 1'], ['lighting.1', 'Linger 2']]);
  assert.equal(o.knobs.filter((k) => k.id.startsWith('lighting.')).length, 6);
  assert.equal(edgeOf(open.edges, 'b2')?.targetHandle, 'k:lighting.2');
  // Turned as one: each channel as far, clamped.
  const turnedTo = polyTurned(poly, 0.7);
  assert.deepEqual(turnedTo.map((c) => c.id), poly.channels!.map((c) => c.id));
  assert.deepEqual(turnedTo.map((c) => Math.round(c.value * 100) / 100), [0.4, 0.6, 0.8, 1, 0.7, 0.7]);
});

// ---- A v2 patch drawn: devices as nodes, opened in place, entered; edits for the store (0057 §6, §7) ------------------

const await_graph = await import('../src/lib/visuals/patchgraph.ts');
const import_device = await import('../src/lib/visuals/device-make.ts');
const { graphOfV2, applyV2, editKey, connectionV2, patchAt, crumbsOf, portsOfModule, handleOf, macroKnob } = await import('../src/lib/visuals/patchgraph.ts');
const { toV2 } = await import('../src/lib/visuals/v2.ts');
const { newStore, change, undo, redo } = await import('../src/lib/visuals/patch-store.ts');
const { ENGINE_MODULES, GRAPH_MODULES, showPartTypes, SHOW_PARTS } = await import('../src/lib/visuals/ports.ts');
const { EXTRA_TYPES } = await import('../src/lib/visuals/registry.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;
type Device = import('../src/lib/visuals/ports.ts').Device;

const chainTypes = cratarrTypes(RANGES, DEFAULTS);
const v2types: Record<string, ModuleType> = Object.fromEntries(
  [...chainTypes, ...EXTRA_TYPES, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chainTypes.find((t: ModuleType) => t.type === 'show')!)].map((t: ModuleType) => [t.type, t]),
);
function v2patch(): PatchV2 {
  const p = toV2(DEFAULTS) as PatchV2;
  p.cables.push({ id: 'lab-1', from: { module: 'flame', port: 'light' }, to: { module: 'show', port: 'light' }, value: 1, mode: 'swing' });
  return p;
}
const md = (g: { nodes: { id: string; data: unknown }[] }, id: string) => g.nodes.find((n) => n.id === id)?.data as ModuleData | undefined;

test('v2: Show is one device node, its exposed light in, picture out and knobs; the flame its source', () => {
  const g = graphOfV2({ patch: v2patch(), types: v2types });
  const show = md(g, 'show')!;
  assert.ok(show.device && !show.device.open);
  assert.equal(show.label, 'Show');
  assert.deepEqual([show.v2!.pin, show.v2!.pout], ['light', 'picture']);
  assert.deepEqual(show.knobs.map((k) => k.id), [...SHOW_PARTS.filter((x) => x.knob !== 'size').map((x) => x.knob), 'level']);
  assert.ok(!g.nodes.some((n) => n.id.startsWith('show/')), 'its modules not drawn while it is closed');
  const wire = g.edges.find((e) => e.id === 'lab-1')!;
  assert.deepEqual([wire.source, wire.sourceHandle, wire.target, wire.targetHandle, wire.data?.wire], ['flame', 'out', 'show', 'in', 'light']);
  // Lanes by category (rack.ts byCategory), each under its header: the flame in Sources, Show in Effects, the
  // modulators under them, In / Out last.
  const at = (id: string) => g.nodes.find((n) => n.id === id)!.position;
  const heads = g.nodes.filter((n) => n.data.kind === 'lane').map((n) => n.id);
  assert.deepEqual(heads, ['@lane:sources', '@lane:effects', '@lane:mixers', '@lane:modulators', '@lane:io']);
  assert.ok(at('@lane:sources').y < at('flame').y && at('flame').y < at('@lane:effects').y && at('@lane:effects').y < at('show').y);
  assert.ok(at('show').y < at('@lane:modulators').y && at('music').y > at('@lane:io').y);
  const control = g.nodes.filter((n) => (n.data as ModuleData).modulator);
  assert.ok(control.length > 3 && control.every((n) => n.position.y > at('show').y));
  const adsrs = g.nodes.filter((n) => (n.data as ModuleData).type === 'adsr');
  assert.ok(adsrs.length >= 1);
  const boxes = control.map((n) => ({ ...n.position, w: sizeOfTest(n.data as ModuleData).width, h: sizeOfTest(n.data as ModuleData).height }));
  for (let i = 0; i < boxes.length; i++)
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i];
      const b = boxes[j];
      assert.ok(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y, 'the control lane does not overlap');
    }
});
const { sizeOf: sizeOfTest } = await import('../src/lib/visuals/patchgraph.ts');

test('v2: a device opened in place is a frame round its modules and cables; a cable into it goes to the module behind', () => {
  const g = graphOfV2({ patch: v2patch(), types: v2types, open: new Set(['show']) });
  const box = g.nodes.find((n) => n.id === 'show')!;
  assert.equal(box.type, 'box');
  assert.equal((box.data as { device?: true }).device, true);
  const inner = g.nodes.filter((n) => n.parentId === 'show' && n.data.kind === 'module').map((n) => n.id);
  assert.deepEqual(inner, SHOW_PARTS.map((x) => `show/${x.type}`));
  assert.ok(!g.nodes.some((n) => n.parentId === 'show' && n.data.kind === 'lane'), 'no lane headers inside a frame');
  assert.ok(g.nodes.indexOf(box) < g.nodes.findIndex((n) => n.parentId === 'show'), 'the frame before its modules');
  for (const n of g.nodes.filter((x) => x.parentId === 'show')) assert.ok(n.position.x >= 0 && n.position.y >= 0);
  assert.deepEqual(g.edges.filter((e) => e.id.startsWith('show/')).length, SHOW_PARTS.length - 1);
  assert.equal(g.edges.find((e) => e.id === 'show/show-1')?.data?.wire, 'picture');
  const wire = g.edges.find((e) => e.id === 'lab-1')!;
  assert.deepEqual([wire.target, wire.targetHandle], ['show/bend', 'in']);
  assert.ok(box.width! > 5 * 100 && box.height! > 60);
});

test('v2: entered, the device is the level shown, with its name in the breadcrumb; devices nest', () => {
  const p = v2patch();
  const g = graphOfV2({ patch: p, types: v2types, at: ['show'] });
  assert.deepEqual(g.nodes.filter((n) => n.data.kind === 'module').map((n) => n.id), SHOW_PARTS.map((x) => x.type));
  assert.deepEqual(crumbsOf(p, ['show']), ['Show']);
  assert.equal(patchAt(p, ['nothing']), null);
  // A device inside Show, opened in place inside Show opened in place.
  const show = p.modules.find((m) => m.id === 'show') as Device;
  const inner: Device = { id: 'pair', type: 'device', label: 'Pair', knobs: {}, patch: { v: 2, modules: [{ id: 'sat', type: 'saturate', knobs: { saturation: 0.3 } }], cables: [], macros: [], pins: [] }, inputs: [{ as: 'sat', label: 'Sat', module: 'sat', port: 'saturation' }], outputs: [{ as: 'picture', label: 'Picture', module: 'sat', port: 'picture' }], macros: [{ id: 'm', label: 'More', value: 0.25, targets: [] }] };
  show.patch.modules.push(inner);
  const nested = graphOfV2({ patch: p, types: v2types, open: new Set(['show', 'show/pair']) });
  assert.equal(nested.nodes.find((n) => n.id === 'show/pair')?.parentId, 'show');
  assert.equal(nested.nodes.find((n) => n.id === 'show/pair/sat')?.parentId, 'show/pair');
  const closed = graphOfV2({ patch: p, types: v2types, open: new Set(['show']) });
  const pair = md(closed, 'show/pair')!;
  assert.deepEqual(pair.knobs.map((k) => [k.id, k.value]), [['sat', 0.3], [macroKnob('m'), 0.25]]);
  assert.deepEqual(portsOfModule(v2types)(inner)!.out.map((x) => [x.id, x.type]), [['picture', 'picture']]);
});

test('v2 edits: a knob (a device’s exposed one to the knob behind it, a macro its value), a cable drawn, cables out', () => {
  const p = v2patch();
  applyV2(p, { kind: 'knob', module: ['show'], knob: 'glow', value: 0.9 });
  const show = p.modules.find((m) => m.id === 'show') as Device;
  assert.equal(show.patch.modules.find((m) => m.id === 'tonemap')!.knobs.glow, 0.9);
  applyV2(p, { kind: 'knob', module: ['show', 'blur'], knob: 'smoothing', value: 0.1 });
  assert.equal(show.patch.modules.find((m) => m.id === 'blur')!.knobs.smoothing, 0.1);
  show.macros.push({ id: 'x', label: 'X', value: 0, targets: [] });
  applyV2(p, { kind: 'knob', module: ['show'], knob: macroKnob('x'), value: 2 });
  assert.equal(show.macros[0].value, 1);
  const before = show.patch.cables.length;
  applyV2(p, { kind: 'connect', scope: ['show'], from: { module: 'bend', port: 'picture' }, to: { module: 'saturate', port: 'picture' } });
  assert.equal(show.patch.cables.length, before + 1);
  const made = show.patch.cables.at(-1)!;
  applyV2(p, { kind: 'remove', scope: ['show'], cables: [made.id] });
  assert.equal(show.patch.cables.length, before);
  assert.equal(editKey({ kind: 'knob', module: ['show'], knob: 'glow', value: 0 }), 'knob:show.glow');
  assert.equal(editKey({ kind: 'remove', scope: [], cables: [] }), null);
});

test('v2 through the store: each edit a step, a knob’s drag one, undone and redone', () => {
  let s = newStore(v2patch());
  const turn = (v: number, at: number) => (s = change(s, (p: PatchV2) => applyV2(p, { kind: 'knob', module: ['show'], knob: 'glow', value: v }), 'knob:show.glow', at));
  turn(0.2, 1000);
  turn(0.3, 1100);
  turn(0.4, 1200);
  assert.equal(s.past.length, 1, 'a drag, one step');
  s = change(s, (p: PatchV2) => applyV2(p, { kind: 'remove', scope: [], cables: ['lab-1'] }), null, 5000);
  assert.equal(s.past.length, 2);
  s = undo(s);
  assert.ok(s.now.cables.some((c) => c.id === 'lab-1'));
  s = undo(s);
  const glow = () => ((s.now.modules.find((m) => m.id === 'show') as Device).patch.modules.find((m) => m.id === 'tonemap')!.knobs.glow);
  assert.notEqual(glow(), 0.4);
  s = redo(s);
  assert.equal(glow(), 0.4);
});

test('v2 connections: at one level, by type, not twice, a picture cycle only through a Feedback', () => {
  const p = v2patch();
  const g = graphOfV2({ patch: p, types: v2types, open: new Set(['show']) });
  const inShow = connectionV2({ source: 'show/bend', sourceHandle: 'out', target: 'show/saturate', targetHandle: 'in' }, g.nodes, p, [], v2types);
  assert.deepEqual(inShow, { kind: 'connect', scope: ['show'], from: { module: 'bend', port: 'picture' }, to: { module: 'saturate', port: 'picture' } });
  assert.equal(connectionV2({ source: 'flame', sourceHandle: 'out', target: 'show/blur', targetHandle: 'in' }, g.nodes, p, [], v2types), null, 'across a frame: no');
  assert.equal(connectionV2({ source: 'show/saturate', sourceHandle: 'out', target: 'show/bend', targetHandle: 'in' }, g.nodes, p, [], v2types), null, 'a picture cycle with no Feedback: no');
  const music = md(g, 'music')!;
  const clock = md(g, 'clock')!;
  const beat = handleOf(music, 'out', 'beat');
  assert.ok(connectionV2({ source: 'music', sourceHandle: beat, target: 'show/blur', targetHandle: 'k:smoothing' }, g.nodes, p, [], v2types) === null, 'across a frame');
  assert.ok(connectionV2({ source: 'clock', sourceHandle: handleOf(clock, 'out', 'phase'), target: 'flame', targetHandle: 'k:twist' }, g.nodes, p, [], v2types));
  assert.equal(connectionV2({ source: 'flame', sourceHandle: 'out', target: 'clock', targetHandle: 'k:speed' }, g.nodes, p, [], v2types), null, 'light into a value: no');
  // Entered, the scope is the device's.
  const entered = graphOfV2({ patch: p, types: v2types, at: ['show'] });
  assert.deepEqual(connectionV2({ source: 'bend', sourceHandle: 'out', target: 'saturate', targetHandle: 'in' }, entered.nodes, patchAt(p, ['show'])!, ['show'], v2types)?.scope, ['show']);
});

test('faces: a Random’s recent output, a line while smooth, steps once stepped; an ADSR lane its channel’s knobs', () => {
  let seen: number[] = [];
  for (const v of [0, 0.5, -0.5]) seen = faces.randomStep(seen, v, 0.5);
  assert.deepEqual(seen, [0, 1, -1]);
  assert.equal(faces.randomStep(Array(faces.RANDOM_SAMPLES).fill(0), 1).length, faces.RANDOM_SAMPLES);
  assert.ok(faces.randomPath(seen, 0, 100, 20).includes('L') && !faces.randomPath(seen, 0, 100, 20).includes('H'));
  assert.ok(faces.randomPath(seen, 1, 100, 20).includes('H') && faces.randomPath(seen, 1, 100, 20).includes('V'));
  assert.equal(faces.randomPath([], 0, 100, 20), '');
  assert.deepEqual(faces.laneKnobs({ 'release.2': 0.9, release: 0.3, attack: 0.1 }, 2), { attack: 0.1, decay: 0, sustain: 0, release: 0.9, amount: 1 });
  assert.equal(faces.FACES.random, 'random');
});

test('v2: a cable into a port its module does not declare, on a port row of its own (none hidden); one into a band of a polyphonic knob, onto it', () => {
  const p = v2patch();
  p.cables.push({ id: 'z', from: { module: 'clock', port: 'phase' }, to: { module: 'flame', port: 'nowhere' }, value: 1, mode: 'swing' });
  p.cables.push({ id: 'r', from: { module: 'clock', port: 'phase' }, to: { module: 'adsr', port: 'release.3' }, value: 1, mode: 'swing' });
  const g = graphOfV2({ patch: p, types: v2types });
  assert.equal(g.edges.find((e) => e.id === 'z')?.targetHandle, 'k:nowhere');
  assert.ok((g.nodes.find((n) => n.id === 'flame')!.data as ModuleData).ports.some((x) => x.id === 'nowhere'));
  const adsr = g.nodes.find((n) => (n.data as ModuleData).type === 'adsr');
  if (adsr) assert.equal(g.edges.find((e) => e.id === 'r')?.targetHandle, adsr.id === 'adsr' ? 'k:release' : undefined);
});

test('v2 categories: a whole one pinned through the store, a module added by its lane’s "+"', () => {
  const { typesOf, categoryPin } = await_graph;
  let st = newStore(v2patch());
  st = change(st, (p: PatchV2) => applyV2(p, { kind: 'pin', scope: [], category: 'effects', on: true }), null, 1);
  assert.ok(st.now.pins.includes(categoryPin('effects')));
  const g = graphOfV2({ patch: st.now, types: v2types });
  assert.equal((g.nodes.find((n) => n.id === '@lane:effects')!.data as { pinned: boolean }).pinned, true);
  assert.equal((g.nodes.find((n) => n.id === '@lane:sources')!.data as { pinned: boolean }).pinned, false);
  st = change(st, (p: PatchV2) => applyV2(p, { kind: 'pin', scope: [], category: 'effects', on: false }), null, 2);
  assert.ok(!st.now.pins.includes(categoryPin('effects')));
  st = undo(st);
  assert.ok(st.now.pins.includes(categoryPin('effects')), 'undone, pinned again');
  // Pinned inside a device: that device's patch's pins.
  st = change(st, (p: PatchV2) => applyV2(p, { kind: 'pin', scope: ['show'], category: 'effects', on: true }), null, 3);
  assert.ok(patchAt(st.now, ['show'])!.pins.includes(categoryPin('effects')));
  // "+": the category's types; a new one with a free id.
  const effects = typesOf('effects', v2types).map((t: ModuleType) => t.type);
  assert.ok(effects.includes('fold') && !effects.includes('lfo') && !effects.includes('flame'));
  assert.ok(typesOf('modulators', v2types).some((t: ModuleType) => t.type === 'adsr'));
  st = change(st, (p: PatchV2) => applyV2(p, { kind: 'add', scope: [], type: 'fold' }), null, 4);
  st = change(st, (p: PatchV2) => applyV2(p, { kind: 'add', scope: [], type: 'fold' }), null, 5);
  assert.deepEqual(st.now.modules.filter((m) => m.type === 'fold').map((m) => m.id), ['fold-1', 'fold-2']);
});

test("a step knob's next is a trigger beside it: a trigger cabled into it, a value not; no port row of its own", () => {
  const { nextPort } = await_graph;
  const t = { ...types, fx: { ...types.fx, knobs: [...types.fx.knobs, { id: 'mode', label: 'Mode', min: 1, max: 4, neutral: 1, default: 1, kind: 'step' as const }] } };
  const p: Patch = { ...sample(), cables: [{ id: 'n', from: { signal: 'beat' }, to: { module: 'k1', knob: nextPort('mode') }, value: 1, mode: 'swing' }] };
  const g = graphOf({ patch: p, types: t, signals: [{ signal: 'level' }] });
  const d = g.nodes.find((n) => n.id === 'k1')!.data as ModuleData;
  assert.equal(d.knobs.find((k) => k.id === 'mode')?.next, true, 'shown, cabled');
  assert.ok(!d.ports.some((x) => x.id.endsWith(':next')));
  assert.equal(g.edges.find((e) => e.id === 'n')?.targetHandle, 'k:mode:next');
  const level = signalKey({ signal: 'level' });
  assert.equal(cableOf({ source: level, target: 'k1', sourceHandle: 'mod', targetHandle: 'k:mode:next' }, g.nodes, []), null, 'a value into a next: no');
  assert.ok(cableOf({ source: signalKey({ signal: 'beat' }), target: 'k1', sourceHandle: 'mod', targetHandle: 'k:turn' }, g.nodes, []));
});

test('faces: a VCA its gain as a bar; a Merge its channels as ticks into one bar; Flash is named Accent', () => {
  assert.equal(faces.vcaGain({ 'v.gain': 1 }, 'v', { gain: 2 }), 0.5);
  assert.equal(faces.vcaGain(null, 'v', { gain: 2 }), 1);
  assert.equal(faces.vcaGain({}, 'v', {}), 0);
  const m = faces.mergeOf({ 'm.in.0': 0.5, 'm.in.3': 2, m: 0.4 }, 'm');
  assert.deepEqual(m.ins, [0.5, null, null, 1, null, null]);
  assert.equal(m.out, 0.4);
  assert.deepEqual(faces.mergeOf(null, 'm').ins, Array(6).fill(null));
  assert.equal(faces.FACES.vca, 'vca');
  assert.equal(faces.FACES.merge, 'merge');
  assert.equal(labelOf({ type: 'flash', label: 'Flash' }), 'Accent');
});

test('Make device: the modules in, their own cables with them, the crossing ones led to its ports, macros chosen; undone', () => {
  const { makeDevice, unmakeDevice } = import_device;
  // flame → bend → blur → fx1, a clock cable into blur, a cable out of bend to a meter; blur's knob in a macro.
  const p: PatchV2 = {
    v: 2,
    modules: [
      { id: 'flame', type: 'flame', knobs: {} },
      { id: 'bend', type: 'bend', knobs: { waves: 0.5 } },
      { id: 'blur', type: 'blur', knobs: { smoothing: 0.25 } },
      { id: 'clock', type: 'clock', knobs: {} },
      { id: 'meter', type: 'meter', knobs: {} },
    ],
    cables: [
      { id: 'a', from: { module: 'flame', port: 'light' }, to: { module: 'bend', port: 'picture' }, value: 1, mode: 'swing' },
      { id: 'b', from: { module: 'bend', port: 'picture' }, to: { module: 'blur', port: 'picture' }, value: 1, mode: 'swing' },
      { id: 'c', from: { module: 'clock', port: 'phase' }, to: { module: 'blur', port: 'smoothing' }, value: 0.5, mode: 'swing' },
      { id: 'd', from: { module: 'blur', port: 'picture' }, to: { module: 'meter', port: 'picture' }, value: 1, mode: 'swing' },
      { id: 'e', from: { module: 'clock', port: 'phase' }, to: { module: 'bend', port: 'waves' }, value: 0.5, mode: 'swing' },
    ],
    macros: [{ id: 'm', on: 'frame', label: 'M', value: 0, targets: [{ module: 'bend', knob: 'waves', from: 0, to: 1 }] }],
    pins: ['blur.smoothing'],
  };
  let st = newStore(structuredClone(p));
  st = change(st, (q: PatchV2) => applyV2(q, { kind: 'make', scope: [], modules: ['bend', 'blur'], macros: [{ module: 'blur', knob: 'smoothing' }] }, v2types), null, 1);
  const d = st.now.modules.find((m) => m.type === 'device') as Device;
  assert.ok(d, 'a device');
  assert.deepEqual(st.now.modules.map((m) => m.id), ['flame', d.id, 'clock', 'meter'], 'where its first module was');
  assert.deepEqual(d.patch.modules.map((m) => m.id), ['bend', 'blur']);
  assert.deepEqual(d.patch.cables.map((c) => c.id), ['b']);
  assert.deepEqual(d.inputs.map((e) => [e.as, e.module, e.port]), [['picture', 'bend', 'picture'], ['smoothing', 'blur', 'smoothing'], ['waves', 'bend', 'waves']]);
  assert.deepEqual(d.outputs.map((e) => [e.as, e.module, e.port]), [['picture', 'blur', 'picture']]);
  const at = (id: string) => st.now.cables.find((c) => c.id === id)!;
  assert.deepEqual([at('a').to, at('c').to, at('d').from], [{ module: d.id, port: 'picture' }, { module: d.id, port: 'smoothing' }, { module: d.id, port: 'picture' }]);
  assert.equal(d.macros.length, 1);
  assert.equal(d.macros[0].targets[0].module, 'blur');
  assert.ok(Math.abs(d.macros[0].value - 0.25) < 1e-9, 'from where the knob is');
  assert.equal(st.now.macros[0].targets[0].module, `${d.id}/bend`, "the level's macro reaches in by the device's path");
  assert.deepEqual(st.now.pins, [`${d.id}/blur.smoothing`]);
  // Drawn: one node, its ports the crossing cables'.
  const g = graphOfV2({ patch: st.now, types: v2types });
  assert.equal(g.edges.find((e) => e.id === 'd')?.source, d.id);
  // Unmade: as it was (but for the macros it made, now the level's own).
  st = change(st, (q: PatchV2) => applyV2(q, { kind: 'unmake', scope: [], device: d.id }), null, 2);
  assert.deepEqual(st.now.modules.map((m) => m.id), ['flame', 'bend', 'blur', 'clock', 'meter']);
  const ends = (q: PatchV2) => q.cables.map((c) => `${c.from.module}.${c.from.port}>${c.to.module}.${c.to.port}`).sort();
  assert.deepEqual(ends(st.now), ends(p));
  assert.equal(st.now.macros[0].targets[0].module, 'bend');
  assert.equal(st.now.macros.length, 2);
  assert.deepEqual(st.now.pins, ['blur.smoothing']);
  // Undo, undo: the patch as it began.
  st = undo(undo(st));
  assert.deepEqual(st.now, p);
  // Fewer than two: nothing.
  const q = structuredClone(p);
  assert.equal(makeDevice(q, ['bend']), null);
  assert.equal(unmakeDevice(q, 'bend'), false);
});

test('Make device inside a device, and unmade there: ids taken at the level get a number', () => {
  const p = v2patch();
  const show = () => patchAt(p, ['show'])!;
  applyV2(p, { kind: 'make', scope: ['show'], modules: ['curve', 'saturate'], macros: [], label: 'Finish' }, v2types);
  const inner = show().modules.find((m) => m.type === 'device') as Device;
  assert.equal(inner.label, 'Finish');
  assert.deepEqual(inner.patch.modules.map((m) => m.id), ['curve', 'saturate']);
  // Show's exposed knob for its saturation still reaches the knob (through the inner device's port).
  const g = graphOfV2({ patch: p, types: v2types, open: new Set(['show', `show/${inner.id}`]) });
  assert.ok(g.nodes.some((n) => n.id === `show/${inner.id}/saturate`));
  // A module of the same id at Show's level: the unmade one is renamed.
  show().modules.push({ id: 'curve', type: 'curve', knobs: {} });
  applyV2(p, { kind: 'unmake', scope: ['show'], device: inner.id });
  assert.ok(show().modules.some((m) => m.id === 'curve-2'));
});

test("a level knob's gain is a dot beside it, not a port row; its cables do not ring the knob", () => {
  const { gainPort } = await_graph;
  const p: Patch = { ...sample(), cables: [{ id: 'g', from: { modulator: 'lfo' }, to: { module: 'k1', knob: gainPort('amount') }, value: 1, mode: 'swing' }] };
  const g = graphOf({ patch: p, types });
  const d = g.nodes.find((n) => n.id === 'k1')!.data as ModuleData;
  const amount = d.knobs.find((k) => k.id === 'amount')!;
  assert.equal(amount.gain, true);
  assert.equal(amount.ring, null);
  assert.ok(!d.ports.some((x) => x.id.endsWith(':gain')));
  assert.equal(g.edges.find((e) => e.id === 'g')?.targetHandle, 'k:amount:gain');
});

test('portrait: the picture runs down (a layer, the blend, the loop, finish, the screen), the modulators in a column beside it', () => {
  const g = graphOf({ patch: sample(), types, portrait: true });
  const n = (id: string) => g.nodes.find((x) => x.id === id)!;
  const abs = (id: string) => {
    const x = n(id);
    const p = x.parentId ? n(x.parentId).position : { x: 0, y: 0 };
    return { x: p.x + x.position.x, y: p.y + x.position.y };
  };
  // Down a layer's box: its source over its module.
  assert.ok(abs('s1').y < abs('k1').y && abs('s1').x === abs('k1').x);
  assert.deepEqual([(n('s1').data as ModuleData).out, (n('k1').data as ModuleData).in], ['bottom', 'top']);
  // Then the blend, the loop, finish and the screen, each under the last.
  const order = ['@blend', '@loop', 'f1', 'f2', '@screen'].map((id) => n(id).position.y);
  assert.deepEqual([...order].sort((a, b) => a - b), order);
  assert.ok(order[0] > n(layerBox('two')).position.y);
  // The modulators in a column right of the picture's, from its top; the far labels room to the right of each.
  const column = Math.max(...['@blend', '@loop', '@screen', layerBox('one'), layerBox('two')].map((id) => n(id).position.x + (n(id).width ?? sizeOfTest(n(id).data).width)));
  assert.ok(n('lfo').position.x > column && n('fol').position.x === n('lfo').position.x);
  assert.ok(Math.min(n('lfo').position.y, n('fol').position.y) === 0);
  const room = (id: string) => (n(id).data as ModuleData).room!;
  assert.ok(room('k1') > sizeOfTest(n('k1').data).width, 'beside a picture module, room to the modulators');
  assert.ok(abs('k1').x + room('k1') <= n('lfo').position.x, 'never over the next');
  // A loop's way back leaves at the right.
  assert.equal((n('m2').data as ModuleData).fbSide, 'right');
});

test('a cable ends at its jack’s centre: half a jack in from the edge point on the jack’s side', () => {
  const { toJack, JACK } = await_graph;
  const r = JACK / 2;
  assert.deepEqual(toJack(100, 50, 'right'), { x: 100 - r, y: 50 });
  assert.deepEqual(toJack(100, 50, 'left'), { x: 100 + r, y: 50 });
  assert.deepEqual(toJack(100, 50, 'top'), { x: 100, y: 50 + r });
  assert.deepEqual(toJack(100, 50, 'bottom'), { x: 100, y: 50 - r });
  // Every edge drawn by our own components (picture wires and cables), over the nodes.
  const g = graphOf({ patch: sample(), types });
  assert.ok(g.edges.every((e) => e.type === 'wire' || e.type === 'cable'));
  assert.equal(g.edges.find((e) => e.id.startsWith('fb:'))?.data?.step, true);
});

test('far out every cable is on its nodes’ far sockets (in on the input side, out on the output side), bundled', () => {
  const { farEdges, FAR_IN, FAR_OUT } = await_graph;
  const g = graphOf({ patch: sample(), types });
  const far = farEdges(g.edges, g.nodes);
  assert.equal(far.length, g.edges.length);
  for (const e of far) {
    assert.equal(e.sourceHandle, FAR_OUT, e.id);
    assert.equal(e.targetHandle, FAR_IN, e.id);
  }
  // A frame (a device opened in place) has none: its edges as they are.
  const boxed = farEdges([{ id: 'x', source: layerBox('one'), target: 'k1', sourceHandle: 'out', targetHandle: 'in' }], g.nodes);
  assert.deepEqual([boxed[0].sourceHandle, boxed[0].targetHandle], ['out', FAR_IN]);
});

test('a node’s far form counts its cables in and out ("6 in · 12 out"), not its normals', () => {
  const { countLabel } = await_graph;
  assert.equal(countLabel({ in: 6, out: 12 }), '6 in · 12 out');
  assert.equal(countLabel({ in: 0, out: 3 }), '3 out');
  assert.equal(countLabel({ in: 2, out: 0 }), '2 in');
  assert.equal(countLabel({ in: 0, out: 0 }), '');
  const g = graphOfV2({ patch: v2patch(), types: v2types });
  const count = (id: string) => (g.nodes.find((n) => n.id === id)!.data as ModuleData).cables!;
  const music = count('music');
  assert.ok(music.out > 5, `Music: ${JSON.stringify(music)}`);
  assert.equal(music.out, g.edges.filter((e) => e.source === 'music' && !e.data?.normal).length);
  assert.equal(count('flame').in, g.edges.filter((e) => e.target === 'flame' && !e.data?.normal).length);
});

test('portrait with the view known: the layout takes the shape that fits it best', () => {
  const { extent, fitZoom } = await_graph;
  const phone = { width: 358, height: 420 };
  const fit = (g: { nodes: GraphNode[] }) => fitZoom(extent(g.nodes as never, (n: never) => sizeOfTest((n as { data: never }).data)), phone);
  // v1: no worse than one column beside the modulators.
  const plain = graphOf({ patch: sample(), types, portrait: true });
  const fitted = graphOf({ patch: sample(), types, portrait: true, view: phone });
  assert.ok(fit(fitted) >= fit(plain) - 1e-9, `${fit(fitted)} < ${fit(plain)}`);
  // v2: the lanes' width that fits best, at least as well as the fixed one.
  const fixed = graphOfV2({ patch: v2patch(), types: v2types, portrait: true });
  const best = graphOfV2({ patch: v2patch(), types: v2types, portrait: true, view: phone });
  assert.ok(fit(best) >= fit(fixed) - 1e-9, `${fit(best)} < ${fit(fixed)}`);
  // A node the viewer put somewhere stays.
  const kept = graphOf({ patch: sample(), types, portrait: true, view: phone, positions: { lfo: { x: 900, y: 900 } } });
  assert.deepEqual(kept.nodes.find((n) => n.id === 'lfo')!.position, { x: 900, y: 900 });
});

// ---- ELK's layout (elk-layout.ts), ELK in-process here (its bundled build) -------------------------------------------

const elkMod = await import('../src/lib/visuals/elk-layout.ts');
const ELKBundled = (await import('elkjs/lib/elk.bundled.js')).default;
const elkNow = new ELKBundled();
const sizeBy = (n: { id: string; data: unknown }) => sizeOfTest(n.data as never);

test('ELK’s graph: ports by the cables’ sockets, inputs west and outputs east (north and south in portrait), the categories as partitions, a device opened here a compound node', () => {
  const g = graphOfV2({ patch: v2patch(), types: v2types, open: new Set(['show']) });
  const e = elkMod.toElk(g.nodes, g.edges, sizeBy as never);
  assert.equal(e.layoutOptions!['elk.algorithm'], 'layered');
  assert.equal(e.layoutOptions!['elk.edgeRouting'], 'ORTHOGONAL');
  assert.equal(e.layoutOptions!['elk.hierarchyHandling'], 'INCLUDE_CHILDREN');
  const flame = e.children!.find((n) => n.id === 'flame')!;
  assert.equal(flame.layoutOptions!['elk.portConstraints'], 'FIXED_ORDER');
  // Partitions in the order signals flow: In (Music), Modulators, Sources, Effects, Mixers, Out.
  assert.equal(flame.layoutOptions!['elk.partitioning.partition'], '2', 'Sources after the Modulators');
  assert.equal(e.children!.find((n) => n.id === 'music')!.layoutOptions!['elk.partitioning.partition'], '0', 'Music in');
  const side = (n: typeof flame, h: string) => n.ports!.find((p) => p.id === `${n.id}::${h}`)?.layoutOptions!['elk.port.side'];
  assert.equal(side(flame, 'out'), 'EAST');
  assert.equal(side(flame, 'k:shape'), 'WEST');
  const show = e.children!.find((n) => n.id === 'show')!;
  assert.deepEqual(show.children!.map((c) => c.id), SHOW_PARTS.map((x) => `show/${x.type}`));
  assert.ok(!e.children!.some((n) => n.id.startsWith('@lane')), 'lane headers are not ELK’s');
  const p = elkMod.toElk(g.nodes, g.edges, sizeBy as never, { portrait: true });
  assert.equal(p.layoutOptions!['elk.direction'], 'DOWN');
  assert.equal(side(p.children!.find((n) => n.id === 'flame')!, 'out'), 'SOUTH');
  // What ELK depends on: not a knob's value.
  const turned = v2patch();
  applyV2(turned, { kind: 'knob', module: ['flame'], knob: 'hue', value: 0.9 });
  const k1 = elkMod.layoutKey(g.nodes, g.edges, sizeBy as never);
  const g2 = graphOfV2({ patch: turned, types: v2types, open: new Set(['show']) });
  assert.equal(elkMod.layoutKey(g2.nodes, g2.edges, sizeBy as never), k1);
});

test('ELK lays the v2 patch: the categories in order, the picture wires routed, the viewer’s places kept, lane heads over their lanes', async () => {
  const g = graphOfV2({ patch: v2patch(), types: v2types });
  const laid = await elkMod.elkLayout(elkNow, g.nodes, g.edges, sizeBy as never, (id: string) => id === 'clock');
  const at = (id: string) => laid.nodes.find((n) => n.id === id)!.position;
  // Partitions left to right as signals flow: Music, the modulators, the flame, its effects.
  assert.ok(at('music').x < at('envelope').x && at('envelope').x < at('flame').x && at('flame').x < at('show').x, JSON.stringify([at('music'), at('envelope'), at('flame'), at('show')]));
  // The clock was put somewhere by the viewer: where it was.
  assert.deepEqual(at('clock'), g.nodes.find((n) => n.id === 'clock')!.position);
  // A picture wire has its route; a lane's head is just above its first node.
  const wire = laid.edges.find((e) => e.id === 'lab-1')!;
  assert.ok(wire.data?.route && wire.data.route.length >= 2);
  const head = at('@lane:sources');
  assert.ok(head.y < at('flame').y && head.x <= at('flame').x);
  // Every node has a place.
  assert.ok(laid.nodes.every((n) => Number.isFinite(n.position.x) && Number.isFinite(n.position.y)));
});

test('ELK with Show open here (a compound node) and in portrait (down)', async () => {
  const g = graphOfV2({ patch: v2patch(), types: v2types, open: new Set(['show']), portrait: true });
  const laid = await elkMod.elkLayout(elkNow, g.nodes, g.edges, sizeBy as never, () => false, { portrait: true });
  const n = (id: string) => laid.nodes.find((x) => x.id === id)!;
  assert.ok(n('show').width! > 0 && n('show').height! > 0, 'the frame sized by ELK');
  assert.ok(n('show/bend').position.y < n('show/saturate').position.y, 'down inside the frame too');
  assert.ok(n('flame').position.y < n('show').position.y, 'Sources over Effects');
});

test('a route drawn square from jack to jack', () => {
  const { routePath } = await_graph;
  // ELK's port a pixel off the jack: its first bend slid onto the jack's line, no needless step.
  const [path] = routePath({ x: 0, y: 0 }, [{ x: 0, y: 1 }, { x: 50, y: 1 }, { x: 50, y: 40 }, { x: 99, y: 40 }], { x: 100, y: 40 });
  assert.equal(path, 'M0,0 L50,0 L50,40 L100,40');
  // No bends and not in line: one square corner (down first in portrait).
  const [down] = routePath({ x: 0, y: 0 }, [{ x: 0, y: 0 }, { x: 30, y: 50 }], { x: 30, y: 50 }, true);
  assert.equal(down, 'M0,0 L0,50 L30,50');
});

test('the graph opens at 100%, modules whole: centred where the patch fits, else at its start (panned, never shrunk)', () => {
  const { openView, FULL_ZOOM } = await_graph;
  assert.equal(FULL_ZOOM, 1);
  const pad = { top: 56, right: 16, bottom: 60, left: 16 };
  const view = { width: 1600, height: 1000 };
  // Small: centred at 100%.
  const small = openView({ x: 100, y: 50, width: 400, height: 300 }, view, pad);
  assert.equal(small.zoom, 1);
  assert.equal(small.x, 16 + (1568 - 400) / 2 - 100);
  assert.equal(small.y, 56 + (884 - 300) / 2 - 50);
  // Bigger than the view: at 100% still, its top-left at the view's (inside the padding).
  const big = openView({ x: -20, y: 10, width: 5000, height: 3000 }, view, pad);
  assert.deepEqual(big, { x: 16 + 20, y: 56 - 10, zoom: 1 });
  // A phone: wide patch, short one: across from its start, centred down.
  const phone = openView({ x: 0, y: 0, width: 1200, height: 200 }, { width: 390, height: 500 }, pad);
  assert.equal(phone.x, 16);
  assert.equal(phone.y, 56 + (384 - 200) / 2);
});

test('ELK: a device opened here has room round its modules, and modules room between them', () => {
  const g = graphOfV2({ patch: v2patch(), types: v2types, open: new Set(['show']) });
  const e = elkMod.toElk(g.nodes, g.edges, sizeBy as never);
  const show = e.children!.find((n) => n.id === 'show')!;
  assert.match(show.layoutOptions!['elk.padding'], /top=48/);
  assert.ok(Number(e.layoutOptions!['elk.spacing.nodeNode']) >= 48 && Number(e.layoutOptions!['elk.layered.spacing.nodeNodeBetweenLayers']) >= 96);
});

test('too big for the view, the graph opens at its first module (the leftmost; the topmost in portrait)', () => {
  const { openView, firstModule } = await_graph;
  const pad = { top: 56, right: 16, bottom: 60, left: 16 };
  const nodes = [
    { position: { x: 0, y: 0 }, data: { kind: 'lane' } },
    { position: { x: 400, y: 30 }, data: { kind: 'module' } },
    { position: { x: 900, y: 600 }, data: { kind: 'module' } },
    { position: { x: 380, y: 500 }, data: { kind: 'module' } },
  ] as never[];
  assert.deepEqual(firstModule(nodes), { x: 380, y: 500 });
  assert.deepEqual(firstModule(nodes, true), { x: 400, y: 30 });
  const v = openView({ x: 0, y: 0, width: 3000, height: 3000 }, { width: 390, height: 500 }, pad, 1, firstModule(nodes, true));
  assert.deepEqual(v, { x: 16 - 400, y: 56 - 30, zoom: 1 });
});
