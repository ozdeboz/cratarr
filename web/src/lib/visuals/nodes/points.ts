// The flame's points (decision 0046; 0057 step 3, the Flame's own node): the chaos game in a vertex shader, one step
// a pass, each point's state carried by transform feedback and drawn into the light (light.ts) where it lands; the
// flame as uniforms; the dive's camera on them (zoom.ts).
import { LENS_SHAPE_VARIATIONS, MARK, colourOf, PARTS, type Flame } from '../flame';
import { FILL_ANGLES, LENS_SHAPES, fillTexture, type Irregular } from '../shape-set';
import { diveCamera, diveFit, diveTarget, type DiveFit } from '../zoom';
import { IDENTITY, type Affine } from '../transform';
import { program } from '../sources/gl';
import type { PointsParams } from '../stage-values';
import type { Light } from './light';
import { none, uniforms } from './shared';

// The six parts, rotations up to eight arms and as many mirrors.
const MAX_XFORMS = 22;
const SLOTS = 8;

// The Cratarr lens (flame.ts's `markOf`, `MARK`): a distance into a set of rings by an area share, written out ring by
// ring for the shader; then the point into the mark's slice or the rest by its angle.
const ringsGlsl = (name: string, rings: [number, number][]) => {
  const total = rings.reduce((s, [a, b]) => s + b * b - a * a, 0);
  const steps = rings.map(([a, b]) => {
    const area = b * b - a * a;
    return `  if (left < ${area.toFixed(6)}) return sqrt(${(a * a).toFixed(6)} + left);\n  left -= ${area.toFixed(6)};`;
  });
  return `float ${name}(float u) {\n  float left = clamp(u, 0.0, 1.0) * ${total.toFixed(6)};\n${steps.join('\n')}\n  return ${rings[rings.length - 1][1].toFixed(6)};\n}`;
};
const MARK_GLSL = `${ringsGlsl('markRings', MARK.rings)}
${ringsGlsl('markSlice', MARK.slice)}
vec2 mark(vec2 p, float r) {
  float t = (atan(p.y, p.x) + PI) / (2.0 * PI);
  float u = r / (1.0 + r);
  u *= u;
  float share = ${MARK.share.toFixed(6)};
  bool inSlice = t < share;
  float to = inSlice ? -PI / 6.0 + (t / share) * (PI / 3.0) : PI / 6.0 + ((t - share) / (1.0 - share)) * (5.0 * PI / 3.0);
  float far = inSlice ? markSlice(u) : markRings(u);
  return far * vec2(cos(to), sin(to));
}`;
// The Lens's shapes (0057 §4g; shape-set.ts `lensInto`): t from a point's angle and u from its distance, as the mark's.
// Polygon and Star by their sides and depth (shape-set.ts regularFill); the symbols by their fill tables (`fillTexture`:
// a row of the area through each wedge, two of its rings), read with texelFetch. No float arrays (the Mali's miscompile).
const TABLE_SHAPES = LENS_SHAPES.slice(2) as unknown as Irregular[];
const TABLE_FIRST = LENS_SHAPE_VARIATIONS[TABLE_SHAPES[0] as keyof typeof LENS_SHAPE_VARIATIONS];
/** The texture unit the fill tables are read from (one the light's passes do not use). */
const TABLE_UNIT = 7;
const LENS_GLSL = `vec2 lensTu(vec2 p, float r) {
  float u = r / (1.0 + r);
  return vec2((atan(p.y, p.x) + PI) / (2.0 * PI), u * u);
}
vec3 starSide(float n, float depth) {
  float h = PI / n;
  float rho = cos(h) * (1.0 - 0.95 * depth);
  vec2 nn = normalize(vec2(rho * sin(h), 1.0 - rho * cos(h)));
  return vec3(nn, nn.x);
}
float starAngle(float t, float n, float depth) {
  vec3 sd = starSide(n, depth);
  float h = PI / n;
  float a = atan(sd.y, sd.x);
  float ta = tan(a);
  float whole = tan(h - a) + ta;
  float x = clamp(t, 0.0, 1.0) * 2.0 * n;
  float k = min(2.0 * n - 1.0, floor(x));
  float tau = x - k;
  float at = floor(k * 0.5) * 2.0 * h;
  bool even = mod(k, 2.0) < 0.5;
  float psi = a + atan((even ? tau : 1.0 - tau) * whole - ta);
  return even ? at + psi : at + 2.0 * h - psi;
}
vec2 starReach(float th, float n, float depth) {
  vec3 sd = starSide(n, depth);
  float s = 2.0 * PI / n;
  float p = mod(th, s);
  p = p > 0.5 * s ? s - p : p;
  return vec2(sd.z, cos(p) * sd.x + sin(p) * sd.y);
}
vec2 regularFill(vec2 tu, float sides, float depth) {
  float n = max(3.0, sides);
  float n0 = floor(n);
  float f = n - n0;
  float th = mix(starAngle(tu.x, n0, depth), starAngle(tu.x, n0 + 1.0, depth), f);
  vec2 cd = mix(starReach(th, n0, depth), starReach(th, n0 + 1.0, depth), f);
  return (cd.x / cd.y) * sqrt(clamp(tu.y, 0.0, 1.0)) * vec2(cos(th), sin(th));
}
void ringStep(inout float left, inout float far, inout bool found, vec2 ring) {
  float area = ring.y * ring.y - ring.x * ring.x;
  if (found || area <= 0.0) return;
  if (left < area) {
    far = sqrt(ring.x * ring.x + left);
    found = true;
    return;
  }
  left -= area;
  far = ring.y;
}
vec2 tableFill(int k, vec2 tu) {
  int row = 3 * k;
  int lo = 0;
  int hi = ${FILL_ANGLES - 1};
  for (int i = 0; i < 12; i++) {
    if (lo >= hi) break;
    int mid = (lo + hi) / 2;
    if (texelFetch(uLensTable, ivec2(mid, row), 0).r <= tu.x) lo = mid + 1; else hi = mid;
  }
  float before = lo > 0 ? texelFetch(uLensTable, ivec2(lo - 1, row), 0).r : 0.0;
  float here = texelFetch(uLensTable, ivec2(lo, row), 0).r;
  float across = here > before ? (tu.x - before) / (here - before) : 0.5;
  float th = -PI + (float(lo) + across) * (2.0 * PI / ${FILL_ANGLES}.0);
  vec4 r01 = texelFetch(uLensTable, ivec2(lo, row + 1), 0);
  vec4 r23 = texelFetch(uLensTable, ivec2(lo, row + 2), 0);
  float total = max(0.0, r01.y * r01.y - r01.x * r01.x) + max(0.0, r01.w * r01.w - r01.z * r01.z) + max(0.0, r23.y * r23.y - r23.x * r23.x) + max(0.0, r23.w * r23.w - r23.z * r23.z);
  float left = clamp(tu.y, 0.0, 1.0) * total;
  float far = 0.0;
  bool found = false;
  ringStep(left, far, found, r01.xy);
  ringStep(left, far, found, r01.zw);
  ringStep(left, far, found, r23.xy);
  ringStep(left, far, found, r23.zw);
  return far * vec2(cos(th), sin(th));
}`;

/** Passes a point added to the running ones settles for, unseen, before it is drawn (flam3's fuse). */
const SETTLE = 30;

const ITERATE = `#version 300 es
precision highp float;
precision highp int;
layout(location = 0) in vec4 aState;
layout(location = 1) in uint aSeed;
out vec4 vState;
flat out uint vSeed;
out vec3 vColor;
out vec4 vBandA;
out vec2 vBandB;
uniform int uCount;
uniform vec3 uAbc[${MAX_XFORMS}];
uniform vec3 uDef[${MAX_XFORMS}];
uniform vec4 uMeta[${MAX_XFORMS}];
uniform ivec4 uVarId[${MAX_XFORMS * 2}];
uniform vec4 uVarW[${MAX_XFORMS * 2}];
// The parts' post maps (flam3's "post"), where uPostMask has their bit.
uniform vec3 uPostAbc[${PARTS}];
uniform vec3 uPostDef[${PARTS}];
uniform int uPostMask;
// The final transform, applied to where a point is drawn (not to where it goes on), where uHasFinal.
uniform int uHasFinal;
uniform vec3 uFinalAbc;
uniform vec3 uFinalDef;
uniform ivec4 uFinalVarId[2];
uniform vec4 uFinalVarW[2];
// The Lens's Polygon and Star (their sides and depth), and its symbols' fill tables.
uniform float uLensSides;
uniform float uLensDepth;
uniform highp sampler2D uLensTable;
uniform vec4 uCamera;
// The light's Transforms (transform.ts), in the camera's square space.
uniform mat2 uView;
uniform vec2 uViewAt;
uniform vec2 uTurn;
// The dive (zoom.ts): its part, how far into the loop (0..1: also the share of points drawn as that part's copy), and
// the camera's zoom and turn about the part's fixed point (a complex factor, then the point).
uniform int uDive;
uniform int uDiveI;
uniform float uDiveT;
uniform vec4 uDiveZ;
// The flame's colours: one chroma (OKLCH); each band's transform's colour as a point in the colour plane, and its
// lightness (a point's is its bands', by how near its colour is to each).
uniform float uChroma;
uniform vec2 uBandHue[${PARTS}];
uniform float uBandLight[${PARTS}];
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

// A variation of the transform's affine output p (abc, def: its coefficients, which some variations use).
vec2 variation(int v, vec2 p, float r2, float r, float th, vec3 abc, vec3 def, inout uint s) {
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
    case 15: return vec2(p.x + abc.y * sin(p.y / (abc.z * abc.z + 1e-6)), p.y + def.y * sin(p.x / (def.z * def.z + 1e-6)));
    case 17: return vec2(p.x + abc.z * sin(tan(3.0 * p.y)), p.y + def.z * sin(tan(3.0 * p.x)));
    case 21: {
      float k = abc.z * abc.z + 1e-6;
      return (mod(r + k, 2.0 * k) - k + r * (1.0 - k)) * vec2(cos(th), sin(th));
    }
    case 22: {
      float k = PI * (abc.z * abc.z + 1e-6);
      float a = mod(th + def.z, k) > k * 0.5 ? th - k * 0.5 : th + k * 0.5;
      return r * vec2(cos(a), sin(a));
    }
    case 31: {
      float p1 = rnd(s);
      float p2 = 2.0 * PI * rnd(s);
      return p1 * vec2(p.x * cos(p2), p.y * sin(p2));
    }
    case 34: {
      float p1 = rnd(s);
      float p2 = 2.0 * PI * rnd(s);
      return p1 * vec2(cos(p2), sin(p2));
    }
    case 41: {
      float a = rnd(s) * PI;
      return vec2(sin(a), sin(a) * sin(a) / cos(a));
    }
    case 43: return vec2(rnd(s) - 0.5, rnd(s) - 0.5);
    case 44: return tan(rnd(s) * PI) / (r2 + 1e-6) * vec2(cos(p.x), sin(p.y));
    case 45: {
      float a = rnd(s) * r;
      return p.x * vec2(cos(a) + sin(a), cos(a) - sin(a));
    }
    case 46: {
      float cr = cos(r);
      return vec2(p.x, 1.0 / cr + (cr < 0.0 ? 1.0 : -1.0));
    }
    case 47: {
      float a = rnd(s) * r;
      float sn = sin(a);
      float k = log(sn * sn + 1e-12) / log(10.0) + cos(a);
      if (bad(k)) k = -30.0;
      return p.x * vec2(k, k - sn * PI);
    }
    default: return p;
  }
}

// The final transform's warps only (flame.ts's FINALS): a second copy of the whole switch above for every point cost
// the Intel a third more of its time a step.
${MARK_GLSL}
${LENS_GLSL}
vec2 warp(int v, vec2 p, float r2, float r, float th) {
  switch (v) {
    case 1: return sin(p);
    case 2: return p / r2;
    case 3: return vec2(p.x * sin(r2) - p.y * cos(r2), p.x * cos(r2) + p.y * sin(r2));
    case 4: return vec2((p.x - p.y) * (p.x + p.y), 2.0 * p.x * p.y) / r;
    case 5: return vec2(th / PI, r - 1.0);
    case 6: return r * vec2(sin(th + r), cos(th - r));
    case 7: return r * vec2(sin(th * r), -cos(th * r));
    case 8: return (th / PI) * vec2(sin(PI * r), cos(PI * r));
    case 13: return sqrt(r) * vec2(cos(th * 0.5), sin(th * 0.5));
    case 16: return (2.0 / (r + 1.0)) * p.yx;
    case 27: return (2.0 / (r + 1.0)) * p;
    case 28: return (4.0 / (r2 + 4.0)) * p;
    case 99: return mark(p, r);
    case ${LENS_SHAPE_VARIATIONS.polygon}: return regularFill(lensTu(p, r), uLensSides, 0.0);
    case ${LENS_SHAPE_VARIATIONS.star}: return regularFill(lensTu(p, r), uLensSides, uLensDepth);
    default: return v >= ${TABLE_FIRST} ? tableFill(v - ${TABLE_FIRST}, lensTu(p, r)) : p;
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
    p += w * variation(uVarId[i * 2 + k / 4][k % 4], q, r2, r, th, uAbc[i], uDef[i], s);
  }
  if (i < ${PARTS} && ((uPostMask >> i) & 1) == 1) p = vec2(dot(uPostAbc[i], vec3(p, 1.0)), dot(uPostDef[i], vec3(p, 1.0)));
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
  // How much of each band's colour the point is: by how near its colour is to each band's, shares of one.
  float near[${PARTS}];
  float sum = 0.0;
  float light = 0.0;
  for (int k = 0; k < ${PARTS}; k++) {
    vec2 d = c - uBandHue[k];
    // A bell of nearness, as exp(-12 d²) but without its cost (six a point a step).
    float n = 1.0 / (1.0 + 12.0 * dot(d, d));
    near[k] = n * n * n;
    sum += near[k];
    light += near[k] * uBandLight[k];
  }
  sum = max(sum, 1e-6);
  vColor = srgb(vec3(light / sum, uChroma * c));
  vBandA = vec4(near[0], near[1], near[2], near[3]) / sum;
  vBandB = vec2(near[4], near[5]) / sum;
  // The dive: some points drawn as the dived-to part's copy of them (that copy is the whole, smaller: drawn larger as
  // the loop goes on, it becomes the whole again), all through the camera's zoom about the part's fixed point.
  vec2 drawn = p;
  if (uDive == 1 && !lost) {
    uint s2 = s ^ 0x9e3779b9u;
    if (rnd(s2) < uDiveT) {
      int j = uDiveI;
      vec3 hj = vec3(p, 1.0);
      vec2 qj = vec2(dot(uAbc[j], hj), dot(uDef[j], hj));
      float rj2 = dot(qj, qj) + 1e-12;
      float rj = sqrt(rj2);
      float thj = atan(qj.x, qj.y);
      vec2 pj = vec2(0.0);
      for (int k = 0; k < ${SLOTS}; k++) {
        float w = uVarW[j * 2 + k / 4][k % 4];
        if (w == 0.0) continue;
        pj += w * variation(uVarId[j * 2 + k / 4][k % 4], qj, rj2, rj, thj, uAbc[j], uDef[j], s2);
      }
      if (j < ${PARTS} && ((uPostMask >> j) & 1) == 1) pj = vec2(dot(uPostAbc[j], vec3(pj, 1.0)), dot(uPostDef[j], vec3(pj, 1.0)));
      if (!(bad(pj.x) || bad(pj.y))) drawn = pj;
    }
    vec2 d = drawn - uDiveZ.zw;
    drawn = uDiveZ.zw + vec2(uDiveZ.x * d.x - uDiveZ.y * d.y, uDiveZ.y * d.x + uDiveZ.x * d.y);
  }
  // Where it is drawn: through the final transform, where the flame has one.
  vec2 shown = drawn;
  if (uHasFinal == 1 && !lost) {
    vec3 fh = vec3(drawn, 1.0);
    vec2 fq = vec2(dot(uFinalAbc, fh), dot(uFinalDef, fh));
    float fr2 = dot(fq, fq) + 1e-12;
    float fr = sqrt(fr2);
    float fth = atan(fq.x, fq.y);
    // Its warp and the plain picture (flame.ts's randomFinal: two variations, the warp first).
    shown = uFinalVarW[0].x * warp(uFinalVarId[0].x, fq, fr2, fr, fth) + uFinalVarW[0].y * fq;
    if (bad(shown.x) || bad(shown.y)) lost = true;
  }
  // The camera: centred, the flame's size to the shorter side, turned slowly as a whole.
  vec2 v = (shown - uCamera.xy) * uCamera.z;
  v = vec2(uTurn.x * v.x - uTurn.y * v.y, uTurn.y * v.x + uTurn.x * v.y);
  v = uView * v + uViewAt;
  if (uCamera.w > 1.0) v.x /= uCamera.w; else v.y *= uCamera.w;
  gl_Position = lost ? vec4(2.0, 2.0, 0.0, 1.0) : vec4(v, 0.0, 1.0);
  gl_PointSize = 1.0;
}`;

const PLOT = `#version 300 es
precision mediump float;
in vec3 vColor;
in vec4 vBandA;
in vec2 vBandB;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
void main() { o = vec4(vColor, 1.0); bandA = vBandA; bandB = vec4(vBandB, 0.0, 0.0); }`;

export class FlamePoints {
  private gl: WebGL2RenderingContext;
  private iterate: WebGLProgram;
  private buffers: WebGLBuffer[] = [];
  private vaos: WebGLVertexArrayObject[] = [];
  private feedback: WebGLTransformFeedback;
  private flip = 0;
  /** The points: how many run the chaos game (the first `count` of buffers holding `capacity`), and how many of those
   *  are on the flame and drawn (`ready`): ones added later settle unseen for `settleLeft` more passes first. Fewer
   *  points only draw fewer; more only add new ones (2026-10-18: each change started every point again at random, the
   *  whole flame rebuilding, and on a tablet near its limit the step-down changed them every few seconds). */
  private count = 0;
  private capacity = 0;
  ready = 0;
  private settleLeft = 0;
  /** Steps every point takes unseen before it is drawn (flam3's fuse): a new point is not on the flame yet. */
  private fuse = 30;
  /** The flame drawn (null till the first). */
  flame: Flame | null = null;
  /** Each band's colour, as the flame has it (Tone map lights the band's points with it). */
  bandRgb = new Float32Array(PARTS * 3);
  /** The dive now (zoom.ts): its part and that part's fit, how far into the loop, when the fit was made and for which
   *  flame's parts. */
  private dive: { j: number; fit: DiveFit; t: number; at: number; parts: number } | null = null;
  /** How fast the dive turns the picture now (radians a second): the motion meter counts it with the parts' own. */
  diveTurnRate = 0;
  private random: () => number;
  private now: () => number;
  /** The Lens's symbols' fill tables (null till a lens needs them). */
  private table: WebGLTexture | null = null;
  private u: (p: WebGLProgram, name: string) => WebGLUniformLocation | null;

  /** `points` at first, placed and seeded by `random`; `now` the clock (ms). */
  constructor(gl: WebGL2RenderingContext, points: number, random: () => number, now: () => number) {
    this.gl = gl;
    this.random = random;
    this.now = now;
    this.u = uniforms(gl);
    this.iterate = program(gl, ITERATE, PLOT, ['vState', 'vSeed']);
    // Its tables' sampler on a unit of its own from the first (left on unit 0 it could name the light drawn into).
    gl.useProgram(this.iterate);
    gl.uniform1i(this.u(this.iterate, 'uLensTable'), TABLE_UNIT);
    this.feedback = gl.createTransformFeedback()!;
    this.seed(points);
  }

  /** How many points carry the chaos game: fewer where the device cannot keep up (they start again at random). */
  setPoints(n: number) {
    if (n === this.count) return;
    if (n > this.capacity) this.reserve(n);
    if (n < this.count) {
      this.count = n;
      this.ready = Math.min(this.ready, n);
      return;
    }
    // The new ones settle unseen over the next passes (a frame's own, not thirty at once), then are drawn.
    this.count = n;
    this.settleLeft = SETTLE;
  }

  /** Whether points added lately are still settling. */
  get settling(): boolean {
    return this.ready < this.count;
  }

  /** Buffers for `n` points: the ones running kept where they are, the rest new at random. */
  private reserve(n: number) {
    if (n <= this.capacity) return;
    const gl = this.gl;
    const old = this.buffers;
    const latest = old[this.flip];
    const data = new ArrayBuffer(n * 20);
    const f = new Float32Array(data);
    const u = new Uint32Array(data);
    for (let i = this.count; i < n; i++) {
      f[i * 5] = this.random() * 2 - 1;
      f[i * 5 + 1] = this.random() * 2 - 1;
      u[i * 5 + 4] = (this.random() * 0xfffffffe + 1) >>> 0;
    }
    for (const v of this.vaos) gl.deleteVertexArray(v);
    this.buffers = [];
    this.vaos = [];
    for (let k = 0; k < 2; k++) {
      const b = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_COPY);
      if (latest && this.count) {
        gl.bindBuffer(gl.COPY_READ_BUFFER, latest);
        gl.copyBufferSubData(gl.COPY_READ_BUFFER, gl.ARRAY_BUFFER, 0, 0, this.count * 20);
      }
      this.buffers.push(b);
      this.vaos.push(this.vaoOf(b));
    }
    gl.bindBuffer(gl.COPY_READ_BUFFER, null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    gl.bindVertexArray(null);
    for (const b of old) gl.deleteBuffer(b);
    this.capacity = n;
  }

  private vaoOf(b: WebGLBuffer): WebGLVertexArrayObject {
    const gl = this.gl;
    const vao = gl.createVertexArray()!;
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 20, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribIPointer(1, 1, gl.UNSIGNED_INT, 20, 16);
    return vao;
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
      f[i * 5] = this.random() * 2 - 1;
      f[i * 5 + 1] = this.random() * 2 - 1;
      u[i * 5 + 4] = (this.random() * 0xfffffffe + 1) >>> 0;
    }
    for (let k = 0; k < 2; k++) {
      const b = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_COPY);
      this.buffers.push(b);
      this.vaos.push(this.vaoOf(b));
    }
    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    this.count = n;
    this.capacity = n;
    this.ready = 0;
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
      const reach = x.colorReach ?? 1;
      meta.set([i === xs.length - 1 ? 1.0001 : run, x.colorSpeed, reach * Math.cos(2 * Math.PI * x.color), reach * Math.sin(2 * Math.PI * x.color)], i * 4);
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
    // The parts' post maps.
    const postAbc = new Float32Array(PARTS * 3);
    const postDef = new Float32Array(PARTS * 3);
    let mask = 0;
    xs.slice(0, PARTS).forEach((x, i) => {
      if (!x.post) return;
      mask |= 1 << i;
      postAbc.set([x.post[0], x.post[1], x.post[2]], i * 3);
      postDef.set([x.post[3], x.post[4], x.post[5]], i * 3);
    });
    gl.uniform1i(this.u(this.iterate, 'uPostMask'), mask);
    gl.uniform3fv(this.u(this.iterate, 'uPostAbc'), postAbc);
    gl.uniform3fv(this.u(this.iterate, 'uPostDef'), postDef);
    // The final transform.
    const final = flame.final;
    gl.uniform1i(this.u(this.iterate, 'uHasFinal'), final ? 1 : 0);
    if (final) {
      gl.uniform3f(this.u(this.iterate, 'uFinalAbc'), final.affine[0], final.affine[1], final.affine[2]);
      gl.uniform3f(this.u(this.iterate, 'uFinalDef'), final.affine[3], final.affine[4], final.affine[5]);
      // The shader takes one warp and the plain picture: the strongest warp (while two finals blend, the other one is
      // left out until it is the stronger), and the linear weight.
      const fid = new Int32Array(8);
      const fw = new Float32Array(8);
      let strongest: [number, number] = [0, 0];
      for (const [v, w] of final.variations) if (v !== 0 && w > strongest[1]) strongest = [v, w];
      fid[0] = strongest[0];
      fw[0] = strongest[1];
      fw[1] = final.variations.get(0) ?? 0;
      gl.uniform4iv(this.u(this.iterate, 'uFinalVarId'), fid);
      gl.uniform4fv(this.u(this.iterate, 'uFinalVarW'), fw);
      // The Lens's shapes: Polygon's and Star's sides and depth; the symbols' tables, made the first time one is drawn.
      gl.uniform1f(this.u(this.iterate, 'uLensSides'), final.shape?.sides ?? 5);
      gl.uniform1f(this.u(this.iterate, 'uLensDepth'), final.shape?.depth ?? 0.5);
      if (strongest[0] >= TABLE_FIRST && !this.table) this.table = this.makeTable();
    }
    // The bands' transforms are the first six (flame.ts): each one's colour, a point in the colour plane.
    const parts = Array.from({ length: PARTS }, (_, b) => xs[b] ?? { color: b / PARTS, colorReach: 1 });
    const lights = parts.map((_, b) => flame.colours.lights?.[b] ?? flame.colours.light);
    gl.uniform1f(this.u(this.iterate, 'uChroma'), flame.colours.chroma);
    gl.uniform2fv(
      this.u(this.iterate, 'uBandHue'),
      parts.flatMap((x) => [(x.colorReach ?? 1) * Math.cos(2 * Math.PI * x.color), (x.colorReach ?? 1) * Math.sin(2 * Math.PI * x.color)]),
    );
    gl.uniform1fv(this.u(this.iterate, 'uBandLight'), lights);
    parts.forEach((x, b) => this.bandRgb.set(colourOf(lights[b], flame.colours.chroma * (x.colorReach ?? 1), x.color), b * 3));
  }

  /** The Lens's symbols' fill tables (shape-set.ts `fillTexture`), on their own unit. */
  private makeTable(): WebGLTexture {
    const gl = this.gl;
    const t = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE0 + TABLE_UNIT);
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, FILL_ANGLES, TABLE_SHAPES.length * 3, 0, gl.RGBA, gl.FLOAT, fillTexture(TABLE_SHAPES));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.bindTexture(gl.TEXTURE_2D, null);
    gl.activeTexture(gl.TEXTURE0);
    return t;
  }

  /** The light's Transforms as one (transform.ts): where each point lands after the camera. */
  view: Affine = IDENTITY;

  /** This frame's steps into the light in use (bound, blending on): `passes` steps of every point, each drawn where
   *  it lands, after the unseen ones of the points still settling. */
  draw(light: Light, look: PointsParams, passes: number, since: number) {
    const gl = this.gl;
    const flame = this.flame!;
    // The points: each pass one step of the chaos game for every point, drawn where it lands.
    gl.useProgram(this.iterate);
    gl.uniform4f(this.u(this.iterate, 'uCamera'), flame.center[0], flame.center[1], look.scale / flame.size, light.width / light.height);
    gl.uniform2f(this.u(this.iterate, 'uTurn'), Math.cos(look.turn), Math.sin(look.turn));
    gl.uniformMatrix2fv(this.u(this.iterate, 'uView'), false, this.view.m);
    gl.uniform2f(this.u(this.iterate, 'uViewAt'), this.view.at[0], this.view.at[1]);
    this.diveStep(look.dive, since);
    // The table's unit is sampled whether or not a Lens needs it: a black pixel there without one.
    gl.activeTexture(gl.TEXTURE0 + TABLE_UNIT);
    gl.bindTexture(gl.TEXTURE_2D, this.table ?? none(gl));
    gl.activeTexture(gl.TEXTURE0);
    gl.blendFunc(gl.ONE, gl.ONE);
    gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, this.feedback);
    const unseen = this.fuse;
    this.fuse = 0;
    if (unseen) gl.enable(gl.RASTERIZER_DISCARD);
    for (let p = 0; p < unseen + passes; p++) {
      if (p === unseen && unseen) {
        gl.disable(gl.RASTERIZER_DISCARD);
        this.ready = this.count;
      }
      const from = this.flip;
      const to = 1 - from;
      gl.bindVertexArray(this.vaos[from]);
      // The points on the flame, drawn (all of them unseen while the whole set first settles), then any still settling,
      // unseen; each written to its own part of the other buffer.
      const drawn = p < unseen ? 0 : this.ready;
      if (drawn) {
        gl.bindBufferRange(gl.TRANSFORM_FEEDBACK_BUFFER, 0, this.buffers[to], 0, drawn * 20);
        gl.beginTransformFeedback(gl.POINTS);
        gl.drawArrays(gl.POINTS, 0, drawn);
        gl.endTransformFeedback();
      }
      const rest = this.count - drawn;
      if (rest) {
        if (drawn) gl.enable(gl.RASTERIZER_DISCARD);
        gl.bindBufferRange(gl.TRANSFORM_FEEDBACK_BUFFER, 0, this.buffers[to], drawn * 20, rest * 20);
        gl.beginTransformFeedback(gl.POINTS);
        gl.drawArrays(gl.POINTS, drawn, rest);
        gl.endTransformFeedback();
        if (drawn) gl.disable(gl.RASTERIZER_DISCARD);
      }
      gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER, 0, null);
      this.flip = to;
      if (p >= unseen && this.ready < this.count && --this.settleLeft <= 0) this.ready = this.count;
    }
    // Drawn again whatever the passes were (a first frame of none would leave every later draw discarded).
    gl.disable(gl.RASTERIZER_DISCARD);
    gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, null);
    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);
    if (this.table) {
      gl.activeTexture(gl.TEXTURE0 + TABLE_UNIT);
      gl.bindTexture(gl.TEXTURE_2D, null);
      gl.activeTexture(gl.TEXTURE0);
    }
  }

  /** The dive's camera for this frame at `rate` (the points' program in use). Its part is chosen as a loop starts (and
   *  for a flame of other parts), its fit made again four times a second as the flame moves. Let go of (`rate` 0)
   *  mid-loop, it finishes the loop gently and rests at the whole, so the picture never stops zoomed in. */
  private diveStep(rate: number, since: number) {
    const gl = this.gl;
    const flame = this.flame;
    const off = () => {
      gl.uniform1i(this.u(this.iterate, 'uDive'), 0);
      this.dive = null;
      this.diveTurnRate = 0;
    };
    if (!flame || (rate <= 0 && !this.dive)) return off();
    const now = this.now();
    let d = this.dive;
    if (!d || d.parts !== flame.xforms.length) {
      const target = diveTarget(flame);
      if (!target) return off();
      d = this.dive = { j: target.j, fit: target, t: 0, at: now, parts: flame.xforms.length };
    } else if (now - d.at > 250) {
      const fit = diveFit(flame, d.j);
      if (fit) d.fit = fit;
      d.at = now;
    }
    const speed = rate > 0 ? rate : 0.15;
    const camera = diveCamera(d.fit, d.t, speed);
    d.t += since / Math.max(0.5, Math.log(1 / Math.hypot(d.fit.a[0], d.fit.a[1])) / speed);
    if (d.t >= 1) {
      // A loop done: the part become the whole. Let go of, the dive rests here; else the next loop's part is chosen.
      if (rate <= 0) return off();
      const target = diveTarget(flame);
      if (!target) return off();
      this.dive = { j: target.j, fit: target, t: d.t % 1, at: now, parts: flame.xforms.length };
    }
    this.diveTurnRate = camera.turnRate;
    gl.uniform1i(this.u(this.iterate, 'uDive'), 1);
    gl.uniform1i(this.u(this.iterate, 'uDiveI'), d.j);
    gl.uniform1f(this.u(this.iterate, 'uDiveT'), d.t);
    gl.uniform4f(this.u(this.iterate, 'uDiveZ'), camera.zoom[0], camera.zoom[1], camera.at[0], camera.at[1]);
  }

  dispose() {
    const gl = this.gl;
    for (const b of this.buffers) gl.deleteBuffer(b);
    for (const v of this.vaos) gl.deleteVertexArray(v);
    gl.deleteTransformFeedback(this.feedback);
    gl.deleteProgram(this.iterate);
    if (this.table) gl.deleteTexture(this.table);
  }
}
