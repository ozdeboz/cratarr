// The picture loop's Fluid (decision 0056): the whole picture, after the layers are blended and the loop's Warp, carried
// along by a fluid the music stirs, as the flame's own loop carries its light (fluid.ts, decision 0047). The same fluid
// and the same stirring (stirring.ts): each band's hits kick, its level stirs, the beats push out from the middle and
// turn, a downbeat shoves, a new section whirls. The picture loop has no parts, so the bands' places are six round the
// middle. At Amount 0 it is a plain copy (its neutral); what fades is the loop's Keep's, not this.

import { Fluid, gridFor } from './fluid';
import type { FrameInfo, Knob, ModuleType, PictureModule } from './patch';
import { newStirring, stir, type StirInput, type Stirring } from './stirring';

const knob = (id: string, label: string, min: number, max: number, neutral: number, def: number): Knob => ({
  id,
  label,
  min,
  max,
  neutral,
  default: def,
  kind: 'level',
});

/** Amount: how far the picture moves with the flow (0: not at all); Stir: how hard the music pushes; Flow: how long its
 *  motion lasts; Swirl: how strongly its small whirls are kept. */
export const FLUID_LOOP: ModuleType = {
  type: 'fluid-loop',
  label: 'Fluid',
  place: 'loop',
  knobs: [knob('amount', 'Amount', 0, 1, 0, 0.6), knob('stir', 'Stir', 0, 1, 0, 0.5), knob('flow', 'Flow', 0, 1, 0, 0.5), knob('swirl', 'Swirl', 0, 2, 0, 1)],
};

const BANDS = 6;
/** The bands' places: six round the middle, this far out (texture coordinates). */
const PART_OUT = 0.25;
const PARTS: [number, number][] = Array.from({ length: BANDS }, (_, b) => {
  const a = (b * Math.PI) / 3 + Math.PI / 6;
  return [0.5 + PART_OUT * Math.cos(a), 0.5 + PART_OUT * Math.sin(a)];
});
const TURNS = PARTS.map((_, b) => (b % 2 ? -1 : 1));
/** A band's hit, told from its Hits signal: a rise of at least this much over the last frame's. */
const HIT_RISE = 0.1;

/** What the loop's stirring keeps from frame to frame: the stirring's own, and each band's hits counted. */
export type LoopStir = { stirring: Stirring; hits: number[]; lights: number[]; strengths: number[] };
export const newLoopStir = (): LoopStir => ({ stirring: newStirring(), hits: Array(BANDS).fill(-1), lights: Array(BANDS).fill(0), strengths: Array(BANDS).fill(0) });

/** The frame as the stirring takes it, from the frame's signals (absolute, 0..1: what the stirring was made for) and
 *  its knobs. Pure. */
export function stirInputOf(s: LoopStir, frame: Pick<FrameInfo, 'dt' | 'time' | 'beat' | 'signals'>, k: Record<string, number>): StirInput {
  const read = (signal: string, band?: number) => {
    const v = frame.signals({ signal, band, reference: 'absolute' });
    return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0;
  };
  const hits = PARTS.map((_, b) => {
    const light = read('hits', b);
    if (light - s.lights[b] >= HIT_RISE) {
      s.hits[b]++;
      s.strengths[b] = light;
    }
    s.lights[b] = light;
    return s.hits[b] < 0 ? null : { index: s.hits[b], strength: s.strengths[b], nextIn: null };
  });
  const beat = frame.beat ? { ...frame.beat, nextIn: null } : null;
  return {
    t: frame.time,
    dt: frame.dt,
    amount: Math.max(0, k.stir ?? 0),
    parts: PARTS,
    turns: TURNS,
    lights: PARTS.map((_, b) => read('hits', b)),
    level: PARTS.map((_, b) => read('level', b)),
    continuity: PARTS.map(() => 0.5),
    hits,
    beat,
    downbeat: read('downbeat'),
    section: read('section'),
    loudness: read('loudness'),
    swell: read('swell'),
    bps: frame.beat && frame.beat.length > 0 ? 1 / frame.beat.length : 1.5,
    center: [0.5, 0.5],
  };
}

/** A running picture-loop Fluid: its fluid made at the first frame, at the picture's size. */
export function createFluidLoop(): PictureModule {
  let fluid: Fluid | null = null;
  let copy: { prog: WebGLProgram; vao: WebGLVertexArrayObject } | null = null;
  const s = newLoopStir();
  return {
    draw(gl, input, out, width, height, knobs, frame) {
      if (!input) return;
      if (!fluid && Fluid.supported(gl)) {
        try {
          fluid = new Fluid(gl, ...gridFor(width, height));
        } catch (e) {
          console.warn('Visuals: the loop’s fluid cannot run here', e);
        }
      }
      if (!fluid) {
        // A device that cannot hold the fluid: the picture as it is.
        copy ??= copier(gl);
        gl.disable(gl.BLEND);
        gl.bindFramebuffer(gl.FRAMEBUFFER, out);
        gl.viewport(0, 0, width, height);
        gl.useProgram(copy.prog);
        gl.bindVertexArray(copy.vao);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, input);
        gl.uniform1i(gl.getUniformLocation(copy.prog, 'uPicture'), 0);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        return;
      }
      fluid.resize(...gridFor(width, height));
      const amount = Math.min(1, Math.max(0, knobs.amount ?? 0));
      if (amount > 0) {
        const { forces, flow } = stir(s.stirring, stirInputOf(s, frame, knobs));
        flow.energy = Math.min(1, flow.energy * 2 * Math.max(0, knobs.flow ?? 0));
        flow.swirl *= Math.max(0, knobs.swirl ?? 0);
        fluid.push(forces);
        fluid.step(frame.dt, flow);
      }
      fluid.carry(input, out, width, height, amount);
    },
    dispose(gl) {
      fluid?.dispose();
      fluid = null;
      if (copy) {
        gl.deleteProgram(copy.prog);
        gl.deleteVertexArray(copy.vao);
        copy = null;
      }
    },
  };
}

/** A plain copy of a picture: one triangle over the target. */
function copier(gl: WebGL2RenderingContext): { prog: WebGLProgram; vao: WebGLVertexArrayObject } {
  const vs = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;
  const fs = `#version 300 es
precision highp float;
uniform sampler2D uPicture;
in vec2 vUv;
out vec4 o;
void main() { o = texture(uPicture, vUv); }`;
  const shader = (type: number, src: string) => {
    const sh = gl.createShader(type)!;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    return sh;
  };
  const prog = gl.createProgram()!;
  const [v, f] = [shader(gl.VERTEX_SHADER, vs), shader(gl.FRAGMENT_SHADER, fs)];
  gl.attachShader(prog, v);
  gl.attachShader(prog, f);
  gl.linkProgram(prog);
  gl.deleteShader(v);
  gl.deleteShader(f);
  return { prog, vao: gl.createVertexArray()! };
}
