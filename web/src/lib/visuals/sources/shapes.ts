// The shape sources (decision 0056, step 4; 0057 §4g): MilkDrop's kind of shapes, written anew, from the one set of
// shapes (shape-set.ts). Two modules, one code: they lay their shapes on a ring alike and stroke them alike (an outline,
// a fill, or both), and differ only in how they pick a shape.
// - Regular shapes (type `shapes`, the Shapes of 70): the polygon and the star. The number of sides morphs through the
//   fractions (the two whole shapes' edges mixed, near a circle at many sides), the depth from the polygon (0) to a deep
//   star. Its corners are a `points` output, for Instance (a corner fractal).
// - Irregular shapes (type `irregular`): one of the set's list, a step; drawn from that shape's grid of distances.
//   Monochrome: light only, its colour a Tint's (0057 §4g).
// The count morphs too (the last one fading in as the ring makes room). No shapes at the count's neutral, or the
// opacity's.
import type { FrameInfo, Knob, ModuleType, PictureModule } from '../patch';
import { IRREGULAR, SDF_SIZE, SHAPE_LABEL, regularOutline, sdfGrid } from '../shape-set';
import { MOST_SHAPES, SHAPE_FLOATS, TAU, shapeLayout, sidesOf, type ShapeClocks } from './geometry';
import { Mesh, QUAD, done, program, target } from './gl';
import { knob, values } from './knob';

/** Where both lay their shapes and how they stroke them. */
const LAYOUT: Knob[] = [
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
];
const COUNT = [knob('count', 'Shapes', 0, MOST_SHAPES, 4, 'level'), knob('opacity', 'Opacity', 0, 1, 0.85, 'level')];

export const SHAPES: ModuleType = {
  type: 'shapes',
  label: 'Regular shapes',
  place: 'source',
  knobs: [
    ...COUNT,
    knob('sides', 'Sides', 3, 48, 5),
    // 0 the polygon (the Shapes of 70, as it was), up to a deep star.
    knob('depth', 'Depth', 0, 1, 0),
    ...LAYOUT,
    knob('hue', 'Hue', 0, 1, 0.55),
    knob('hueSpread', 'Hue spread', 0, 1, 0.12),
    knob('saturation', 'Saturation', 0, 1, 0.75),
    knob('additive', 'Additive', 0, 1, 1),
  ],
};

export const IRREGULAR_SHAPES: ModuleType = {
  type: 'irregular',
  label: 'Irregular shapes',
  place: 'source',
  knobs: [...COUNT, { ...knob('shape', 'Shape', 0, IRREGULAR.length - 1, 0), kind: 'step', options: IRREGULAR.map((n) => SHAPE_LABEL[n]) }, ...LAYOUT, knob('additive', 'Additive', 0, 1, 1)],
};

/** The corners of Regular shapes' shapes this frame (as `shapeLayout` lays them, in its units), into `out` as x, y
 *  pairs; how many. Each shape as many as its sides round to. */
export function shapeCorners(k: Record<string, number>, layout: Float32Array, n: number, out: Float32Array): number {
  const corners = Math.max(3, Math.round(k.sides));
  let m = 0;
  for (let i = 0; i < n && (m + 1) * 2 <= out.length; i++) {
    const o = i * SHAPE_FLOATS;
    for (let c = 0; c < corners && (m + 1) * 2 <= out.length; c++) {
      const a = (TAU * c) / corners;
      const r = layout[o + 2] * regularOutline(a, k.sides, k.depth ?? 0);
      out[m * 2] = layout[o] + r * Math.cos(a + layout[o + 3]);
      out[m * 2 + 1] = layout[o + 1] + r * Math.sin(a + layout[o + 3]);
      m++;
    }
  }
  return m;
}

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

// Past the shape's outline, in pixels: Regular's by its sides and depth (shape-set.ts regularEdge), Irregular's from
// its grid of distances (shape-set.ts sdfGrid), past the grid the distance on to it added.
const REGULAR_EDGE = `uniform float uSides;
uniform float uFrac;
uniform float uDepth;
// Past the nearest side's line (geometry.ts polygonEdge).
float edge(float r, float a, float n, float radius) {
  float s = 6.2831853 / n;
  float b = mod(a, s) - 0.5 * s;
  return r * cos(b) - radius * cos(0.5 * s);
}
// A star's (shape-set.ts sideOf): its corner to its inner corner, whose distance is the polygon's side's middle at
// depth 0, nearer the middle as it deepens.
float starEdge(float r, float a, float n, float radius) {
  float h = 3.14159265 / n;
  float rho = cos(h) * (1.0 - 0.95 * uDepth);
  vec2 e = vec2(rho * cos(h) - 1.0, rho * sin(h));
  vec2 nn = normalize(vec2(e.y, -e.x));
  float p = mod(a, 2.0 * h);
  p = p > h ? 2.0 * h - p : p;
  return r * (cos(p) * nn.x + sin(p) * nn.y) - radius * nn.x;
}
float shapeEdge(float r, float a, float radius) {
  if (uDepth <= 0.0) return mix(edge(r, a, uSides, radius), edge(r, a, uSides + 1.0, radius), uFrac);
  return mix(starEdge(r, a, uSides, radius), starEdge(r, a, uSides + 1.0, radius), uFrac);
}`;

const IRREGULAR_EDGE = `uniform highp sampler2DArray uAtlas;
uniform float uLayer;
float shapeEdge(float r, float a, float radius) {
  vec2 q = vLocal / max(radius, 1e-4);
  vec2 c = clamp(q, -1.0, 1.0);
  return (texture(uAtlas, vec3((c + 1.0) * 0.5, uLayer)).r + length(q - c)) * radius;
}`;

const fragment = (edge: string) => `#version 300 es
precision highp float;
uniform float uHalfLine;
uniform float uFill;
uniform float uAdd;
in vec2 vLocal;
in float vRadius;
in vec4 vColour;
layout(location = 0) out vec4 o;
${edge}
void main() {
  float r = length(vLocal);
  float a = r > 1e-4 ? atan(vLocal.y, vLocal.x) : 0.0;
  float e = shapeEdge(r, a, vRadius);
  float inside = clamp(0.5 - e, 0.0, 1.0);
  float line = clamp(uHalfLine + 0.5 - abs(e), 0.0, 1.0) * min(2.0 * uHalfLine, 1.0);
  float alpha = vColour.a * max(uFill * inside, line);
  // Premultiplied; additive light covers nothing beneath it.
  o = vec4(vColour.rgb * alpha, alpha * (1.0 - uAdd));
}`;

/** What both draw with: the ring laid out, each shape a quad, stroked by its edge. */
class ShapeSource implements PictureModule {
  private type: ModuleType;
  protected program: WebGLProgram;
  private mesh: Mesh;
  protected at: Record<string, WebGLUniformLocation | null>;
  protected layout = new Float32Array(MOST_SHAPES * SHAPE_FLOATS);
  protected drawn = 0;
  private vertices: Float32Array;
  private clocks: ShapeClocks = { orbit: 0, spin: 0 };

  constructor(gl: WebGL2RenderingContext, type: ModuleType, edge: string, own: string[]) {
    this.type = type;
    this.program = program(gl, VERTEX, fragment(edge));
    this.mesh = new Mesh(gl, [4, 4, 2]);
    this.vertices = new Float32Array(MOST_SHAPES * 6 * this.mesh.stride);
    this.at = Object.fromEntries(['uHalf', 'uUnit', 'uMargin', 'uHalfLine', 'uFill', 'uAdd', ...own].map((n) => [n, gl.getUniformLocation(this.program, n)]));
  }

  /** Its own uniforms (the program in use), and any textures it binds; undone in `after`. */
  protected shape(_gl: WebGL2RenderingContext, _k: Record<string, number>) {}
  protected after(_gl: WebGL2RenderingContext) {}

  draw(gl: WebGL2RenderingContext, _input: WebGLTexture | null, out: WebGLFramebuffer | null, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo) {
    const k = values(this.type, knobs);
    // The rates move clocks, so a rate that changes never jumps the turn.
    this.clocks.orbit = (this.clocks.orbit + TAU * k.orbit * frame.dt) % TAU;
    this.clocks.spin = (this.clocks.spin + TAU * k.spin * frame.dt) % TAU;
    target(gl, out, width, height);
    const short = Math.min(width, height);
    const n = shapeLayout(k, this.clocks, width / short, height / short, this.layout);
    this.drawn = n;
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
    gl.useProgram(this.program);
    gl.uniform2f(this.at.uHalf, width / 2, height / 2);
    gl.uniform1f(this.at.uUnit, unit);
    gl.uniform1f(this.at.uMargin, halfLine + 1.5);
    gl.uniform1f(this.at.uHalfLine, halfLine);
    gl.uniform1f(this.at.uFill, k.fill);
    gl.uniform1f(this.at.uAdd, k.additive);
    this.shape(gl, k);
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.FUNC_ADD);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    this.mesh.draw(gl, v, n * 6);
    this.after(gl);
    done(gl);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteProgram(this.program);
    this.mesh.dispose(gl);
  }
}

export class Shapes extends ShapeSource {
  private k: Record<string, number> = {};

  constructor(gl: WebGL2RenderingContext) {
    super(gl, SHAPES, REGULAR_EDGE, ['uSides', 'uFrac', 'uDepth']);
  }

  protected shape(gl: WebGL2RenderingContext, k: Record<string, number>) {
    this.k = k;
    const [sides, frac] = sidesOf(k.sides);
    gl.uniform1f(this.at.uSides, sides);
    gl.uniform1f(this.at.uFrac, frac);
    gl.uniform1f(this.at.uDepth, k.depth);
  }

  /** The corners drawn last frame (its `corners` output: x, y pairs in units, half the shorter side 1); how many. */
  corners(out: Float32Array): number {
    return this.drawn ? shapeCorners(this.k, this.layout, this.drawn, out) : 0;
  }
}

export class IrregularShapes extends ShapeSource {
  private atlas: WebGLTexture;
  /** The shapes in the atlas so far (each made the first time it is drawn). */
  private loaded = new Set<number>();

  constructor(gl: WebGL2RenderingContext) {
    super(gl, IRREGULAR_SHAPES, IRREGULAR_EDGE, ['uAtlas', 'uLayer']);
    this.atlas = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.R16F, SDF_SIZE, SDF_SIZE, IRREGULAR.length);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, null);
  }

  protected shape(gl: WebGL2RenderingContext, k: Record<string, number>) {
    const layer = Math.min(IRREGULAR.length - 1, Math.max(0, Math.round(k.shape)));
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    if (!this.loaded.has(layer)) {
      gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, layer, SDF_SIZE, SDF_SIZE, 1, gl.RED, gl.FLOAT, sdfGrid(IRREGULAR[layer]));
      this.loaded.add(layer);
    }
    gl.uniform1i(this.at.uAtlas, 0);
    gl.uniform1f(this.at.uLayer, layer);
  }

  protected after(gl: WebGL2RenderingContext) {
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, null);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteTexture(this.atlas);
    super.dispose(gl);
  }
}
