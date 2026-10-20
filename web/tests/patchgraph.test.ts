// The graph builder's mapping (decisions 0056 step 3, 0057 §6, §7; patchgraph.ts): a v2 patch as nodes and edges,
// every cable an edge between its ports' handles, wires by type, normals, bundles, the knobs shown and their rings,
// connections checked, a cable's end rerouted, modules removed, the cables' sag (after VCV Rack's): npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';

(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { DEFAULTS, RANGES } = await import('../src/lib/visuals/settings.svelte.ts');
const { cratarrTypes } = await import('../src/lib/visuals/patch.ts');
const { MODULATORS } = await import('../src/lib/visuals/modulators/modulators.ts');
const { amountLabel, drawnOpacity, labelOf, nearestEnd, nextOpacity, ringOf, saggingPath, slumpOf, turned } = await import('../src/lib/visuals/patchgraph.ts');
const { ADSR, CLOCK, ENVELOPE, MUSIC, SEQUENCER, FEEDBACK } = await import('../src/lib/visuals/ports.ts');
type ModuleType = import('../src/lib/visuals/patch.ts').ModuleType;
type Module = import('../src/lib/visuals/patch.ts').Module;
type ModuleData = import('../src/lib/visuals/patchgraph.ts').ModuleData;
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;
type CableV2 = import('../src/lib/visuals/ports.ts').CableV2;

const knob = (id: string, kind: 'level' | 'swing' | 'step' = 'level', device = false) => ({ id, label: id, min: 0, max: 1, neutral: 0, default: 0.5, kind, ...(device ? { device } : {}) });
const types: Record<string, ModuleType> = {
  frame: { type: 'frame', label: 'Whole frame', place: 'frame', knobs: [knob('intensity'), knob('quality', 'level', true)] },
  src: { type: 'src', label: 'Source', place: 'source', knobs: [knob('a'), knob('b'), knob('c'), knob('d'), knob('e')] },
  fx: { type: 'fx', label: 'Effect', place: 'module', knobs: [knob('amount'), knob('turn', 'swing')] },
  move: { type: 'move', label: 'Move', place: 'loop', knobs: [knob('amount')] },
  ...MODULATORS,
  music: MUSIC,
  feedback: FEEDBACK,
};
const mod = (id: string, type: string, knobs: Record<string, number> = {}): Module => ({ id, type, knobs });
let ids = 0;
/** A cable `module.port` → `module.port`. */
const cab = (from: string, to: string, value = 1, channel?: number, id = `c${++ids}`): CableV2 => {
  const [fm, fp] = from.split('.');
  const [tm, ...tp] = to.split('.');
  return { id, from: { module: fm, port: fp, ...(channel === undefined ? {} : { channel }) }, to: { module: tm, port: tp.join('.') }, value };
};

/** Two sources, an effect, two loop modules, two finishes, an LFO, a follower on a band, Music. */
function sample(cables?: CableV2[]): PatchV2 {
  return {
    v: 2,
    modules: [mod('s1', 'src'), mod('k1', 'fx'), mod('s2', 'src'), mod('m1', 'move'), mod('l1', 'move'), mod('f1', 'fx'), mod('lfo', 'lfo'), mod('fol', 'fast'), mod('music', 'music')],
    cables: cables ?? [
      cab('s1.picture', 'k1.picture', 1, undefined, 'p1'),
      cab('lfo.out', 'k1.turn', 0.4, undefined, 'c1'),
      cab('music.level', 'fol.in', 1, 0, 'c2'),
      cab('fol.out', 's1.e', 0.5, undefined, 'c3'),
      cab('music.beat', 'gone.x', 1, undefined, 'c4'),
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

const { graphOfV2, applyV2, connectionV2, rerouteV2, handleOf, WIRES, MUTED_DASH, wireStyle, bundleLabel, cableLabel, withModulator, CLOCK_BEAT, polyTurned } = await import('../src/lib/visuals/patchgraph.ts');
const { WAVES, soundOf } = await import('../src/lib/visuals/sources/waves.ts');
const { withInputs } = await import('../src/lib/visuals/compose.ts');
const faces = await import('../src/lib/visuals/faces.ts');
const lab = await import('../src/lib/visuals/graph-lab.ts');
type GraphEdge = import('../src/lib/visuals/patchgraph.ts').GraphEdge;
const edgeOf = (edges: GraphEdge[], id: string) => edges.find((e) => e.id === id);
const dataOf = (g: { nodes: { id: string; data: unknown }[] }, id: string) => g.nodes.find((n) => n.id === id)!.data as ModuleData;
const labTypes = lab.labTypes();

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

test('every cable an edge between its ports’ handles, deletable; a picture one a wire, the rest cables; one to nothing left out', () => {
  const p = freeze(sample());
  const { nodes, edges } = graphOfV2({ patch: p, types });
  assert.deepEqual(
    edges.map((e) => [e.id, e.source, e.sourceHandle, e.target, e.targetHandle, e.type]),
    [
      ['p1', 's1', 'out', 'k1', 'in', 'wire'],
      ['c1', 'lfo', 'mod', 'k1', 'k:turn', 'cable'],
      ['c2', 'music', 'o:level', 'fol', 'k:in', 'cable'],
      ['c3', 'fol', 'mod', 's1', 'k:e', 'cable'],
    ],
  );
  assert.ok(edges.every((e) => e.deletable && e.selectable));
  assert.equal(edgeOf(edges, 'c1')?.data?.amount, '+40%');
  assert.equal(edgeOf(edges, 'p1')?.class, 'picture');
  assert.equal(edgeOf(edges, 'p1')?.data?.step, true);
  assert.equal(nodes.filter((n) => n.data.kind === 'module').length, p.modules.length, 'every module a node, once');
});

test('knobs: a few shown, the cabled ones always, never the device’s; opened out, all; rings from the cables', () => {
  const g = graphOfV2({ patch: sample(), types });
  const s1 = dataOf(g, 's1');
  assert.deepEqual(s1.knobs.map((k) => k.id), ['a', 'b', 'c', 'e']);
  assert.equal(s1.hidden, 1);
  assert.deepEqual(s1.knobs.find((k) => k.id === 'e')!.ring, [0.5, 1]);
  assert.equal(s1.knobs.find((k) => k.id === 'a')!.ring, null);
  const open = graphOfV2({ patch: sample(), types, open: new Set(['s1']) });
  assert.deepEqual(dataOf(open, 's1').knobs.map((k) => k.id), ['a', 'b', 'c', 'd', 'e']);
  assert.equal(dataOf(open, 's1').hidden, 0);
});

test('positions given are kept, by node id at the level shown', () => {
  const g = graphOfV2({ patch: sample(), types, positions: { k1: { x: 5, y: 7 } } });
  assert.deepEqual(g.nodes.find((n) => n.id === 'k1')!.position, { x: 5, y: 7 });
});

test('turning: clamped to the range, a step by whole steps', () => {
  const k = { min: 0, max: 1, kind: 'level' as const };
  assert.equal(turned(k, 0.5, 80), 1);
  assert.equal(turned(k, 0.5, -16), 0.4);
  assert.equal(turned({ min: 1, max: 9, kind: 'step' as const }, 3, 31), 5);
});

test('every output labelled, down the right: a modulator’s Out its `mod` handle, the rest `o:<port>`', () => {
  const g = graphOfV2({ patch: sample(), types: { ...types, clock: CLOCK } });
  assert.deepEqual(dataOf(g, 'lfo').outs, [{ id: 'out', label: 'Out', type: 'value', handle: 'mod' }]);
  const clock = graphOfV2({ patch: { ...sample(), modules: [mod('clock', 'clock')] }, types: { ...types, clock: CLOCK } });
  // Each by its declared label (ports.ts).
  assert.deepEqual(
    dataOf(clock, 'clock').outs.map((o) => [o.id, o.label, o.handle]),
    CLOCK.ports.out.map((o, i) => [o.id, o.label, i ? `o:${o.id}` : 'mod']),
  );
  assert.deepEqual(CLOCK.ports.out.map((o) => o.id), ['phase', 'rate', 'beat', 'bar', 'phrase']);
  // A picture module's picture output is its own (its `out` handle), not in the list.
  assert.deepEqual(dataOf(g, 'k1').outs, []);
  assert.ok(g.nodes.every((n) => n.data.kind !== 'module' || (n.data as ModuleData).outs.every((o) => o.label)));
});

test('rerouting: the knob end onto another knob, the source end onto another output; checked; nothing moved, nothing', () => {
  const p = sample();
  const g = graphOfV2({ patch: p, types });
  const r = rerouteV2('c1', { source: 'lfo', sourceHandle: 'mod', target: 'k1', targetHandle: 'k:amount' }, g.nodes, p, [], types);
  assert.deepEqual(r, { kind: 'reroute', scope: [], id: 'c1', end: 'to', to: { module: 'k1', port: 'amount' } });
  const from = rerouteV2('c1', { source: 'fol', sourceHandle: 'mod', target: 'k1', targetHandle: 'k:turn' }, g.nodes, p, [], types);
  assert.deepEqual([from?.end, from?.to], ['from', { module: 'fol', port: 'out' }]);
  assert.equal(rerouteV2('c1', { source: 'lfo', sourceHandle: 'mod', target: 'k1', targetHandle: 'k:turn' }, g.nodes, p, [], types), null, 'nothing moved');
  assert.equal(rerouteV2('c1', { source: 'lfo', sourceHandle: 'mod', target: 'k1', targetHandle: 'in' }, g.nodes, p, [], types), null, 'a value into a picture: no');
  // Applied: the same cable (its amount kept), its end moved; the unmoved end keeps its channel.
  const next = applyV2(structuredClone(p), r!, types);
  assert.deepEqual(next.cables.find((c) => c.id === 'c1'), { id: 'c1', from: { module: 'lfo', port: 'out' }, to: { module: 'k1', port: 'amount' }, value: 0.4 });
  const c2 = applyV2(structuredClone(p), { kind: 'reroute', scope: [], id: 'c2', end: 'to', to: { module: 'k1', port: 'turn' } }, types);
  assert.deepEqual(c2.cables.find((c) => c.id === 'c2')!.from, { module: 'music', port: 'level', channel: 0 });
});

test('modules removed: their cables, the depths read from them, their pins; a device’s ports onto them and the cables into those', () => {
  const p = sample();
  p.pins = ['k1.turn', 'cable:c1', 'lfo', 'module:lfo', 'lfo.rate', 'category:effects'];
  p.cables[2] = { ...p.cables[2], depth: { module: 'lfo', port: 'out' } };
  const next = applyV2(structuredClone(p), { kind: 'removeModules', scope: [], modules: ['lfo'] }, types);
  assert.ok(!next.modules.some((m) => m.id === 'lfo'));
  assert.ok(!next.cables.some((c) => c.id === 'c1'));
  assert.equal(next.cables.find((c) => c.id === 'c2')!.depth, null);
  assert.deepEqual(next.pins, ['k1.turn', 'category:effects']);
  // Inside a device: its port onto the module goes, and the cable outside into that port.
  const d: PatchV2 = {
    v: 2,
    modules: [
      mod('lfo', 'lfo'),
      { id: 'dev', type: 'device', label: 'Dev', knobs: {}, patch: { v: 2, modules: [mod('k1', 'fx'), mod('k2', 'fx')], cables: [], macros: [], pins: [] }, inputs: [{ module: 'k1', port: 'turn', as: 'turn', label: 'Turn' }, { module: 'k2', port: 'amount', as: 'amount', label: 'Amount' }], outputs: [], macros: [{ id: 'macro-1', label: 'M', value: 0.5, targets: [{ module: 'k1', knob: 'amount', from: 0, to: 1 }] }] } as Module,
    ],
    cables: [cab('lfo.out', 'dev.turn', 1, undefined, 'x1'), cab('lfo.out', 'dev.amount', 1, undefined, 'x2')],
    macros: [],
    pins: [],
  };
  const out = applyV2(structuredClone(d), { kind: 'removeModules', scope: ['dev'], modules: ['k1'] }, types);
  const dev = out.modules.find((m) => m.id === 'dev') as unknown as { inputs: { as: string }[]; patch: PatchV2; macros: { targets: unknown[] }[] };
  assert.deepEqual(dev.inputs.map((x) => x.as), ['amount']);
  assert.deepEqual(dev.patch.modules.map((m) => m.id), ['k2']);
  assert.equal(dev.macros[0].targets.length, 0);
  assert.deepEqual(out.cables.map((c) => c.id), ['x2']);
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
  assert.equal(nearestEnd({ x: 10, y: 0 }, { x: 0, y: 0 }, { x: 100, y: 0 }), 'from');
  assert.equal(nearestEnd({ x: 90, y: 0 }, { x: 0, y: 0 }, { x: 100, y: 0 }), 'to');
});

test('cable opacity: 100, 50, 20 % in turn, drawn as Rack draws it (to the power 1.5)', () => {
  assert.equal(nextOpacity(1), 0.5);
  assert.equal(nextOpacity(0.5), 0.2);
  assert.equal(nextOpacity(0.2), 1);
  assert.equal(nextOpacity(0.7), 0.5);
  assert.equal(drawnOpacity(1), 1);
  assert.ok(Math.abs(drawnOpacity(0.25) - 0.125) < 1e-9);
});

// ---- Typed wires and ports, normals, bundles, faces, "New ▸ LFO / ADSR" (0057 §1, §4b, §4h) -------------------------

test('wires by type: picture solid, light solid and thicker, value dashed, trigger dotted, audio and flow double lines', () => {
  assert.equal(WIRES.picture.dash, null);
  assert.equal(WIRES.light.dash, null);
  assert.ok(WIRES.light.width > WIRES.picture.width);
  assert.equal(WIRES.value.dash, '6 5');
  const [on, off] = WIRES.trigger.dash!.split(' ').map(Number);
  assert.ok(on < 0.5 && off > on, 'dots: next to no dash, a gap');
  assert.ok(WIRES.audio.double && WIRES.flow.double && !WIRES.value.double && !WIRES.picture.double);
  assert.ok(WIRES.flow.dash && WIRES.flow.dash !== WIRES.value.dash, 'a flow in long dashes');
  assert.equal(wireStyle('value'), '--wire-w: 1.5px; --wire-dash: 6 5');
  assert.ok(wireStyle('trigger', true).endsWith(MUTED_DASH));
  // The lab's every-wire patch: each cable drawn, by its type.
  const p = lab.wiresV2();
  const { edges } = graphOfV2({ patch: p, types: labTypes });
  for (const c of p.cables) assert.ok(edgeOf(edges, c.id), `${c.id} drawn`);
  const wire = (from: string, to: string) => {
    const c = p.cables.find((x) => `${x.from.module}.${x.from.port}` === from && `${x.to.module}.${x.to.port}` === to)!;
    return edgeOf(edges, c.id)!;
  };
  assert.deepEqual([wire('flame.light', 'fluid.light').type, wire('flame.light', 'fluid.light').class], ['wire', 'light']);
  assert.deepEqual([wire('flame.points', 'fluid.at').type, wire('flame.points', 'fluid.at').data?.wire], ['cable', 'points']);
  assert.deepEqual([wire('fluid.field', 'bend.field').type, wire('fluid.field', 'bend.field').data?.wire], ['cable', 'flow']);
  assert.equal(wire('waves.picture', 'bend.picture').class, 'picture');
  assert.equal(wire('music.beat', 'adsr.gate').data?.wire, 'trigger');
  assert.equal(wire('lfo.out', 'waves.hue').data?.wire, 'value');
  assert.deepEqual(new Set(edges.map((e) => e.data?.wire)), new Set(['picture', 'light', 'value', 'trigger', 'audio', 'points', 'flow']), 'every type drawn');
});

test('a polymorphic output carries what comes in: a Feedback on light gives light, Bend on a picture a picture', () => {
  const p = lab.wiresV2();
  const g = graphOfV2({ patch: p, types: labTypes });
  const into = (to: string) => g.edges.find((e) => e.target === to && e.targetHandle === 'in')!;
  assert.equal(into('flame').class, 'light', 'the Feedback, fed light, gives light');
  assert.equal(dataOf(g, 'feedback').poutType, 'light');
  assert.equal(dataOf(g, 'fluid').pinType, 'light');
  assert.equal(into('mix').class, 'picture');
  // Out of a Fluid fed light, into a picture input: no; out of one fed nothing, any of its types.
  const mixIn2 = { target: 'mix', targetHandle: handleOf(dataOf(g, 'mix'), 'in', 'in2') };
  assert.equal(connectionV2({ source: 'fluid', sourceHandle: 'out', ...mixIn2 }, g.nodes, p, [], labTypes), null);
  const bare: PatchV2 = { ...p, cables: p.cables.filter((c) => c.to.module !== 'fluid') };
  const gb = graphOfV2({ patch: bare, types: labTypes });
  assert.ok(connectionV2({ source: 'fluid', sourceHandle: 'out', ...mixIn2 }, gb.nodes, bare, [], labTypes));
});

test('ports and handles by type; connections only where the types fit', () => {
  const t = { ...types, waves: WAVES, adsr: ADSR, clock: CLOCK, envelope: ENVELOPE, sequencer: SEQUENCER };
  const p: PatchV2 = { ...sample([]), modules: [mod('music', 'music'), mod('waves', 'waves'), mod('adsr', 'adsr'), mod('lfo', 'lfo')] };
  const g = graphOfV2({ patch: p, types: t });
  assert.deepEqual(dataOf(g, 'waves').ports.map((x) => [x.id, x.type]).filter(([id]) => id === 'audio'), [['audio', 'audio']]);
  assert.deepEqual(dataOf(g, 'adsr').ports.filter((x) => x.id === 'gate').map((x) => x.type), ['trigger']);
  const music = dataOf(g, 'music');
  const c = (from: string, port: string, to: string, into: string) => connectionV2({ source: from, sourceHandle: handleOf(dataOf(g, from), 'out', port), target: to, targetHandle: handleOf(dataOf(g, to), 'in', into) }, g.nodes, p, [], t);
  assert.ok(c('music', 'beat', 'adsr', 'gate'), 'a trigger into a trigger');
  assert.equal(c('lfo', 'out', 'adsr', 'gate'), null, 'a value into a trigger: no');
  assert.ok(c('music', 'beat', 'waves', 'width'), 'a trigger into a value');
  assert.ok(music.outs.some((o) => o.id === 'audio' && o.type === 'audio'));
});

test("a port's normal is drawn faint until a cable is plugged into it, then it is gone", () => {
  const p = lab.wiresV2();
  const { edges } = graphOfV2({ patch: p, types: labTypes });
  const normal = (id: string) => edgeOf(edges, `normal:${id}`);
  const sound = normal('waves.audio')!;
  assert.equal(sound.class, 'normal w-audio');
  assert.equal(sound.source, 'music');
  assert.ok(sound.data?.normal && !sound.selectable && !sound.deletable);
  // The Fluid's flow into the Loop effect by its normal; Bend's is cabled, so none.
  assert.deepEqual([normal('loop.field')?.source, normal('loop.field')?.sourceHandle, normal('loop.field')?.class], ['fluid', 'o:field', 'normal w-flow']);
  assert.equal(normal('bend.field'), undefined);
  assert.equal(normal('fluid.at'), undefined, 'the flame’s points cabled in');
  // From a module's jack: the Sequencer stepped by the Clock's beat, reset by its bar.
  assert.deepEqual([normal('seq.clock')?.source, normal('seq.clock')?.sourceHandle, normal('seq.clock')?.data?.wire], ['clock', 'o:beat', 'trigger']);
  assert.equal(normal('seq.reset')?.sourceHandle, 'o:bar');
  // A normal from a module not there is not drawn.
  const alone = graphOfV2({ patch: { ...p, modules: p.modules.filter((m) => m.id !== 'clock') }, types: labTypes });
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
  const p = lab.wiresV2();
  const g = graphOfV2({ patch: p, types: labTypes });
  const by = (from: string, to: string) => edgeOf(g.edges, p.cables.find((c) => `${c.from.module}.${c.from.port}` === from && c.to.module === to)!.id)!;
  assert.equal(by('music.hits', 'bands').data?.channels, 6);
  assert.equal(by('music.level', 'waves').data?.channels, undefined, 'one band');
  // Fed six channels, the ADSR's face has a lane each; fed one, one.
  assert.equal(dataOf(g, 'bands').face?.lanes, 6);
  assert.equal(dataOf(g, 'adsr').face?.lanes, 1);
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

test('"New ▸ ADSR" from a knob (Visuals’ v1 cables until the store is mounted): the module, its cable, its gate from the beat', () => {
  const before = { routes: {}, extra: { layers: [], loop: [], modulators: [mod('lfo', 'lfo')], cables: [{ id: 'cable-1', from: { modulator: 'lfo' }, to: { module: 's1', knob: 'a' }, value: 0.5, mode: 'swing' as const }], macros: [] } };
  const made = withModulator(before, { id: 'adsr-1', type: 'adsr', knobs: { attack: 0.01 } }, ADSR, { module: 's1', knob: 'b' }, 'level');
  assert.deepEqual(made.cables.extra.modulators.map((m) => m.id), ['lfo', 'adsr-1']);
  const [sync, into] = made.cables.extra.cables.slice(1);
  assert.deepEqual([sync.from, sync.to, sync.value], [CLOCK_BEAT, { module: 'adsr-1', knob: 'gate' }, 1]);
  assert.deepEqual([into.from, into.to, into.mode], [{ modulator: 'adsr-1' }, { module: 's1', knob: 'b' }, 'level']);
  assert.equal(made.cable, into);
  assert.equal(new Set(made.cables.extra.cables.map((c) => c.id)).size, 3, 'ids free');
  assert.equal(before.extra.cables.length, 1, 'the cables before, untouched');
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

const { GRID, FOLD, INSTANCE } = await import('../src/lib/visuals/ports.ts');

test('points: a chain line (dash, dot); a Grid is an ordinary node with its jacks, its points into a Fold only', () => {
  const [dash, gap, dot] = WIRES.points.dash!.split(' ').map(Number);
  assert.ok(dash > 3 && dot < 0.5 && gap > 0, 'a dash and a dot');
  assert.ok(new Set(Object.values(WIRES).map((w) => `${w.dash}|${w.width}|${w.double}`)).size === Object.keys(WIRES).length, 'each type its own look');
  const t = { ...types, waves: WAVES, grid: GRID, fold: FOLD, instance: INSTANCE };
  const p: PatchV2 = { ...sample([cab('grid.points', 'fold.grid', 1, undefined, 'g')]), modules: [mod('waves', 'waves'), mod('grid', 'grid'), mod('fold', 'fold'), mod('inst', 'instance')] };
  const g = graphOfV2({ patch: p, types: t });
  assert.deepEqual(dataOf(g, 'grid').gives, { type: 'points' });
  assert.deepEqual(dataOf(g, 'grid').outs.map((o) => o.id), ['points', 'index', 'distance', 'angle', 'row', 'column']);
  assert.ok(dataOf(g, 'fold').ports.some((x) => x.id === 'grid' && x.type === 'points'));
  assert.ok(dataOf(g, 'inst').ports.some((x) => x.id === 'points' && x.type === 'points'));
  assert.equal(edgeOf(g.edges, 'g')?.data?.wire, 'points');
  const c = (to: string, port: string) => connectionV2({ source: 'grid', sourceHandle: 'mod', target: to, targetHandle: handleOf(dataOf(g, to), 'in', port) }, g.nodes, p, [], t);
  assert.equal(c('waves', 'hue'), null, 'points into a value: no');
  assert.ok(c('inst', 'points'));
});

test("a band's knobs are one polyphonic knob, its six as ticks, opened out to six; turned as one they keep apart", () => {
  const knob6 = (id: string) => Array.from({ length: 6 }, (_, b) => ({ id: `${id}.${b}`, label: `Linger ${b + 1}`, min: 0, max: 1, neutral: 0, default: 0.5, kind: 'level' as const }));
  const t = { ...types, src: { ...types.src, knobs: [...knob6('lighting'), knob('a')] } };
  const p: PatchV2 = {
    ...sample([cab('music.level', 's1.lighting.2', 0.5, 2, 'b2')]),
    modules: [{ id: 's1', type: 'src', knobs: { 'lighting.0': 0.2, 'lighting.1': 0.4, 'lighting.2': 0.6, 'lighting.3': 0.8, 'lighting.4': 0.5, 'lighting.5': 0.5 } }, mod('music', 'music')],
  };
  const g = graphOfV2({ patch: p, types: t });
  const d = dataOf(g, 's1');
  const poly = d.knobs.find((k) => k.id === 'lighting')!;
  assert.equal(poly.label, 'Linger');
  assert.deepEqual(poly.channels!.map((c) => c.id), ['lighting.0', 'lighting.1', 'lighting.2', 'lighting.3', 'lighting.4', 'lighting.5']);
  assert.ok(Math.abs(poly.value - 0.5) < 1e-9, 'its mean');
  assert.deepEqual(poly.channels!.map((c) => c.at), [0.2, 0.4, 0.6, 0.8, 0.5, 0.5]);
  assert.ok(!d.knobs.some((k) => k.id.startsWith('lighting.')), 'no channel knob drawn apart');
  assert.ok(!d.ports.some((x) => x.id === 'lighting'), 'nor as a port');
  assert.ok(poly.ring, 'a cable into one channel rings it');
  assert.equal(edgeOf(g.edges, 'b2')?.targetHandle, 'k:lighting', 'onto the one knob');
  // Opened out: six knobs, the cable onto its own channel's.
  const open = graphOfV2({ patch: p, types: t, open: new Set(['s1.lighting']) });
  const o = dataOf(open, 's1');
  assert.deepEqual(o.knobs.filter((k) => k.id.startsWith('lighting')).map((k) => [k.id, k.label]).slice(0, 2), [['lighting.0', 'Linger 1'], ['lighting.1', 'Linger 2']]);
  assert.equal(o.knobs.filter((k) => k.id.startsWith('lighting.')).length, 6);
  assert.equal(edgeOf(open.edges, 'b2')?.targetHandle, 'k:lighting.2');
  // Turned as one: each channel as far, clamped; one change of the store's, keyed by the knob.
  const turnedTo = polyTurned(poly, 0.7);
  assert.deepEqual(turnedTo.map((c) => c.id), poly.channels!.map((c) => c.id));
  assert.deepEqual(turnedTo.map((c) => Math.round(c.value * 100) / 100), [0.4, 0.6, 0.8, 1, 0.7, 0.7]);
});

// ---- A v2 patch drawn: devices as nodes, opened in place, entered; edits for the store (0057 §6, §7) ------------------

const await_graph = await import('../src/lib/visuals/patchgraph.ts');
const import_device = await import('../src/lib/visuals/device-make.ts');
const { editKey, patchAt, crumbsOf, portsOfModule, macroKnob } = await import('../src/lib/visuals/patchgraph.ts');
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
  // Light runs through Show's first parts (they pass what comes in) until Tone map makes it a picture.
  assert.equal(g.edges.find((e) => e.id === 'show/show-1')?.data?.wire, 'light');
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
  const inShow = connectionV2({ source: 'show/tonemap', sourceHandle: 'out', target: 'show/saturate', targetHandle: 'in' }, g.nodes, p, [], v2types);
  assert.deepEqual(inShow, { kind: 'connect', scope: ['show'], from: { module: 'tonemap', port: 'picture' }, to: { module: 'saturate', port: 'picture' } });
  assert.equal(connectionV2({ source: 'show/bend', sourceHandle: 'out', target: 'show/saturate', targetHandle: 'in' }, g.nodes, p, [], v2types), null, 'Bend carries light here: not into a picture');
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
  const p = sample([cab('music.beat', `k1.${nextPort('mode')}`, 1, undefined, 'n')]);
  const g = graphOfV2({ patch: p, types: t });
  const d = dataOf(g, 'k1');
  assert.equal(d.knobs.find((k) => k.id === 'mode')?.next, true, 'shown, cabled');
  assert.ok(!d.ports.some((x) => x.id.endsWith(':next')));
  assert.equal(g.edges.find((e) => e.id === 'n')?.targetHandle, 'k:mode:next');
  const from = (port: string, into: string) => connectionV2({ source: 'music', sourceHandle: handleOf(dataOf(g, 'music'), 'out', port), target: 'k1', targetHandle: into }, g.nodes, p, [], t);
  assert.equal(from('level', 'k:mode:next'), null, 'a value into a next: no');
  assert.ok(from('beat', 'k:turn'));
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
  const g = graphOfV2({ patch: sample([cab('lfo.out', `k1.${gainPort('amount')}`, 1, undefined, 'g')]), types });
  const d = dataOf(g, 'k1');
  const amount = d.knobs.find((k) => k.id === 'amount')!;
  assert.equal(amount.gain, true);
  assert.equal(amount.ring, null);
  assert.ok(!d.ports.some((x) => x.id.endsWith(':gain')));
  assert.equal(g.edges.find((e) => e.id === 'g')?.targetHandle, 'k:amount:gain');
});


test('a cable ends at its jack’s centre: half a jack in from the edge point on the jack’s side', () => {
  const { toJack, JACK } = await_graph;
  const r = JACK / 2;
  assert.deepEqual(toJack(100, 50, 'right'), { x: 100 - r, y: 50 });
  assert.deepEqual(toJack(100, 50, 'left'), { x: 100 + r, y: 50 });
  assert.deepEqual(toJack(100, 50, 'top'), { x: 100, y: 50 + r });
  assert.deepEqual(toJack(100, 50, 'bottom'), { x: 100, y: 50 - r });
  // Every edge drawn by our own components (picture wires and cables), over the nodes.
  const g = graphOfV2({ patch: sample(), types });
  assert.ok(g.edges.every((e) => e.type === 'wire' || e.type === 'cable'));
  assert.equal(g.edges.find((e) => e.id === 'p1')?.data?.step, true);
});

test('far out every cable is on its nodes’ far sockets (in on the input side, out on the output side), bundled', () => {
  const { farEdges, FAR_IN, FAR_OUT } = await_graph;
  const g = graphOfV2({ patch: sample(), types });
  const far = farEdges(g.edges, g.nodes);
  assert.equal(far.length, g.edges.length);
  for (const e of far) {
    assert.equal(e.sourceHandle, FAR_OUT, e.id);
    assert.equal(e.targetHandle, FAR_IN, e.id);
  }
  // A frame (a device opened in place) has none: its edges as they are.
  const open = graphOfV2({ patch: v2patch(), types: v2types, open: new Set(['show']) });
  const boxed = farEdges([{ id: 'x', source: 'show', target: 'flame', sourceHandle: 'out', targetHandle: 'in' }], open.nodes);
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
  // The lanes' width that fits best, at least as well as the fixed one.
  const fixed = graphOfV2({ patch: v2patch(), types: v2types, portrait: true });
  const best = graphOfV2({ patch: v2patch(), types: v2types, portrait: true, view: phone });
  assert.ok(fit(best) >= fit(fixed) - 1e-9, `${fit(best)} < ${fit(fixed)}`);
  // A node the viewer put somewhere stays.
  const kept = graphOfV2({ patch: sample(), types, portrait: true, view: phone, positions: { lfo: { x: 900, y: 900 } } });
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

test('too big for the view, the graph opens at its first module (the one nearest the top left corner)', () => {
  const { openView, firstModule } = await_graph;
  const pad = { top: 56, right: 16, bottom: 60, left: 16 };
  const nodes = [
    { position: { x: 0, y: 0 }, data: { kind: 'lane' } },
    { position: { x: 400, y: 30 }, data: { kind: 'module' } },
    { position: { x: 900, y: 600 }, data: { kind: 'module' } },
    { position: { x: 380, y: 500 }, data: { kind: 'module' } },
  ] as never[];
  assert.deepEqual(firstModule(nodes), { x: 400, y: 30 }, 'not the leftmost far down');
  assert.deepEqual(firstModule(nodes, true), { x: 400, y: 30 });
  assert.deepEqual(firstModule([{ position: { x: 0, y: 40 }, data: { kind: 'module' } }, { position: { x: 300, y: 0 }, data: { kind: 'module' } }] as never[]), { x: 0, y: 40 });
  const v = openView({ x: 0, y: 0, width: 3000, height: 3000 }, { width: 390, height: 500 }, pad, 1, firstModule(nodes, true));
  assert.deepEqual(v, { x: 16 - 400, y: 56 - 30, zoom: 1 });
});

test("a step knob's diamond and a level knob's gain dot are on (shown) only where a cable is in them", () => {
  const { gainPort, nextPort } = await_graph;
  const t = { ...types, fx: { ...types.fx, knobs: [...types.fx.knobs, { id: 'mode', label: 'Mode', min: 1, max: 4, neutral: 1, default: 1, kind: 'step' as const }] } };
  const bare = graphOfV2({ patch: sample(), types: t, open: new Set(['k1']) });
  const k = (g: typeof bare, id: string) => (g.nodes.find((n) => n.id === 'k1')!.data as ModuleData).knobs.find((x) => x.id === id)!;
  assert.ok(k(bare, 'mode').next && !k(bare, 'mode').nextOn);
  assert.ok(k(bare, 'amount').gain && !k(bare, 'amount').gainOn);
  const p = sample([cab('music.beat', `k1.${nextPort('mode')}`, 1, undefined, 'n'), cab('lfo.out', `k1.${gainPort('amount')}`, 1, undefined, 'g')]);
  const wired = graphOfV2({ patch: p, types: t, open: new Set(['k1']) });
  assert.ok(k(wired, 'mode').nextOn && k(wired, 'amount').gainOn);
});

test('fine adjust: full speed within 40 px up, half to 80, a quarter to 160, an eighth beyond; a step knob in whole steps', () => {
  const { fineSpeed, fineStep, speedLabel, fineText, FINE_SPAN } = await_graph;
  assert.deepEqual([0, 40, 41, 80, 81, 160, 161, 500, -30].map(fineSpeed), [1, 1, 0.5, 0.5, 0.25, 0.25, 0.125, 0.125, 1]);
  assert.deepEqual([1, 0.5, 0.25, 0.125].map(speedLabel), ['×1', '×½', '×¼', '×⅛']);
  const level = { min: 0, max: 2, kind: 'level' as const };
  // A whole span sideways at full speed: the whole range; at an eighth, an eighth of it.
  assert.equal(fineStep(level, 0, FINE_SPAN, 1).value, 2);
  assert.equal(fineStep(level, 1, FINE_SPAN, 0.125).value, 1.25);
  assert.equal(fineStep(level, 1.9, FINE_SPAN, 1).value, 2, 'kept in range');
  assert.equal(fineStep(level, 0.1, -FINE_SPAN, 1).value, 0);
  // A step knob: whole steps, the fraction carried so slow moves still get there.
  const step = { min: 1, max: 4, kind: 'step' as const };
  let raw = 2;
  for (let i = 0; i < 10; i++) raw = fineStep(step, raw, FINE_SPAN / 30, 1).raw;
  assert.equal(fineStep(step, raw, 0, 1).value, 3);
  assert.equal(fineText(step, 2.6), '3');
  assert.equal(fineText(level, 0.123456), '0.123');
  assert.equal(fineText({ min: 20, max: 16000, kind: 'swing' }, 251.7), '252');
});
