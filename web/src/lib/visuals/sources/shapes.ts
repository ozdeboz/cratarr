// The Shapes source (decision 0056, step 4): MilkDrop's kind of shapes, written anew. Polygons on a ring, each an
// outline or filled or both; the number of sides morphs through the fractions (the two whole polygons' edges mixed,
// near a circle at many sides), and the count does too (the last one fading in as the ring makes room). No shapes at
// the count's neutral, or the opacity's.
import type { FrameInfo, ModuleType, PictureModule } from '../patch';
import { MOST_SHAPES, SHAPE_FLOATS, TAU, shapeLayout, sidesOf, type ShapeClocks } from './geometry';
import { Mesh, QUAD, done, program, target } from './gl';
import { knob, values } from './knob';

export const SHAPES: ModuleType = {
  type: 'shapes',
  label: 'Shapes',
  place: 'source',
  knobs: [
    knob('count', 'Shapes', 0, MOST_SHAPES, 4, 'level'),
    knob('opacity', 'Opacity', 0, 1, 0.85, 'level'),
    knob('sides', 'Sides', 3, 48, 5),
    knob('radius', 'Radius', 0, 1, 0.22),
    knob('thickness', 'Thickness', 0, 0.1, 0.012),
    knob('fill', 'Fill', 0, 1, 0),
    knob('spread', 'Spread', 0, 1, 0.45),
    knob('x', 'Across', -1, 1, 0),
    knob('y', 'Up', -1, 1, 0),
    knob('orbit', 'Orbit', -1, 1, 0.03),
    knob('rotation', 'Rotation', 0, 1, 0),
    knob('spin', 'Spin', -2, 2, 0.05),
    knob('twist', 'Twist', -0.5, 0.5, 0),
    knob('taper', 'Taper', -1, 1, 0),
    knob('hue', 'Hue', 0, 1, 0.55),
    knob('hueSpread', 'Hue spread', 0, 1, 0.12),
    knob('saturation', 'Saturation', 0, 1, 0.75),
    knob('additive', 'Additive', 0, 1, 1),
    // How it meets the picture under it (compose.ts, at the blend): its colours the flame's around it, its light
    // spread soft. The owner: shapes "do not blend well" with the flame.
    knob('tint', 'Flame colours', 0, 1, 0.7),
    knob('glow', 'Glow', 0, 1, 0.4),
  ],
};

const VERTEX = `#version 300 es
layout(location = 0) in vec4 aShape;
layout(location = 1) in vec4 aColour;
layout(location = 2) in vec2 aCorner;
uniform vec2 uHalf;
uniform float uUnit;
uniform float uMargin;
out vec2 vLocal;
out float vRadius;
out vec4 vColour;
void main() {
  // In pixels about the shape's middle, turned with it; the quad its corners' circle and the outline's half.
  float r = aShape.z * uUnit;
  vLocal = aCorner * (r + uMargin);
  float c = cos(aShape.w), s = sin(aShape.w);
  vec2 p = aShape.xy * uUnit + vec2(c * vLocal.x - s * vLocal.y, s * vLocal.x + c * vLocal.y);
  vRadius = r;
  vColour = aColour;
  gl_Position = vec4(p / uHalf, 0.0, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision highp float;
uniform float uSides;
uniform float uFrac;
uniform float uHalfLine;
uniform float uFill;
uniform float uAdd;
in vec2 vLocal;
in float vRadius;
in vec4 vColour;
layout(location = 0) out vec4 o;
// Past the nearest side's line, in pixels (geometry.ts polygonEdge).
float edge(float r, float a, float n, float radius) {
  float s = 6.2831853 / n;
  float b = mod(a, s) - 0.5 * s;
  return r * cos(b) - radius * cos(0.5 * s);
}
void main() {
  float r = length(vLocal);
  float a = r > 1e-4 ? atan(vLocal.y, vLocal.x) : 0.0;
  float e = mix(edge(r, a, uSides, vRadius), edge(r, a, uSides + 1.0, vRadius), uFrac);
  float inside = clamp(0.5 - e, 0.0, 1.0);
  float line = clamp(uHalfLine + 0.5 - abs(e), 0.0, 1.0) * min(2.0 * uHalfLine, 1.0);
  float alpha = vColour.a * max(uFill * inside, line);
  // Premultiplied; additive light covers nothing beneath it.
  o = vec4(vColour.rgb * alpha, alpha * (1.0 - uAdd));
}`;

export class Shapes implements PictureModule {
  private program: WebGLProgram;
  private mesh: Mesh;
  private at: Record<string, WebGLUniformLocation | null>;
  private layout = new Float32Array(MOST_SHAPES * SHAPE_FLOATS);
  private vertices: Float32Array;
  private clocks: ShapeClocks = { orbit: 0, spin: 0 };

  constructor(gl: WebGL2RenderingContext) {
    this.program = program(gl, VERTEX, FRAGMENT);
    this.mesh = new Mesh(gl, [4, 4, 2]);
    this.vertices = new Float32Array(MOST_SHAPES * 6 * this.mesh.stride);
    this.at = Object.fromEntries(['uHalf', 'uUnit', 'uMargin', 'uSides', 'uFrac', 'uHalfLine', 'uFill', 'uAdd'].map((n) => [n, gl.getUniformLocation(this.program, n)]));
  }

  draw(gl: WebGL2RenderingContext, _input: WebGLTexture | null, out: WebGLFramebuffer | null, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo) {
    const k = values(SHAPES, knobs);
    // The rates move clocks, so a rate that changes never jumps the turn.
    this.clocks.orbit = (this.clocks.orbit + TAU * k.orbit * frame.dt) % TAU;
    this.clocks.spin = (this.clocks.spin + TAU * k.spin * frame.dt) % TAU;
    target(gl, out, width, height);
    const short = Math.min(width, height);
    const n = shapeLayout(k, this.clocks, width / short, height / short, this.layout);
    if (n === 0) return;
    const v = this.vertices;
    const stride = this.mesh.stride;
    for (let i = 0; i < n; i++)
      for (let c = 0; c < 6; c++) {
        const o = (i * 6 + c) * stride;
        v.set(this.layout.subarray(i * SHAPE_FLOATS, (i + 1) * SHAPE_FLOATS), o);
        v[o + 8] = QUAD[c * 2];
        v[o + 9] = QUAD[c * 2 + 1];
      }
    const unit = short / 2;
    const halfLine = 0.5 * k.thickness * unit;
    const [sides, frac] = sidesOf(k.sides);
    gl.useProgram(this.program);
    gl.uniform2f(this.at.uHalf, width / 2, height / 2);
    gl.uniform1f(this.at.uUnit, unit);
    gl.uniform1f(this.at.uMargin, halfLine + 1.5);
    gl.uniform1f(this.at.uSides, sides);
    gl.uniform1f(this.at.uFrac, frac);
    gl.uniform1f(this.at.uHalfLine, halfLine);
    gl.uniform1f(this.at.uFill, k.fill);
    gl.uniform1f(this.at.uAdd, k.additive);
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.FUNC_ADD);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    this.mesh.draw(gl, v, n * 6);
    done(gl);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteProgram(this.program);
    this.mesh.dispose(gl);
  }
}
