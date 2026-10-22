// Instance and Fold (decision 0057 §4c), the picture modules that read points (a Grid's: points-pictures.ts).
// Instance: its picture drawn once at each point, one instanced draw of quads (each a cell of the points' spacing, the
// picture's middle at that size), kept where the brighter (MAX) into a cleared target, by its amount. Fold: the
// picture folded by a symmetry (FOLDS: Hydra's repeat, kaleid and pixelate's maths, and a mirror), about its middle,
// or with a grid about each pixel's nearest point (a Voronoi mosaic). Mali rules: level-0 reads, no arrays (the
// points in a texture, read one by one).
import { MOST, packed, spacingOf, type Point } from '../points-pictures';
import { program } from '../sources/gl';
import { toScreen } from '../transform';
import type { ModuleNode } from './modules';
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
uniform highp sampler2D uPoints;
uniform int uCount;
uniform int uMode;
uniform float uN;
uniform float uTurn;
uniform float uAmount;
uniform vec2 uK;
out vec4 o;
// Itself within -1..1, mirrored back into it beyond (fx_repeat's).
float fold1(float x) { return 1.0 - abs(mod(x + 1.0, 4.0) - 2.0); }
vec2 turned(vec2 v, float a) { float c = cos(a), s = sin(a); return vec2(c * v.x - s * v.y, s * v.x + c * v.y); }
vec4 at(vec2 sq) {
  vec2 uv = sq * uK * 0.5 + 0.5;
  return textureLod(uIn, 1.0 - abs(1.0 - mod(uv, 2.0)), 0.0);
}
void main() {
  vec2 p = (vUv * 2.0 - 1.0) / uK;
  // The fold's centre: the middle, or the nearest of the grid's points.
  vec2 c = vec2(0.0);
  float d = 1e9;
  for (int i = 0; i < ${MOST}; i++) {
    if (i >= uCount) break;
    vec2 q = texelFetch(uPoints, ivec2(i, 0), 0).xy;
    vec2 e = p - q;
    if (dot(e, e) < d) { d = dot(e, e); c = q; }
  }
  bool grid = uCount > 0;
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
  } else {
    // Pixelate: Count cells across the short side (pixelate's); with a grid, each cell its point's colour.
    float cells = max(1.0, uN) * 0.5;
    f = grid ? vec2(0.0) : (floor(q * cells) + 0.5) / cells;
  }
  o = mix(at(p), at(base + turned(f, uTurn)), uAmount);
}`;

type Draw = (gl: WebGL2RenderingContext, picture: WebGLTexture | null, out: WebGLFramebuffer, width: number, height: number, knobs: Record<string, number>, points: readonly Point[] | null) => void;

function node(gl: WebGL2RenderingContext, draw: Draw, dispose: () => void): ModuleNode {
  return {
    draw: (gl, inputs, out, width, height, knobs, _frame, points) => draw(gl, inputs.picture, out, width, height, knobs, points ?? null),
    dispose,
  };
}

/** Instance: its picture at each point. */
export function createInstance(gl: WebGL2RenderingContext): ModuleNode {
  const p = program(gl, QUAD, TILE);
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
  return node(
    gl,
    (gl, picture, out, width, height, knobs, points) => {
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      const n = packed(points, data);
      const amount = knobs.amount ?? 1;
      if (!n || !(amount > 0) || !picture) return;
      gl.useProgram(p);
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
    },
    () => {
      gl.deleteProgram(p);
      gl.deleteBuffer(buffer);
      gl.deleteVertexArray(vao);
    },
  );
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
    (gl, picture, out, width, height, knobs, points) => {
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
      gl.uniform1i(u('uPoints'), 1);
      gl.uniform1i(u('uCount'), n);
      gl.uniform1i(u('uMode'), Math.round(knobs.mode ?? 1));
      gl.uniform1f(u('uN'), knobs.count ?? 6);
      gl.uniform1f(u('uTurn'), 2 * Math.PI * (knobs.turn ?? 0));
      gl.uniform1f(u('uAmount'), knobs.amount ?? 1);
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

/** The module types drawn here. */
export const POINTS_NODES: Readonly<Record<string, (gl: WebGL2RenderingContext) => ModuleNode>> = { instance: createInstance, fold: createFold };
