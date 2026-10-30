// The Waves source (decision 0056, step 4): MilkDrop's kind of wave, written anew. The sound's recent samples drawn as
// a line, or where there are none (no live sound) a wave made of the bands' levels. Line, circle and radial are knobs
// mixed point by point, never a switch. Nothing at the width's neutral, or the opacity's. The sound comes in by its
// `audio` port (0057 §1), normalled from Music's.
import type { FrameInfo, ModuleType, PictureModule } from '../patch';
import type { Ports } from '../ports';
import { BANDS, SEGMENT_FLOATS, TAU, WAVE_POINTS, closeSeam, hsv, resample, smooth, synthWave, waveSegments } from './geometry';
import { Mesh, QUAD, done, program, target } from './gl';
import { knob, values } from './knob';

const KNOBS = [
  knob('width', 'Width', 0, 0.05, 0.006, 'level'),
  knob('opacity', 'Opacity', 0, 1, 0.9, 'level'),
  knob('amplitude', 'Height', 0, 1, 0.35),
  knob('circle', 'Circle', 0, 1, 0),
  knob('radial', 'Radial', 0, 1, 0),
  knob('radius', 'Radius', 0, 1, 0.45),
  knob('length', 'Length', 0, 1, 1),
  knob('x', 'Across', -1, 1, 0),
  knob('y', 'Up', -1, 1, 0),
  knob('rotation', 'Rotation', 0, 1, 0),
  knob('spin', 'Spin', -1, 1, 0),
  knob('smoothing', 'Smoothing', 0, 1, 0.3),
  knob('hue', 'Hue', 0, 1, 0.5),
  knob('hueSpread', 'Hue along', 0, 1, 0),
  knob('saturation', 'Saturation', 0, 1, 0.6),
];

export const WAVES: ModuleType & { ports: Ports } = {
  type: 'waves',
  label: 'Waves',
  place: 'source',
  knobs: KNOBS,
  ports: {
    in: [{ id: 'audio', label: 'Sound', type: 'audio', normal: { module: 'music', port: 'audio' } }, ...KNOBS.map((k) => ({ id: k.id, label: k.label, type: 'value' as const }))],
    out: [{ id: 'picture', label: 'Picture', type: 'picture' }],
  },
};

/** The sound a frame gives Waves: its `audio` input's samples, else the frame's own (where nothing passes inputs). */
export function soundOf(frame: Pick<FrameInfo, 'inputs' | 'waveform'>): Float32Array | null {
  const v = frame.inputs?.audio ?? frame.waveform;
  return v && v.length >= 2 ? v : null;
}

const VERTEX = `#version 300 es
layout(location = 0) in vec4 aSegment;
layout(location = 1) in vec3 aEnd;
layout(location = 2) in vec3 aColour;
uniform vec2 uHalf;
uniform float uUnit;
uniform float uHalfWidth;
out vec2 vAt;
flat out vec4 vSegment;
flat out vec4 vColour;
void main() {
  // In pixels: the segment's quad, its half width and a pixel more about it on every side.
  vec2 a = aSegment.xy * uUnit, b = aSegment.zw * uUnit;
  vec2 d = b - a;
  float l = length(d);
  vec2 t = l > 1e-4 ? d / l : vec2(1.0, 0.0);
  vec2 n = vec2(-t.y, t.x);
  float m = uHalfWidth + 1.0;
  vAt = mix(a, b, aEnd.x) + t * (aEnd.x * 2.0 - 1.0) * m + n * aEnd.y * m;
  vSegment = vec4(a, b);
  vColour = vec4(aColour, aEnd.z);
  gl_Position = vec4(vAt / uHalf, 0.0, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision highp float;
uniform float uHalfWidth;
in vec2 vAt;
flat in vec4 vSegment;
flat in vec4 vColour;
layout(location = 0) out vec4 o;
void main() {
  // How far from the segment (its ends round, so the joins close).
  vec2 pa = vAt - vSegment.xy, ba = vSegment.zw - vSegment.xy;
  float h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-4), 0.0, 1.0);
  float d = length(pa - ba * h);
  float alpha = vColour.a * clamp(uHalfWidth + 0.5 - d, 0.0, 1.0) * min(2.0 * uHalfWidth, 1.0);
  o = vec4(vColour.rgb * alpha, alpha);
}`;

export class Waves implements PictureModule {
  private program: WebGLProgram;
  private mesh: Mesh;
  private at: Record<string, WebGLUniformLocation | null>;
  private samples = new Float32Array(WAVE_POINTS);
  private levels = new Float32Array(BANDS);
  private segments = new Float32Array(WAVE_POINTS * SEGMENT_FLOATS);
  private vertices: Float32Array;
  private spin = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.program = program(gl, VERTEX, FRAGMENT);
    this.mesh = new Mesh(gl, [4, 3, 3]);
    this.vertices = new Float32Array(WAVE_POINTS * 6 * this.mesh.stride);
    this.at = Object.fromEntries(['uHalf', 'uUnit', 'uHalfWidth'].map((n) => [n, gl.getUniformLocation(this.program, n)]));
  }

  draw(gl: WebGL2RenderingContext, _input: WebGLTexture | null, out: WebGLFramebuffer | null, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo) {
    const k = values(WAVES, knobs);
    this.spin = (this.spin + TAU * k.spin * frame.dt) % TAU;
    target(gl, out, width, height);
    if (k.width <= 0 || k.opacity <= 0) return;
    const v = this.samples;
    const sound = soundOf(frame);
    if (sound) resample(sound, v);
    else {
      for (let b = 0; b < BANDS; b++) this.levels[b] = frame.signals({ signal: 'level', band: b });
      synthWave(this.levels, frame.time, v);
    }
    smooth(v, k.smoothing);
    closeSeam(v, k.circle);
    const short = Math.min(width, height);
    const n = waveSegments(v, k, this.spin, width / short, height / short, this.segments);
    const s = this.segments;
    const out6 = this.vertices;
    const stride = this.mesh.stride;
    for (let i = 0; i < n; i++) {
      const [r, g, b] = hsv(k.hue + (k.hueSpread * i) / n, k.saturation, 1);
      const alpha = k.opacity * s[i * SEGMENT_FLOATS + 4];
      for (let c = 0; c < 6; c++) {
        const o = (i * 6 + c) * stride;
        out6.set(s.subarray(i * SEGMENT_FLOATS, i * SEGMENT_FLOATS + 4), o);
        out6[o + 4] = (QUAD[c * 2] + 1) / 2;
        out6[o + 5] = QUAD[c * 2 + 1];
        out6[o + 6] = alpha;
        out6[o + 7] = r;
        out6[o + 8] = g;
        out6[o + 9] = b;
      }
    }
    const unit = short / 2;
    gl.useProgram(this.program);
    gl.uniform2f(this.at.uHalf, width / 2, height / 2);
    gl.uniform1f(this.at.uUnit, unit);
    gl.uniform1f(this.at.uHalfWidth, 0.5 * k.width * unit);
    // Brightest wins where segments overlap at their joins, so a join is never doubled.
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.MAX);
    this.mesh.draw(gl, out6, n * 6);
    done(gl);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteProgram(this.program);
    this.mesh.dispose(gl);
  }
}
