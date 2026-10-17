// Drawing fractal flames on the GPU (decision 0046; the flames themselves are lib/visuals/flame.ts), in WebGL2:
// - points run the chaos game in a vertex shader, one step per pass, their state (position, colour: a place in the
//   colour plane, random seed) carried from pass to pass by transform feedback;
// - each pass also draws them, one pixel each, adding their colour into a half-float buffer that fades a
//   little every frame: the picture is the last second or so of points, which gives the many samples a flame needs
//   without drawing them all every frame, and a little motion blur;
// - the screen shows that buffer by the paper's log-density display: brightness from the log of how many points fell
//   on a pixel, colour from their average, then gamma and vibrancy (its section 4), with a little blur where few fell;
// - beside the colours, a second buffer counts how many of a pixel's points are of each of the music's bands' colours
//   (decision 0046, amended), so a band's transient lights its own colour where it lies;
// - every two seconds both buffers are copied over through a pass that clears anything not a number or infinite: a
//   fade only multiplies, so one such value, once in, would stay a black speck for good.
import { colourOf, type Flame } from './flame';

const MAX_XFORMS = 12;
const SLOTS = 8;

const ITERATE = `#version 300 es
precision highp float;
precision highp int;
layout(location = 0) in vec4 aState;
layout(location = 1) in uint aSeed;
out vec4 vState;
flat out uint vSeed;
out vec3 vColor;
out vec3 vBand;
uniform int uCount;
uniform vec3 uAbc[${MAX_XFORMS}];
uniform vec3 uDef[${MAX_XFORMS}];
uniform vec4 uMeta[${MAX_XFORMS}];
uniform ivec4 uVarId[${MAX_XFORMS * 2}];
uniform vec4 uVarW[${MAX_XFORMS * 2}];
uniform vec4 uCamera;
uniform vec2 uTurn;
// The flame's colours: one lightness and chroma (OKLCH); each band's transform's hue as a point on the unit circle.
uniform float uLight;
uniform float uChroma;
uniform vec2 uBandHue[3];
const float PI = 3.14159265358979;

// Not a number, or infinite: from the bits, which no compiler's fast arithmetic can assume away (isnan can be).
bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }

// OKLab to sRGB (each 0..1), as flame.ts's colourOf.
vec3 srgb(vec3 lab) {
  float l = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
  float m = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
  float s = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
  l = l * l * l;
  m = m * m * m;
  s = s * s * s;
  vec3 lin = vec3(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
                  -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
                  -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s);
  return pow(clamp(lin, 0.0, 1.0), vec3(1.0 / 2.2));
}

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
  // The colour moves towards the transform's hue (uMeta.zw) by its speed (uMeta.y).
  vec2 c = mix(aState.zw, uMeta[i].zw, uMeta[i].y);
  bool lost = bad(p.x) || bad(p.y) || abs(p.x) > 1e5 || abs(p.y) > 1e5;
  if (lost) {
    // Back in at random; not drawn this time, as flam3 lets a new point settle before plotting it.
    p = vec2(rnd(s), rnd(s)) * 2.0 - 1.0;
    c = vec2(0.0);
  }
  // A colour that is not a number would stay one, and in the picture for good: grey again.
  if (bad(c.x) || bad(c.y) || dot(c, c) > 4.0) c = vec2(0.0);
  vState = vec4(p, c);
  vSeed = s;
  vColor = srgb(vec3(uLight, uChroma * c));
  // How much of each band's colour the point is: by how near its colour is to each band's hue, shares of one.
  vec3 near = exp(-4.0 * vec3(dot(c - uBandHue[0], c - uBandHue[0]), dot(c - uBandHue[1], c - uBandHue[1]), dot(c - uBandHue[2], c - uBandHue[2])));
  vBand = near / max(near.x + near.y + near.z, 1e-6);
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
in vec3 vBand;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 band;
void main() { o = vec4(vColor, 1.0); band = vec4(vBand, 0.0); }`;

const FULL = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

const FADE = `#version 300 es
precision mediump float;
uniform float uKeep;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 band;
void main() { o = vec4(0.0, 0.0, 0.0, uKeep); band = o; }`;

// Not a number, or infinite, from the bits (see ITERATE's).
// A value not a number is nothing; an infinite one, the most a 16-bit float holds short of infinity.
const BAD = `bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }
float clean1(float x) {
  uint b = floatBitsToUint(x);
  if ((b & 0x7f800000u) != 0x7f800000u) return clamp(x, 0.0, 60000.0);
  return (b & 0x007fffffu) != 0u || (b & 0x80000000u) != 0u ? 0.0 : 60000.0;
}
vec4 clean(vec4 v) { return vec4(clean1(v.x), clean1(v.y), clean1(v.z), clean1(v.w)); }`;

// The scrub: both buffers copied, each value not a number or infinite made nothing (and the largest kept under what a
// 16-bit float holds).
const SCRUB = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform sampler2D uBands;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 band;
${BAD}
void main() {
  ivec2 at = ivec2(gl_FragCoord.xy);
  o = clean(texelFetch(uAcc, at, 0));
  band = clean(texelFetch(uBands, at, 0));
}`;

// Finds whether a buffer holds any such value: only those pixels pass (counted by an occlusion query).
const FIND = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform sampler2D uBands;
out vec4 o;
${BAD}
void main() {
  ivec2 at = ivec2(gl_FragCoord.xy);
  vec4 a = texelFetch(uAcc, at, 0);
  vec4 b = texelFetch(uBands, at, 0);
  if (!(bad(a.x) || bad(a.y) || bad(a.z) || bad(a.w) || bad(b.x) || bad(b.y) || bad(b.z) || bad(b.w))) discard;
  o = vec4(1.0);
}`;

const SHOW = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform vec2 uView;
uniform float uK1;
uniform float uK2;
uniform float uGamma;
uniform float uVibrancy;
uniform float uZoom;
uniform float uFlash;
uniform sampler2D uBands;
// Each band's transient now (0..1), and its colour.
uniform vec3 uHit;
uniform vec3 uBandRgb[3];
uniform float uCeiling;
uniform float uSaturation;
out vec4 o;
${BAD}
// A sample of the buffer, none of it anything but a number (whatever a driver did to it before the scrub came by).
vec4 at(vec2 uv) { return clean(texture(uAcc, uv)); }
void main() {
  // A beat's swell: the picture drawn a little larger about its middle.
  vec2 uv = 0.5 + (gl_FragCoord.xy / uView - 0.5) / uZoom;
  vec2 px = 1.0 / vec2(textureSize(uAcc, 0));
  vec4 a = at(uv);
  // The pixel's points by band: shares of one.
  vec3 share = clamp(clean(texture(uBands, uv)).rgb / max(a.a, 1e-6), 0.0, 1.0);
  // The paper's density estimation, cheaply: where few points fell, the pixel is the mean of a wider ring around it
  // (up to three pixels out), where many fell it stays sharp.
  float radius = 3.0 / (1.0 + sqrt(a.a * uK2) * 2.0);
  if (radius > 0.4) {
    vec2 d = px * radius;
    vec4 ring = at(uv + vec2(d.x, 0.0)) + at(uv - vec2(d.x, 0.0))
              + at(uv + vec2(0.0, d.y)) + at(uv - vec2(0.0, d.y))
              + at(uv + d * 0.7071) + at(uv - d * 0.7071)
              + at(uv + vec2(d.x, -d.y) * 0.7071) + at(uv + vec2(-d.x, d.y) * 0.7071);
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
  // A beat's flash, after the gamma, so it shows as much in the faint parts as in the bright ones; a band's transient
  // brightens the points of its colour, and adds a glow of that colour.
  vec3 hit = uHit * share;
  vec3 shown = mix(flat_, vib, uVibrancy) * uFlash * (1.0 + 0.6 * (hit.x + hit.y + hit.z));
  shown += 0.2 * g * (hit.x * uBandRgb[0] + hit.y * uBandRgb[1] + hit.z * uBandRgb[2]);
  // Less colour where the music is quiet (towards its grey of the same brightness).
  shown = mix(vec3(dot(shown, vec3(0.299, 0.587, 0.114))), shown, uSaturation);
  // The most brightness shown, last: no flash or light goes past it.
  o = vec4(clamp(shown, 0.0, 1.0) * uCeiling, 1.0);
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
  /** How large the flame is drawn: 1 as framed (smaller with more dark around it). */
  scale: number;
  /** How much larger it shows about its middle (a beat's swell): 1 for none, never less. */
  zoom: number;
  /** Brighter by this much as shown (a beat's flash): 1 for none. */
  flash: number;
  /** Each band's transient now, low to high (0..1): its colour's points light up. */
  hits: [number, number, number];
  /** The most brightness shown (0..1), applied after everything else. */
  ceiling: number;
  /** The display's exposure against the points' mean density (1: as the paper has it): the visuals' auto-exposure. */
  exposure: number;
  /** How much of its colour it shows (0: grey, 1: all). */
  saturation: number;
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
  /** The buffers, two sets: the one in use (`cur`) and the one the next scrub copies into. */
  private accs: WebGLTexture[] = [];
  private bandSets: WebGLTexture[] = [];
  private fbos: WebGLFramebuffer[] = [];
  private cur = 0;
  private scrubbing: WebGLProgram;
  private finding: WebGLProgram;
  private query: WebGLQuery | null = null;
  private frames = 0;
  /** How many scrubs found anything not a number or infinite in the buffers (the black specks' cause, if they come). */
  scrubbed = 0;
  private acc: WebGLTexture | null = null;
  /** Beside `acc`: each pixel's points by band. */
  private bands: WebGLTexture | null = null;
  private bandRgb = new Float32Array(9);
  private accFbo: WebGLFramebuffer | null = null;
  private empty: WebGLVertexArrayObject;
  private flip = 0;
  private count = 0;
  private flame: Flame | null = null;
  private loc = new Map<string, WebGLUniformLocation | null>();
  /** The buffer's kind: 32-bit floats where they can be blended into, else 16-bit (a pixel stops counting near 2048
   *  points, only in the densest cores, where the log display hardly shows it), else 8 bits (saturating soon). 16-bit
   *  ones were the default in preview 56 (4 to 20 % quicker): the densest pixels reach some 60,000 points (measured in
   *  32-bit ones, 2026-10-18), against a 16-bit float's 65,504, and a GPU whose blending rounds up (Direct3D allows it)
   *  carries them over into infinity, black for good: the owner's GTX 1650 showed such specks from 56 on, his tablet
   *  (16-bit, a GPU that rounds to nearest, stuck at 2048) never did. */
  precision: 32 | 16 | 8;
  get halfFloat(): boolean {
    return this.precision !== 8;
  }
  /** Steps every point takes unseen before it is drawn (flam3's fuse): a new point is not on the flame yet. */
  private fuse = 30;
  width = 0;
  height = 0;

  /** `points`: how many carry the chaos game at first. `power`: which GPU a computer with two should draw on (the
   *  fast one: by default a browser may take the one that saves power, several times slower). */
  constructor(canvas: HTMLCanvasElement, points: number, power: WebGLPowerPreference = 'high-performance') {
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, preserveDrawingBuffer: false, powerPreference: power });
    if (!gl) throw new Error('WebGL2 is not available.');
    this.gl = gl;
    const floats = !!gl.getExtension('EXT_color_buffer_float');
    this.precision = floats && gl.getExtension('EXT_float_blend') ? 32 : floats || gl.getExtension('EXT_color_buffer_half_float') ? 16 : 8;
    this.iterate = program(gl, ITERATE, PLOT, ['vState', 'vSeed']);
    this.plot = this.iterate;
    this.fade = program(gl, FULL, FADE);
    this.show = program(gl, FULL, SHOW);
    this.scrubbing = program(gl, FULL, SCRUB);
    this.finding = program(gl, FULL, FIND);
    this.empty = gl.createVertexArray()!;
    this.feedback = gl.createTransformFeedback()!;
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
    this.fuse = 30;
    this.seed(n);
  }

  get pointCount(): number {
    return this.count;
  }

  /** The points, at random in the square, each with its own seed. */
  private seed(n: number) {
    const gl = this.gl;
    // Five numbers a point: its place, its colour (grey to start with), its random seed.
    const data = new ArrayBuffer(n * 20);
    const f = new Float32Array(data);
    const u = new Uint32Array(data);
    for (let i = 0; i < n; i++) {
      f[i * 5] = Math.random() * 2 - 1;
      f[i * 5 + 1] = Math.random() * 2 - 1;
      u[i * 5 + 4] = (Math.random() * 0xfffffffe + 1) >>> 0;
    }
    for (let k = 0; k < 2; k++) {
      const b = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_COPY);
      const vao = gl.createVertexArray()!;
      gl.bindVertexArray(vao);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 20, 0);
      gl.enableVertexAttribArray(1);
      gl.vertexAttribIPointer(1, 1, gl.UNSIGNED_INT, 20, 16);
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
    for (const x of [...this.accs, ...this.bandSets]) gl.deleteTexture(x);
    for (const f of this.fbos) gl.deleteFramebuffer(f);
    this.accs = [];
    this.bandSets = [];
    this.fbos = [];
    // 32-bit float textures filter only with an extension; without it the screen reads them nearest.
    const smooth = this.precision !== 32 || !!gl.getExtension('OES_texture_float_linear');
    const texture = () => {
      const tex = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, tex);
      if (this.precision === 32) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, width, height, 0, gl.RGBA, gl.FLOAT, null);
      else if (this.precision === 16) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, width, height, 0, gl.RGBA, gl.HALF_FLOAT, null);
      else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, smooth ? gl.LINEAR : gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, smooth ? gl.LINEAR : gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return tex;
    };
    for (let k = 0; k < 2; k++) {
      this.accs.push(texture());
      this.bandSets.push(texture());
      const fbo = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.accs[k], 0);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT1, gl.TEXTURE_2D, this.bandSets[k], 0);
      gl.drawBuffers([gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1]);
      // A driver may still refuse the kind it offered: one step down, and again.
      if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE && this.precision !== 8) {
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.deleteFramebuffer(fbo);
        this.precision = this.precision === 32 ? 16 : 8;
        this.width = 0;
        this.resize(width, height);
        return;
      }
      this.fbos.push(fbo);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
    this.cur = 0;
    this.use();
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** The set in use as `acc`, `bands` and `accFbo`. */
  private use() {
    this.acc = this.accs[this.cur];
    this.bands = this.bandSets[this.cur];
    this.accFbo = this.fbos[this.cur];
  }

  /** Copies the buffers into the other set, clearing anything not a number or infinite; first asks the GPU whether
   *  there is any (answered a few frames later). */
  private scrub() {
    const gl = this.gl;
    const other = 1 - this.cur;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbos[other]);
    gl.viewport(0, 0, this.width, this.height);
    gl.bindVertexArray(this.empty);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.accs[this.cur]);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.bandSets[this.cur]);
    if (!this.query) {
      this.query = gl.createQuery();
      gl.useProgram(this.finding);
      gl.uniform1i(gl.getUniformLocation(this.finding, 'uAcc'), 0);
      gl.uniform1i(gl.getUniformLocation(this.finding, 'uBands'), 1);
      gl.colorMask(false, false, false, false);
      gl.beginQuery(gl.ANY_SAMPLES_PASSED_CONSERVATIVE, this.query!);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.endQuery(gl.ANY_SAMPLES_PASSED_CONSERVATIVE);
      gl.colorMask(true, true, true, true);
    }
    gl.useProgram(this.scrubbing);
    gl.uniform1i(gl.getUniformLocation(this.scrubbing, 'uAcc'), 0);
    gl.uniform1i(gl.getUniformLocation(this.scrubbing, 'uBands'), 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.activeTexture(gl.TEXTURE0);
    this.cur = other;
    this.use();
  }

  /** The answer to the last scrub's question, once the GPU has it. */
  private heard() {
    const gl = this.gl;
    if (!this.query || !gl.getQueryParameter(this.query, gl.QUERY_RESULT_AVAILABLE)) return;
    if (gl.getQueryParameter(this.query, gl.QUERY_RESULT)) this.scrubbed++;
    gl.deleteQuery(this.query);
    this.query = null;
  }

  /** The flame to draw from now on (its transforms and colours as uniforms). */
  setFlame(flame: Flame) {
    const gl = this.gl;
    this.flame = flame;
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
      meta.set([i === xs.length - 1 ? 1.0001 : run, x.colorSpeed, Math.cos(2 * Math.PI * x.color), Math.sin(2 * Math.PI * x.color)], i * 4);
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
    // The bands' transforms are the first three (flame.ts).
    const hues = [0, 1, 2].map((b) => xs[b]?.color ?? b / 3);
    gl.uniform1f(this.u(this.iterate, 'uLight'), flame.colours.light);
    gl.uniform1f(this.u(this.iterate, 'uChroma'), flame.colours.chroma);
    gl.uniform2fv(this.u(this.iterate, 'uBandHue'), hues.flatMap((h) => [Math.cos(2 * Math.PI * h), Math.sin(2 * Math.PI * h)]));
    hues.forEach((h, b) => this.bandRgb.set(colourOf(flame.colours.light, flame.colours.chroma, h), b * 3));
  }

  /** One frame: the old picture fades by `look.keep`, `passes` more steps of every point fall into it, and it shows. */
  frame(passes: number, look: Shown, viewWidth: number, viewHeight: number) {
    const gl = this.gl;
    const flame = this.flame;
    if (!flame || !this.acc || !this.accFbo) return;
    // Every 120 frames (two to four seconds), the scrub.
    this.heard();
    if (++this.frames % 120 === 0) this.scrub();
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
    gl.uniform4f(this.u(this.iterate, 'uCamera'), flame.center[0], flame.center[1], look.scale / flame.size, this.width / this.height);
    gl.uniform2f(this.u(this.iterate, 'uTurn'), Math.cos(look.turn), Math.sin(look.turn));
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
    gl.uniform1f(this.u(this.show, 'uK2'), ((this.precision === 8 ? 255 : 4) * look.exposure) / perPixel);
    gl.uniform1f(this.u(this.show, 'uGamma'), look.gamma);
    gl.uniform1f(this.u(this.show, 'uVibrancy'), look.vibrancy);
    gl.uniform1f(this.u(this.show, 'uZoom'), look.zoom);
    gl.uniform1f(this.u(this.show, 'uFlash'), look.flash);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.bands);
    gl.uniform1i(this.u(this.show, 'uBands'), 1);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform3f(this.u(this.show, 'uHit'), look.hits[0], look.hits[1], look.hits[2]);
    gl.uniform3fv(this.u(this.show, 'uBandRgb'), this.bandRgb);
    gl.uniform1f(this.u(this.show, 'uCeiling'), look.ceiling);
    gl.uniform1f(this.u(this.show, 'uSaturation'), look.saturation);
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
    for (const x of [...this.accs, ...this.bandSets]) gl.deleteTexture(x);
    for (const f of this.fbos) gl.deleteFramebuffer(f);
    if (this.query) gl.deleteQuery(this.query);
    for (const p of new Set([this.iterate, this.plot, this.fade, this.show, this.scrubbing, this.finding])) gl.deleteProgram(p);
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
