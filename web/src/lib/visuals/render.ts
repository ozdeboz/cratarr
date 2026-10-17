// Drawing fractal flames on the GPU (decision 0046; the flames themselves are lib/visuals/flame.ts), in WebGL2:
// - points run the chaos game in a vertex shader, one step per pass, their state (position, colour coordinate, random
//   seed) carried from pass to pass by transform feedback;
// - each pass also draws them, one pixel each, adding their palette colour into a half-float buffer that fades a
//   little every frame: the picture is the last second or so of points, which gives the many samples a flame needs
//   without drawing them all every frame, and a little motion blur;
// - the screen shows that buffer by the paper's log-density display: brightness from the log of how many points fell
//   on a pixel, colour from their average, then gamma and vibrancy (its section 4), with a little blur where few fell.
import { paletteAt, type Flame } from './flame';

const MAX_XFORMS = 12;
const SLOTS = 8;

const ITERATE = `#version 300 es
precision highp float;
precision highp int;
layout(location = 0) in vec3 aState;
layout(location = 1) in uint aSeed;
out vec3 vState;
flat out uint vSeed;
out vec3 vColor;
uniform int uCount;
uniform vec3 uAbc[${MAX_XFORMS}];
uniform vec3 uDef[${MAX_XFORMS}];
uniform vec4 uMeta[${MAX_XFORMS}];
uniform ivec4 uVarId[${MAX_XFORMS * 2}];
uniform vec4 uVarW[${MAX_XFORMS * 2}];
uniform vec4 uCamera;
uniform vec2 uTurn;
uniform sampler2D uPalette;
const float PI = 3.14159265358979;

uint next(inout uint s) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; }
float rnd(inout uint s) { return float(next(s) >> 8) * (1.0 / 16777216.0); }

vec2 variation(int v, vec2 p, float r2, float r, float th, inout uint s) {
  switch (v) {
    case 1: return sin(p);
    case 2: return p / r2;
    case 3: return vec2(p.x * sin(r2) - p.y * cos(r2), p.x * cos(r2) + p.y * sin(r2));
    case 4: return vec2((p.x - p.y) * (p.x + p.y), 2.0 * p.x * p.y) / r;
    case 5: return vec2(th / PI, r - 1.0);
    case 6: return r * vec2(sin(th + r), cos(th - r));
    case 7: return r * vec2(sin(th * r), -cos(th * r));
    case 8: return (th / PI) * vec2(sin(PI * r), cos(PI * r));
    case 9: return vec2(cos(th) + sin(r), sin(th) - cos(r)) / r;
    case 10: return vec2(sin(th) / r, r * cos(th));
    case 11: return vec2(sin(th) * cos(r), cos(th) * sin(r));
    case 12: {
      float p0 = pow(sin(th + r), 3.0);
      float p1 = pow(cos(th - r), 3.0);
      return r * vec2(p0 + p1, p0 - p1);
    }
    case 13: {
      float om = rnd(s) < 0.5 ? 0.0 : PI;
      return sqrt(r) * vec2(cos(th * 0.5 + om), sin(th * 0.5 + om));
    }
    case 14: return vec2(p.x < 0.0 ? 2.0 * p.x : p.x, p.y < 0.0 ? p.y * 0.5 : p.y);
    case 16: return (2.0 / (r + 1.0)) * p.yx;
    case 18: return exp(p.x - 1.0) * vec2(cos(PI * p.y), sin(PI * p.y));
    case 19: return pow(r, sin(th)) * vec2(cos(th), sin(th));
    case 20: return vec2(cos(PI * p.x) * cosh(p.y), -sin(PI * p.x) * sinh(p.y));
    case 27: return (2.0 / (r + 1.0)) * p;
    case 28: return (4.0 / (r2 + 4.0)) * p;
    case 29: return vec2(sin(p.x), p.y);
    case 42: return vec2(sin(p.x) / cos(p.y), tan(p.y));
    case 48: return p / abs(p.x * p.x - p.y * p.y + 1e-12);
    default: return p;
  }
}

void main() {
  uint s = aSeed;
  // The transform, by the running sums of their weights in uMeta.x.
  float u = rnd(s);
  int i = uCount - 1;
  for (int k = 0; k < ${MAX_XFORMS}; k++) {
    if (k >= uCount) break;
    if (u < uMeta[k].x) { i = k; break; }
  }
  vec3 h = vec3(aState.xy, 1.0);
  vec2 q = vec2(dot(uAbc[i], h), dot(uDef[i], h));
  float r2 = dot(q, q) + 1e-12;
  float r = sqrt(r2);
  float th = atan(q.x, q.y);
  vec2 p = vec2(0.0);
  for (int k = 0; k < ${SLOTS}; k++) {
    float w = uVarW[i * 2 + k / 4][k % 4];
    if (w == 0.0) continue;
    p += w * variation(uVarId[i * 2 + k / 4][k % 4], q, r2, r, th, s);
  }
  float c = mix(aState.z, uMeta[i].y, uMeta[i].z);
  bool lost = any(isnan(p)) || any(isinf(p)) || abs(p.x) > 1e5 || abs(p.y) > 1e5;
  if (lost) {
    // Back in at random; not drawn this time, as flam3 lets a new point settle before plotting it.
    p = vec2(rnd(s), rnd(s)) * 2.0 - 1.0;
    c = rnd(s);
  }
  vState = vec3(p, c);
  vSeed = s;
  vColor = texture(uPalette, vec2(c, 0.5)).rgb;
  // The camera: centred, the flame's size to the shorter side, turned slowly as a whole.
  vec2 v = (p - uCamera.xy) * uCamera.z;
  v = vec2(uTurn.x * v.x - uTurn.y * v.y, uTurn.y * v.x + uTurn.x * v.y);
  if (uCamera.w > 1.0) v.x /= uCamera.w; else v.y *= uCamera.w;
  gl_Position = lost ? vec4(2.0, 2.0, 0.0, 1.0) : vec4(v, 0.0, 1.0);
  gl_PointSize = 1.0;
}`;

const PLOT = `#version 300 es
precision mediump float;
in vec3 vColor;
out vec4 o;
void main() { o = vec4(vColor, 1.0); }`;

const FULL = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

const FADE = `#version 300 es
precision mediump float;
uniform float uKeep;
out vec4 o;
void main() { o = vec4(0.0, 0.0, 0.0, uKeep); }`;

const SHOW = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform vec2 uView;
uniform float uK1;
uniform float uK2;
uniform float uGamma;
uniform float uVibrancy;
out vec4 o;
void main() {
  vec2 uv = gl_FragCoord.xy / uView;
  vec2 px = 1.0 / vec2(textureSize(uAcc, 0));
  vec4 a = texture(uAcc, uv);
  // The paper's density estimation, cheaply: where few points fell, the pixel is the mean of a wider ring around it
  // (up to three pixels out), where many fell it stays sharp.
  float radius = 3.0 / (1.0 + sqrt(a.a * uK2) * 2.0);
  if (radius > 0.4) {
    vec2 d = px * radius;
    vec4 ring = texture(uAcc, uv + vec2(d.x, 0.0)) + texture(uAcc, uv - vec2(d.x, 0.0))
              + texture(uAcc, uv + vec2(0.0, d.y)) + texture(uAcc, uv - vec2(0.0, d.y))
              + texture(uAcc, uv + d * 0.7071) + texture(uAcc, uv - d * 0.7071)
              + texture(uAcc, uv + vec2(d.x, -d.y) * 0.7071) + texture(uAcc, uv + vec2(-d.x, d.y) * 0.7071);
    a = (a + ring) / 9.0;
  }
  if (a.a <= 0.0) { o = vec4(0.0, 0.0, 0.0, 1.0); return; }
  float ls = uK1 * log(1.0 + a.a * uK2) / a.a;
  vec3 c = a.rgb * ls;
  float alpha = a.a * ls;
  // flam3's gamma threshold: below it the curve is a straight line, so the faintest pixels are not lifted into noise.
  const float T = 0.02;
  float g = alpha < T ? alpha / T * pow(T, 1.0 / uGamma) : pow(min(alpha, 1.0), 1.0 / uGamma);
  vec3 vib = c * (g / max(alpha, 1e-6));
  vec3 flat_ = pow(max(c, 0.0), vec3(1.0 / uGamma));
  o = vec4(clamp(mix(flat_, vib, uVibrancy), 0.0, 1.0), 1.0);
}`;

export type Shown = {
  /** Brightness, around 1. */
  brightness: number;
  /** How much of the picture stays from one frame to the next (0..1). */
  keep: number;
  gamma: number;
  vibrancy: number;
  /** The whole picture's turn, radians. */
  turn: number;
};

export class FlameRenderer {
  private gl: WebGL2RenderingContext;
  private iterate: WebGLProgram;
  private plot: WebGLProgram;
  private fade: WebGLProgram;
  private show: WebGLProgram;
  private buffers: WebGLBuffer[] = [];
  private vaos: WebGLVertexArrayObject[] = [];
  private feedback: WebGLTransformFeedback;
  private acc: WebGLTexture | null = null;
  private accFbo: WebGLFramebuffer | null = null;
  private palette: WebGLTexture;
  private empty: WebGLVertexArrayObject;
  private flip = 0;
  private count = 0;
  private flame: Flame | null = null;
  private paletteKey = '';
  private loc = new Map<string, WebGLUniformLocation | null>();
  /** The buffer's kind: 32-bit floats where they can be blended into, else 16-bit (a pixel stops counting near 2048
   *  points, only in the densest cores, where the log display hardly shows it), else 8 bits (saturating soon). */
  precision: 32 | 16 | 8;
  get halfFloat(): boolean {
    return this.precision !== 8;
  }
  /** Steps every point takes unseen before it is drawn (flam3's fuse): a new point is not on the flame yet. */
  private fuse = 20;
  width = 0;
  height = 0;

  /** `points`: how many carry the chaos game at first. */
  constructor(canvas: HTMLCanvasElement, points: number) {
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, preserveDrawingBuffer: false });
    if (!gl) throw new Error('WebGL2 is not available.');
    this.gl = gl;
    const floats = !!gl.getExtension('EXT_color_buffer_float');
    this.precision = floats && gl.getExtension('EXT_float_blend') ? 32 : floats || gl.getExtension('EXT_color_buffer_half_float') ? 16 : 8;
    this.iterate = program(gl, ITERATE, PLOT, ['vState', 'vSeed']);
    this.plot = this.iterate;
    this.fade = program(gl, FULL, FADE);
    this.show = program(gl, FULL, SHOW);
    this.empty = gl.createVertexArray()!;
    this.feedback = gl.createTransformFeedback()!;
    this.palette = gl.createTexture()!;
    this.seed(points);
  }

  /** How many points carry the chaos game: fewer where the device cannot keep up (they start again at random). */
  setPoints(n: number) {
    if (n === this.count) return;
    const gl = this.gl;
    for (const b of this.buffers) gl.deleteBuffer(b);
    for (const v of this.vaos) gl.deleteVertexArray(v);
    this.buffers = [];
    this.vaos = [];
    this.flip = 0;
    this.fuse = 20;
    this.seed(n);
  }

  get pointCount(): number {
    return this.count;
  }

  /** The points, at random in the square, each with its own seed. */
  private seed(n: number) {
    const gl = this.gl;
    const data = new ArrayBuffer(n * 16);
    const f = new Float32Array(data);
    const u = new Uint32Array(data);
    for (let i = 0; i < n; i++) {
      f[i * 4] = Math.random() * 2 - 1;
      f[i * 4 + 1] = Math.random() * 2 - 1;
      f[i * 4 + 2] = Math.random();
      u[i * 4 + 3] = (Math.random() * 0xfffffffe + 1) >>> 0;
    }
    for (let k = 0; k < 2; k++) {
      const b = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_COPY);
      const vao = gl.createVertexArray()!;
      gl.bindVertexArray(vao);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 16, 0);
      gl.enableVertexAttribArray(1);
      gl.vertexAttribIPointer(1, 1, gl.UNSIGNED_INT, 16, 12);
      this.buffers.push(b);
      this.vaos.push(vao);
    }
    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    this.count = n;
  }

  /** The drawing size in pixels (the buffer the points fall into; the screen scales it). */
  resize(width: number, height: number) {
    if (width === this.width && height === this.height) return;
    const gl = this.gl;
    this.width = width;
    this.height = height;
    if (this.acc) gl.deleteTexture(this.acc);
    if (this.accFbo) gl.deleteFramebuffer(this.accFbo);
    this.acc = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.acc);
    if (this.precision === 32) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, width, height, 0, gl.RGBA, gl.FLOAT, null);
    else if (this.precision === 16) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, width, height, 0, gl.RGBA, gl.HALF_FLOAT, null);
    else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    // 32-bit float textures filter only with an extension; without it the screen reads them nearest.
    const smooth = this.precision !== 32 || !!gl.getExtension('OES_texture_float_linear');
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, smooth ? gl.LINEAR : gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, smooth ? gl.LINEAR : gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.accFbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.accFbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.acc, 0);
    // A driver may still refuse the kind it offered: one step down, and again.
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE && this.precision !== 8) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      this.precision = this.precision === 32 ? 16 : 8;
      this.width = 0;
      this.resize(width, height);
      return;
    }
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** The flame to draw from now on (its palette as a texture, its transforms as uniforms). */
  setFlame(flame: Flame) {
    const gl = this.gl;
    this.flame = flame;
    // The palette again only when it changed (while two flames blend, every frame).
    const key = JSON.stringify(flame.palette);
    if (key !== this.paletteKey) {
      this.paletteKey = key;
      const pixels = new Uint8Array(256 * 4);
      for (let i = 0; i < 256; i++) {
        const [r, g, b] = paletteAt(flame.palette, i / 255);
        pixels.set([r * 255, g * 255, b * 255, 255], i * 4);
      }
      gl.bindTexture(gl.TEXTURE_2D, this.palette);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    }

    const xs = flame.xforms.slice(0, MAX_XFORMS);
    const total = xs.reduce((s, x) => s + x.weight, 0) || 1;
    const abc = new Float32Array(MAX_XFORMS * 3);
    const def = new Float32Array(MAX_XFORMS * 3);
    const meta = new Float32Array(MAX_XFORMS * 4);
    const ids = new Int32Array(MAX_XFORMS * SLOTS);
    const ws = new Float32Array(MAX_XFORMS * SLOTS);
    let run = 0;
    xs.forEach((x, i) => {
      const [a, b, c, d, e, f] = x.affine;
      abc.set([a, b, c], i * 3);
      def.set([d, e, f], i * 3);
      run += x.weight / total;
      meta.set([i === xs.length - 1 ? 1.0001 : run, x.color, x.colorSpeed, 0], i * 4);
      [...x.variations].slice(0, SLOTS).forEach(([v, w], k) => {
        ids[i * SLOTS + k] = v;
        ws[i * SLOTS + k] = w;
      });
    });
    gl.useProgram(this.iterate);
    gl.uniform1i(this.u(this.iterate, 'uCount'), xs.length);
    gl.uniform3fv(this.u(this.iterate, 'uAbc'), abc);
    gl.uniform3fv(this.u(this.iterate, 'uDef'), def);
    gl.uniform4fv(this.u(this.iterate, 'uMeta'), meta);
    gl.uniform4iv(this.u(this.iterate, 'uVarId'), ids);
    gl.uniform4fv(this.u(this.iterate, 'uVarW'), ws);
  }

  /** One frame: the old picture fades by `look.keep`, `passes` more steps of every point fall into it, and it shows. */
  frame(passes: number, look: Shown, viewWidth: number, viewHeight: number) {
    const gl = this.gl;
    const flame = this.flame;
    if (!flame || !this.acc || !this.accFbo) return;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.accFbo);
    gl.viewport(0, 0, this.width, this.height);
    gl.enable(gl.BLEND);

    // Fade what is there.
    gl.useProgram(this.fade);
    gl.uniform1f(this.u(this.fade, 'uKeep'), look.keep);
    gl.blendFunc(gl.ZERO, gl.SRC_ALPHA);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    // The points: each pass one step of the chaos game for every point, drawn where it lands.
    gl.useProgram(this.iterate);
    gl.uniform4f(this.u(this.iterate, 'uCamera'), flame.center[0], flame.center[1], 1 / flame.size, this.width / this.height);
    gl.uniform2f(this.u(this.iterate, 'uTurn'), Math.cos(look.turn), Math.sin(look.turn));
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.palette);
    gl.uniform1i(this.u(this.iterate, 'uPalette'), 0);
    gl.blendFunc(gl.ONE, gl.ONE);
    gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, this.feedback);
    const unseen = this.fuse;
    this.fuse = 0;
    if (unseen) gl.enable(gl.RASTERIZER_DISCARD);
    for (let p = 0; p < unseen + passes; p++) {
      if (p === unseen && unseen) gl.disable(gl.RASTERIZER_DISCARD);
      const from = this.flip;
      const to = 1 - from;
      gl.bindVertexArray(this.vaos[from]);
      gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER, 0, this.buffers[to]);
      gl.beginTransformFeedback(gl.POINTS);
      gl.drawArrays(gl.POINTS, 0, this.count);
      gl.endTransformFeedback();
      gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER, 0, null);
      this.flip = to;
    }
    gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, null);
    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);

    // Show it. The points held at a time: each frame's, kept 1/(1 − keep) frames; their mean per pixel sets k2.
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, viewWidth, viewHeight);
    gl.useProgram(this.show);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.acc);
    gl.uniform1i(this.u(this.show, 'uAcc'), 0);
    gl.uniform2f(this.u(this.show, 'uView'), viewWidth, viewHeight);
    const held = (this.count * passes) / Math.max(0.02, 1 - look.keep);
    const perPixel = held / (this.width * this.height);
    gl.uniform1f(this.u(this.show, 'uK1'), look.brightness);
    gl.uniform1f(this.u(this.show, 'uK2'), (this.precision === 8 ? 255 : 4) / perPixel);
    gl.uniform1f(this.u(this.show, 'uGamma'), look.gamma);
    gl.uniform1f(this.u(this.show, 'uVibrancy'), look.vibrancy);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** Whether the GPU took the context away (the page should stop and drop the renderer). */
  get lost(): boolean {
    return this.gl.isContextLost();
  }

  dispose() {
    const gl = this.gl;
    for (const b of this.buffers) gl.deleteBuffer(b);
    for (const v of this.vaos) gl.deleteVertexArray(v);
    gl.deleteVertexArray(this.empty);
    gl.deleteTransformFeedback(this.feedback);
    gl.deleteTexture(this.palette);
    if (this.acc) gl.deleteTexture(this.acc);
    if (this.accFbo) gl.deleteFramebuffer(this.accFbo);
    for (const p of new Set([this.iterate, this.plot, this.fade, this.show])) gl.deleteProgram(p);
    gl.getExtension('WEBGL_lose_context')?.loseContext();
  }

  private u(p: WebGLProgram, name: string): WebGLUniformLocation | null {
    const key = `${this.programs.indexOf(p)}:${name}`;
    if (!this.loc.has(key)) this.loc.set(key, this.gl.getUniformLocation(p, name));
    return this.loc.get(key) ?? null;
  }

  private get programs() {
    return [this.iterate, this.fade, this.show];
  }
}

function program(gl: WebGL2RenderingContext, vs: string, fs: string, feedback?: string[]): WebGLProgram {
  const shader = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'The shader did not compile.');
    return s;
  };
  const p = gl.createProgram()!;
  gl.attachShader(p, shader(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, shader(gl.FRAGMENT_SHADER, fs));
  if (feedback) gl.transformFeedbackVaryings(p, feedback, gl.INTERLEAVED_ATTRIBS);
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'The program did not link.');
  return p;
}
