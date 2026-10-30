// Instance and Fold (decision 0057 §4c), the picture modules that read points (a Grid's: points-pictures.ts).
// Instance: its picture drawn once at each point, one instanced draw of quads (each a cell of the points' spacing, the
// picture's middle at that size), kept where the brighter (MAX) into a cleared target, by its amount. Fold: the
// picture folded by a symmetry (FOLDS: Hydra's repeat, kaleid and pixelate's maths, and a mirror), about its middle,
// or with a grid about each pixel's nearest point (a Voronoi mosaic), each cell one of its windows' pictures. Mali rules: level-0 reads, no arrays (the
// points in a texture, read one by one).
import { MOST, packed, spacingOf, type Point } from '../points-pictures';
import { FIELD_GLSL, PLAIN, strengthsOf } from '../field';
import { program } from '../sources/gl';
import { toScreen } from '../transform';
import type { FlowField, ModuleNode } from './modules';
import { none } from './shared';

const QUAD = `#version 300 es
layout(location = 0) in vec2 aAt;
uniform vec2 uK;
uniform float uHalf;
out vec2 vUv;
void main() {
  vec2 c = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1)) * 2.0 - 1.0;
  vUv = c * uHalf * uK * 0.5 + 0.5;
  gl_Position = vec4((aAt + c * uHalf) * uK, 0.0, 1.0);
}`;

const TILE = `#version 300 es
precision highp float;
in vec2 vUv;
uniform sampler2D uIn;
uniform float uAmount;
out vec4 o;
void main() { o = textureLod(uIn, vUv, 0.0) * uAmount; }`;

const FULL_UV = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const FOLD = `#version 300 es
precision highp float;
in vec2 vUv;
uniform sampler2D uIn;
uniform sampler2D uIn2;
uniform sampler2D uIn3;
uniform int uWindows;
uniform highp sampler2D uPoints;
uniform int uCount;
uniform int uMode;
uniform float uN;
uniform float uTurn;
uniform float uAmount;
uniform float uZoom;
uniform float uEdge;
uniform vec2 uK;
out vec4 o;
// Itself within -1..1, mirrored back into it beyond (fx_repeat's).
float fold1(float x) { return 1.0 - abs(mod(x + 1.0, 4.0) - 2.0); }
vec2 turned(vec2 v, float a) { float c = cos(a), s = sin(a); return vec2(c * v.x - s * v.y, s * v.x + c * v.y); }
int window = 0;
vec4 at(vec2 sq) {
  vec2 uv = sq * uK * 0.5 + 0.5;
  uv = 1.0 - abs(1.0 - mod(uv, 2.0));
  if (window == 1) return textureLod(uIn2, uv, 0.0);
  if (window == 2) return textureLod(uIn3, uv, 0.0);
  return textureLod(uIn, uv, 0.0);
}
void main() {
  vec2 p = (vUv * 2.0 - 1.0) / uK;
  // The fold's centre: the middle, or the nearest of the grid's points (and how near the next, for the seams).
  vec2 c = vec2(0.0);
  float d = 1e9;
  float d2 = 1e9;
  int ci = 0;
  for (int i = 0; i < ${MOST}; i++) {
    if (i >= uCount) break;
    vec2 q = texelFetch(uPoints, ivec2(i, 0), 0).xy;
    vec2 e = p - q;
    float dd = dot(e, e);
    if (dd < d) { d2 = d; d = dd; c = q; ci = i; }
    else if (dd < d2) d2 = dd;
  }
  bool grid = uCount > 0;
  // Each cell's window: one of the pictures cabled in, as its point's number falls (the same cell, the same window).
  if (grid && uWindows > 1) window = int(floor(fract(sin(float(ci) * 12.9898 + 4.1) * 43758.5453) * float(uWindows)));
  vec2 q = turned(p - c, -uTurn);
  vec2 f = q;
  vec2 base = c;
  if (uMode == 0) {
    // Mosaic: the picture again and again, mirrored at each seam (repeat's); with a grid, its middle in every cell.
    if (grid) base = vec2(0.0);
    else { vec2 w = 1.0 / uK; f = vec2(w.x * fold1(q.x * uN / w.x), w.y * fold1(q.y * uN / w.y)); }
  } else if (uMode == 1) {
    // Kaleidoscope: Count sides (2 at least), each mirrored (kaleid's).
    float n = max(2.0, floor(uN));
    float r = length(q);
    float th = atan(q.y, q.x);
    float side = 6.28318530718 / n;
    th = mod(th, side);
    th = min(th, side - th);
    f = r * vec2(cos(th), sin(th));
  } else if (uMode == 2) {
    // Mirror: the one half over the other, across the line Turn sets.
    f = vec2(q.x, abs(q.y));
  } else if (uMode == 3) {
    // Pixelate: Count cells across the short side (pixelate's); with a grid, each cell its point's colour.
    float cells = max(1.0, uN) * 0.5;
    f = grid ? vec2(0.0) : (floor(q * cells) + 0.5) / cells;
  } else {
    // Cells: each cell its own part of the picture, about its point, Zoom times nearer (stained glass, a compound eye);
    // without a grid, the whole picture so.
    f = q / max(0.05, uZoom);
  }
  o = mix(at(p), at(base + turned(f, uTurn)), uAmount);
  // Dark seams between a grid's cells, Edge wide: where the nearest point and the next are about as near.
  if (grid && uEdge > 0.0) o.rgb *= smoothstep(0.0, uEdge, 0.5 * (sqrt(d2) - sqrt(d)));
}`;

type Draw = (gl: WebGL2RenderingContext, picture: WebGLTexture | null, out: WebGLFramebuffer, width: number, height: number, knobs: Record<string, number>, points: readonly Point[] | null, windows: readonly (WebGLTexture | null)[]) => void;

function node(gl: WebGL2RenderingContext, draw: Draw, dispose: () => void): ModuleNode {
  return {
    draw: (gl, inputs, out, width, height, knobs, _frame, points) => draw(gl, inputs.picture, out, width, height, knobs, points ?? null, inputs.windows ?? []),
    dispose,
  };
}

/** Instance: its picture at each point, each copy turned with the flow there (Align) and drawn longer as it goes faster
 *  (Stretch): a Field cabled in, else the Fluid's. */
export function createInstance(gl: WebGL2RenderingContext): ModuleNode {
  const p = program(gl, INSTANCE_QUAD, TILE);
  const data = new Float32Array(MOST * 2);
  const buffer = gl.createBuffer()!;
  const vao = gl.createVertexArray()!;
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data.byteLength, gl.DYNAMIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.vertexAttribDivisor(0, 1);
  gl.bindVertexArray(null);
  gl.bindBuffer(gl.ARRAY_BUFFER, null);
  return {
    draw: (gl, inputs, out, width, height, knobs, _frame, given) => {
      const picture = inputs.picture;
      const points = given ?? null;
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      const n = packed(points, data);
      const amount = knobs.amount ?? 1;
      if (!n || !(amount > 0) || !picture) return;
      gl.useProgram(p);
      const align = knobs.align ?? 0;
      const stretch = knobs.stretch ?? 0;
      const following = !!inputs.along && !!knobs.follow;
      const flowing = (!!inputs.field || following) && (align > 0 || stretch > 0);
      if (flowing) setFlow(gl, p, inputs.field ?? STILL, 1, width, height);
      const alongDone = flowing ? setAlong(gl, p, inputs.along, knobs, 2, width, height) : () => {};
      if (!flowing) gl.uniform1f(gl.getUniformLocation(p, 'uFollow'), 0);
      gl.uniform1i(gl.getUniformLocation(p, 'uFlowing'), flowing ? 1 : 0);
      gl.uniform1f(gl.getUniformLocation(p, 'uAlign'), align);
      gl.uniform1f(gl.getUniformLocation(p, 'uStretch'), stretch);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, data, 0, n * 2);
      gl.bindBuffer(gl.ARRAY_BUFFER, null);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, picture);
      gl.uniform1i(gl.getUniformLocation(p, 'uIn'), 0);
      gl.uniform2f(gl.getUniformLocation(p, 'uK'), ...toScreen(width / height));
      gl.uniform1f(gl.getUniformLocation(p, 'uHalf'), spacingOf(points!.slice(0, n)) / 2);
      gl.uniform1f(gl.getUniformLocation(p, 'uAmount'), amount);
      gl.enable(gl.BLEND);
      gl.blendEquation(gl.MAX);
      gl.bindVertexArray(vao);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, n);
      gl.bindVertexArray(null);
      gl.blendEquation(gl.FUNC_ADD);
      gl.disable(gl.BLEND);
      alongDone();
    },
    dispose: () => {
      gl.deleteProgram(p);
      gl.deleteBuffer(buffer);
      gl.deleteVertexArray(vao);
    },
  };
}

/** Fold: its picture folded by its mode, about the middle or each point of its grid. */
export function createFold(gl: WebGL2RenderingContext): ModuleNode {
  const p = program(gl, FULL_UV, FOLD);
  const data = new Float32Array(MOST * 2);
  const table = gl.createTexture()!;
  gl.bindTexture(gl.TEXTURE_2D, table);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RG32F, MOST, 1, 0, gl.RG, gl.FLOAT, data);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.bindTexture(gl.TEXTURE_2D, null);
  const vao = gl.createVertexArray()!;
  return node(
    gl,
    (gl, picture, out, width, height, knobs, points, windows) => {
      const n = packed(points, data);
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.disable(gl.BLEND);
      gl.useProgram(p);
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, table);
      if (n) gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, n, 1, gl.RG, gl.FLOAT, data, 0);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, picture ?? none(gl));
      const u = (name: string) => gl.getUniformLocation(p, name);
      gl.uniform1i(u('uIn'), 0);
      // The windows cabled in, in order: the first picture, then each further one there is.
      const more = windows.filter((w): w is WebGLTexture => !!w);
      more.forEach((w, i) => {
        gl.activeTexture(gl.TEXTURE2 + i);
        gl.bindTexture(gl.TEXTURE_2D, w);
      });
      for (let i = more.length; i < 2; i++) {
        gl.activeTexture(gl.TEXTURE2 + i);
        gl.bindTexture(gl.TEXTURE_2D, none(gl));
      }
      gl.activeTexture(gl.TEXTURE0);
      gl.uniform1i(u('uIn2'), 2);
      gl.uniform1i(u('uIn3'), 3);
      gl.uniform1i(u('uWindows'), 1 + more.length);
      gl.uniform1i(u('uPoints'), 1);
      gl.uniform1i(u('uCount'), n);
      gl.uniform1i(u('uMode'), Math.round(knobs.mode ?? 1));
      gl.uniform1f(u('uN'), knobs.count ?? 6);
      gl.uniform1f(u('uTurn'), 2 * Math.PI * (knobs.turn ?? 0));
      gl.uniform1f(u('uAmount'), knobs.amount ?? 1);
      gl.uniform1f(u('uZoom'), knobs.zoom ?? 1.5);
      gl.uniform1f(u('uEdge'), knobs.edge ?? 0);
      gl.uniform2f(u('uK'), ...toScreen(width / height));
      gl.bindVertexArray(vao);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.bindVertexArray(null);
    },
    () => {
      gl.deleteProgram(p);
      gl.deleteTexture(table);
      gl.deleteVertexArray(vao);
    },
  );
}

// A flow as the shaders read it (`flowAt`, in the frame's units a second, centred, the short side -1..1): a Field's sum
// (uFormula 1) or the Fluid's velocity texture.
const FLOW_AT = `uniform sampler2D uVelocity;
uniform vec2 uUvPerCell;
uniform int uFormula;
uniform float uScale;
uniform float uPhase;
uniform vec3 uWavesA;
uniform vec3 uWavesB;
uniform vec2 uShape;
uniform vec2 uK;
// Along a picture's outlines (the owner: flows pushed along shapes): the swirl of its brightness, blurred to Reach (a
// level of its mipmaps), as strong as Follow; free of divergence as the Field's waves are (a stream function's curl).
uniform sampler2D uAlong;
uniform float uFollow;
uniform float uReach;
uniform vec2 uAlongTexel;
${FIELD_GLSL}
float brightAt(vec2 p) { return dot(textureLod(uAlong, p * uK * 0.5 + 0.5, uReach).rgb, vec3(0.299, 0.587, 0.114)); }
vec2 flowAt(vec2 p) {
  vec2 v;
  if (uFormula == 1) v = fieldAt(p, uScale, uPhase, uWavesA, uWavesB, uShape);
  else v = textureLod(uVelocity, p * uK * 0.5 + 0.5, 0.0).xy * uUvPerCell * 2.0 / uK;
  if (uFollow != 0.0) {
    // A step of the blurred level's texel, in the frame's units.
    vec2 h = exp2(uReach) * uAlongTexel * 2.0 / uK;
    float dx = brightAt(p + vec2(h.x, 0.0)) - brightAt(p - vec2(h.x, 0.0));
    float dy = brightAt(p + vec2(0.0, h.y)) - brightAt(p - vec2(0.0, h.y));
    v += vec2(dy, -dx) * uFollow;
  }
  return v;
}`;

const ONE_WAVES = [1, 1, 1, 1, 1, 1];
/** No flow of its own (a module flowing only along a picture): the Field's sum with every wave at nothing. */
export const STILL: FlowField = { scale: 1, phase: 0, waves: [0, 0, 0, 0, 0, 0] };

/** Along a picture's outlines: its texture (its mipmaps made now, read from Reach's level; the picture at the frame's
 *  size) on `unit`, Follow and Reach set; none, Follow at 0. What it gives back puts the texture's own filter back. */
function setAlong(gl: WebGL2RenderingContext, p: WebGLProgram, along: WebGLTexture | null | undefined, knobs: Record<string, number>, unit: number, width: number, height: number): () => void {
  const u = (name: string) => gl.getUniformLocation(p, name);
  const follow = along ? (knobs.follow ?? 0) : 0;
  gl.activeTexture(gl.TEXTURE0 + unit);
  gl.bindTexture(gl.TEXTURE_2D, follow ? along! : none(gl));
  if (follow) {
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  }
  gl.uniform1i(u('uAlong'), unit);
  gl.uniform1f(u('uFollow'), follow);
  gl.uniform1f(u('uReach'), knobs.reach ?? 3);
  gl.uniform2f(u('uAlongTexel'), 1 / Math.max(1, width), 1 / Math.max(1, height));
  gl.activeTexture(gl.TEXTURE0);
  return () => {
    if (!follow) return;
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, along!);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.activeTexture(gl.TEXTURE0);
  };
}

// Instance's quads: each a cell of the points' spacing about its point, the picture's middle at that size in it; with a
// flow, turned the way it goes there (as far as Align takes it) and drawn longer along it as fast as it goes (Stretch).
const INSTANCE_QUAD = `#version 300 es
precision highp float;
layout(location = 0) in vec2 aAt;
uniform float uHalf;
uniform float uAlign;
uniform float uStretch;
uniform int uFlowing;
${FLOW_AT}
out vec2 vUv;
void main() {
  vec2 c = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1)) * 2.0 - 1.0;
  vUv = c * uHalf * uK * 0.5 + 0.5;
  vec2 q = c * uHalf;
  if (uFlowing == 1) {
    vec2 v = flowAt(aAt);
    float sp = length(v);
    q.x *= 1.0 + uStretch * sp;
    float a = sp > 0.00001 ? uAlign * atan(v.y, v.x) : 0.0;
    q = vec2(cos(a) * q.x - sin(a) * q.y, sin(a) * q.x + cos(a) * q.y);
  }
  gl_Position = vec4((aAt + q) * uK, 0.0, 1.0);
}`;

// Shape (the owner: shapes over flow grids): one shape about the frame's middle by its signed distance (a circle, a
// polygon of Sides, a star of Sides points, a ring, a line), Size its radius in the frame's units (the short side -1..1),
// turned by Turn, Width a ring's or line's thickness and a star's depth (of Size), its edge Softness soft; in a colour.
const SHAPE = `#version 300 es
precision highp float;
in vec2 vUv;
uniform vec2 uK;
uniform int uKind;
uniform float uSides;
uniform float uSize;
uniform float uTurn;
uniform float uWidth;
uniform float uSoft;
uniform vec3 uColour;
out vec4 o;
const float PI = 3.14159265;
void main() {
  vec2 p = (vUv * 2.0 - 1.0) / uK;
  float c = cos(uTurn), s = sin(uTurn);
  p = vec2(c * p.x + s * p.y, -s * p.x + c * p.y);
  float n = max(3.0, floor(uSides + 0.5));
  float r = uSize;
  float an = PI / n;
  float b = mod(atan(p.y, p.x) + an, 2.0 * an) - an;
  float d;
  if (uKind == 0) d = length(p) - r;
  else if (uKind == 1) d = length(p) * cos(b) - r * cos(an);
  else if (uKind == 2) d = length(p) - r * mix(clamp(1.0 - uWidth * 3.0, 0.15, 0.95), 1.0, 1.0 - abs(b) / an);
  else if (uKind == 3) d = abs(length(p) - r) - uWidth * r;
  else d = length(p - vec2(clamp(p.x, -r, r), 0.0)) - uWidth * r;
  float edge = max(fwidth(d), uSoft * r);
  float a = 1.0 - smoothstep(-edge, edge, d);
  o = vec4(uColour * a, a);
}`;

/** A hue, its saturation and a brightness as RGB (the shaders' hsv: white at saturation 0). */
const colourOf = (hue: number, sat: number, bright: number): [number, number, number] =>
  [0, 4, 2].map((o) => {
    const c = Math.min(1, Math.max(0, Math.abs((((hue * 6 + o) % 6) + 6) % 6 - 3) - 1));
    return (1 + (c - 1) * sat) * bright;
  }) as [number, number, number];

/** Shape: one shape, a picture of its own. */
export function createShape(gl: WebGL2RenderingContext): ModuleNode {
  const p = program(gl, FULL_UV, SHAPE);
  const vao = gl.createVertexArray()!;
  return {
    draw: (gl, _inputs, out, width, height, knobs) => {
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.disable(gl.BLEND);
      gl.useProgram(p);
      const u = (name: string) => gl.getUniformLocation(p, name);
      gl.uniform2f(u('uK'), ...toScreen(width / height));
      gl.uniform1i(u('uKind'), Math.round(knobs.kind ?? 1));
      gl.uniform1f(u('uSides'), knobs.sides ?? 6);
      gl.uniform1f(u('uSize'), knobs.size ?? 0.1);
      gl.uniform1f(u('uTurn'), Math.PI * (knobs.turn ?? 0));
      gl.uniform1f(u('uWidth'), knobs.width ?? 0.15);
      gl.uniform1f(u('uSoft'), knobs.softness ?? 0.05);
      gl.uniform3f(u('uColour'), ...colourOf(knobs.hue ?? 0.55, knobs.saturation ?? 0.4, knobs.brightness ?? 1));
      gl.bindVertexArray(vao);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.bindVertexArray(null);
    },
    dispose: () => {
      gl.deleteProgram(p);
      gl.deleteVertexArray(vao);
    },
  };
}

/** The flow's uniforms for a program with FLOW_AT, its texture on unit `unit`. */
function setFlow(gl: WebGL2RenderingContext, p: WebGLProgram, field: FlowField, unit: number, width: number, height: number) {
  const u = (name: string) => gl.getUniformLocation(p, name);
  const formula = 'scale' in field;
  gl.activeTexture(gl.TEXTURE0 + unit);
  gl.bindTexture(gl.TEXTURE_2D, formula ? none(gl) : field.tex);
  gl.uniform1i(u('uVelocity'), unit);
  gl.uniform2f(u('uUvPerCell'), ...(formula ? ([1, 1] as [number, number]) : field.uvPerCell));
  gl.uniform1i(u('uFormula'), formula ? 1 : 0);
  gl.uniform1f(u('uScale'), formula ? field.scale : 1);
  gl.uniform1f(u('uPhase'), formula ? field.phase : 0);
  const w = formula ? strengthsOf(field.waves, field.shape ?? PLAIN) : ONE_WAVES;
  gl.uniform2f(u('uShape'), ...((formula ? [(field.shape ?? PLAIN).range, (field.shape ?? PLAIN).variety] : [PLAIN.range, 0]) as [number, number]));
  gl.uniform3f(u('uWavesA'), w[0], w[1], w[2]);
  gl.uniform3f(u('uWavesB'), w[3], w[4], w[5]);
  gl.uniform2f(u('uK'), ...toScreen(width / height));
}

// Streamlines (the owner's evolving flow field: a field, curves traced through it from seed points, drawn anew each
// frame; not a feedback warp): each seed a ribbon of STEPS steps, its vertices walked along the flow's direction from
// the seed in the vertex shader (Euler, the step its Length over STEPS), widened by Thickness across the way it goes,
// tapered at both ends, faded where the flow is calm; added together onto black.
const STEPS = 48;
const LINES = `#version 300 es
precision highp float;
precision highp int;
layout(location = 0) in vec2 aSeed;
${FLOW_AT}
uniform float uSat;
uniform float uStep;
uniform float uHalf;
uniform float uCalm;
uniform float uHue;
uniform float uSpread;
uniform float uLines;
// Clusters: the lines kept where a slow giant mask (field.ts's Variety's) is high, faded where it is low, each line's own
// threshold (its number's hash) so the edge of a clump is ragged, not a line.
uniform float uClusters;
uniform float uTime;
out vec4 vColour;
vec3 hsv(float h) { return clamp(abs(mod(h * 6.0 + vec3(0.0, 4.0, 2.0), 6.0) - 3.0) - 1.0, 0.0, 1.0); }
void main() {
  int s = gl_VertexID >> 1;
  float side = (gl_VertexID & 1) == 0 ? -1.0 : 1.0;
  vec2 p = aSeed;
  vec2 v = flowAt(p);
  for (int j = 0; j < ${STEPS}; j++) {
    if (j >= s) break;
    float speed = length(v);
    if (speed < 1e-4) break;
    p += v / speed * uStep;
    v = flowAt(p);
  }
  float speed = length(v);
  vec2 t = speed > 1e-4 ? v / speed : vec2(1.0, 0.0);
  vec2 at = p + vec2(-t.y, t.x) * side * uHalf;
  gl_Position = vec4(at * uK, 0.0, 1.0);
  float along = float(s) / float(${STEPS - 1});
  float a = sin(3.14159265 * along) * smoothstep(0.0, uCalm, speed);
  if (uClusters > 0.0) {
    float m = 0.5 + 0.5 * sin(2.1 * aSeed.x + 0.05 * uTime) * sin(1.7 * aSeed.y + 1.3 - 0.04 * uTime);
    float h = fract(sin(float(gl_InstanceID) * 12.9898 + 78.233) * 43758.5453);
    a *= mix(1.0, smoothstep(h - 0.15, h + 0.15, m), uClusters);
  }
  vColour = vec4(mix(vec3(1.0), hsv(fract(uHue + uSpread * float(gl_InstanceID) / max(1.0, uLines))), uSat) * a, 1.0);
}`;
const LINE = `#version 300 es
precision highp float;
in vec4 vColour;
uniform float uBright;
out vec4 o;
void main() { o = vec4(vColour.rgb * uBright, 1.0); }`;

/** The seeds where none are cabled: an even 8 x 8 over the short side's square. */
const SCATTER: readonly Point[] = Array.from({ length: 64 }, (_, i) => ({ x: ((i % 8) + 0.5) / 4 - 1, y: (Math.floor(i / 8) + 0.5) / 4 - 1 }));

/** The most lines Streamlines draws: a full Grid's seeds, each its Per seed's most (instanced, so no texture's limit). */
const MOST_LINES = MOST * 16;

/** Streamlines: curves through the flow field from its seeds (a Grid's; else an even scatter), drawn anew each frame. */
export function createStreamlines(gl: WebGL2RenderingContext): ModuleNode {
  const p = program(gl, LINES, LINE);
  const data = new Float32Array(MOST_LINES * 2);
  const buffer = gl.createBuffer()!;
  const vao = gl.createVertexArray()!;
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data.byteLength, gl.DYNAMIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.vertexAttribDivisor(0, 1);
  gl.bindVertexArray(null);
  gl.bindBuffer(gl.ARRAY_BUFFER, null);
  return {
    draw: (gl, inputs, out, width, height, knobs, frame, points) => {
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      const field = inputs.field;
      // Per seed, that many lines on a ring round it (a few seeds, as a Flame's six parts, made many).
      const given = points && points.length ? points : SCATTER;
      const per = Math.max(1, Math.round(knobs.perSeed ?? 1));
      const ring = knobs.ring ?? 0.08;
      const seeds = per === 1 ? given : given.flatMap((q) => Array.from({ length: per }, (_, i) => ({ x: q.x + ring * Math.cos((2 * Math.PI * i) / per), y: q.y + ring * Math.sin((2 * Math.PI * i) / per) })));
      const n = packed(seeds, data, MOST_LINES);
      const following = !!inputs.along && !!knobs.follow;
      const flow = field ?? (following ? STILL : null);
      if (!flow || !n) return;
      gl.useProgram(p);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, data, 0, n * 2);
      gl.bindBuffer(gl.ARRAY_BUFFER, null);
      const u = (name: string) => gl.getUniformLocation(p, name);
      setFlow(gl, p, flow, 0, width, height);
      const alongDone = setAlong(gl, p, inputs.along, knobs, 1, width, height);
      gl.uniform1f(u('uSat'), knobs.saturation ?? 0);
      gl.uniform1f(u('uStep'), (knobs.length ?? 0.6) / STEPS);
      gl.uniform1f(u('uHalf'), (knobs.thickness ?? 0.006) / 2);
      gl.uniform1f(u('uCalm'), knobs.calm ?? 0.05);
      gl.uniform1f(u('uHue'), knobs.hue ?? 0.55);
      gl.uniform1f(u('uSpread'), knobs.hueSpread ?? 0.3);
      gl.uniform1f(u('uLines'), n);
      gl.uniform1f(u('uBright'), knobs.brightness ?? 1);
      gl.uniform1f(u('uClusters'), knobs.clusters ?? 0);
      gl.uniform1f(u('uTime'), frame.time);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE);
      gl.bindVertexArray(vao);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, STEPS * 2, n);
      gl.bindVertexArray(null);
      gl.disable(gl.BLEND);
      alongDone();
    },
    dispose: () => {
      gl.deleteProgram(p);
      gl.deleteBuffer(buffer);
      gl.deleteVertexArray(vao);
    },
  };
}

const PUSH = `#version 300 es
precision highp float;
precision highp int;
in vec2 vUv;
uniform sampler2D uIn;
uniform float uAmount;
${FLOW_AT}
out vec4 o;
void main() {
  // Read from where the flow brought this pixel from: a few steps back along it.
  vec2 p = (vUv * 2.0 - 1.0) / uK;
  for (int i = 0; i < 8; i++) p -= flowAt(p) * uAmount / 8.0;
  o = textureLod(uIn, p * uK * 0.5 + 0.5, 0.0);
}`;

/** Push: its picture pushed along the flow (a Field's, else the Fluid's), as far as Amount. */
export function createPush(gl: WebGL2RenderingContext): ModuleNode {
  const p = program(gl, FULL_UV, PUSH);
  const vao = gl.createVertexArray()!;
  return {
    draw: (gl, inputs, out, width, height, knobs) => {
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.disable(gl.BLEND);
      gl.useProgram(p);
      const following = !!inputs.along && !!knobs.follow;
      setFlow(gl, p, inputs.field ?? (following ? STILL : { scale: 1, phase: 0 }), 1, width, height);
      const alongDone = setAlong(gl, p, inputs.along, knobs, 2, width, height);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, inputs.picture ?? none(gl));
      gl.uniform1i(gl.getUniformLocation(p, 'uIn'), 0);
      gl.uniform1f(gl.getUniformLocation(p, 'uAmount'), inputs.field || following ? (knobs.amount ?? 0.3) : 0);
      gl.bindVertexArray(vao);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.bindVertexArray(null);
      alongDone();
    },
    dispose: () => {
      gl.deleteProgram(p);
      gl.deleteVertexArray(vao);
    },
  };
}

/** The module types drawn here. */
export const POINTS_NODES: Readonly<Record<string, (gl: WebGL2RenderingContext) => ModuleNode>> = { instance: createInstance, fold: createFold, streamlines: createStreamlines, push: createPush, shape: createShape };
