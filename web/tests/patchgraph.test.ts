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
  assert.deepEqual(o('lfo').knobs.map((k) => k.id), ['rate', 'shape', 'offset', 'amount']);
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
  // An LFO declares no trigger input: the module and its cable only. Ports normalled from the Clock stay so.
  assert.equal(withModulator(before, { id: 'lfo-2', type: 'lfo', knobs: {} }, MODULATORS.lfo, { module: 's1', knob: 'b' }, 'swing').cables.extra.cables.length, 2);
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
