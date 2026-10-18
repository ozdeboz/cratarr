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
import { MARK, colourOf, PARTS, type Flame } from './flame';
import { Fluid, gridFor, type Flow, type Force } from './fluid';
import { FlashMeter } from './flashmeter';
import type { Tiles } from './meters';

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
uniform vec4 uCamera;
uniform vec2 uTurn;
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
  // Where it is drawn: through the final transform, where the flame has one.
  vec2 shown = p;
  if (uHasFinal == 1 && !lost) {
    vec3 fh = vec3(p, 1.0);
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

const FULL = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

const FADE = `#version 300 es
precision mediump float;
uniform float uKeep;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
void main() { o = vec4(0.0, 0.0, 0.0, uKeep); bandA = o; bandB = o; }`;

// Not a number, or infinite, from the bits (see ITERATE's).
// A value not a number is nothing; an infinite one, the most a 16-bit float holds short of infinity (by its sign: a
// comparison is false for one not a number). The Mali-G615's compiler gets a fragment shader wrong that tests the bits
// and picks its answer from them too (0 came out as 60000, 0.12 as 0: a phone's picture white, with coloured noise;
// measured 2026-10-18), so the bits only answer whether.
const BAD = `bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }
float clean1(float x) { return bad(x) ? (x > 0.0 ? 60000.0 : 0.0) : clamp(x, 0.0, 60000.0); }
vec4 clean(vec4 v) { return vec4(clean1(v.x), clean1(v.y), clean1(v.z), clean1(v.w)); }`;

// The scrub: both buffers copied, each value not a number or infinite made nothing (and the largest kept under what a
// 16-bit float holds).
const SCRUB = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
${BAD}
void main() {
  ivec2 at = ivec2(gl_FragCoord.xy);
  o = clean(texelFetch(uAcc, at, 0));
  bandA = clean(texelFetch(uBandsA, at, 0));
  bandB = clean(texelFetch(uBandsB, at, 0));
}`;

// The carry: the picture so far drawn into buffers of another size (the step-down's changes of resolution, a window
// resized), each value as many old pixels' worth as a new one covers, so it shows as it did instead of starting over.
const CARRY = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
uniform vec2 uSize;
uniform float uArea;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
${BAD}
void main() {
  vec2 uv = gl_FragCoord.xy / uSize;
  o = clean(textureLod(uAcc, uv, 0.0)) * uArea;
  bandA = clean(textureLod(uBandsA, uv, 0.0)) * uArea;
  bandB = clean(textureLod(uBandsB, uv, 0.0)) * uArea;
}`;

// Finds whether a buffer holds any such value: only those pixels pass (counted by an occlusion query).
const FIND = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
out vec4 o;
${BAD}
bool any4(vec4 v) { return bad(v.x) || bad(v.y) || bad(v.z) || bad(v.w); }
void main() {
  ivec2 at = ivec2(gl_FragCoord.xy);
  if (!(any4(texelFetch(uAcc, at, 0)) || any4(texelFetch(uBandsA, at, 0)) || any4(texelFetch(uBandsB, at, 0)))) discard;
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
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
// Each band's light now (-1..3: above 0 its transients', below it dimmed), and its colour.
uniform float uHit[${PARTS}];
uniform vec3 uBandRgb[${PARTS}];
uniform float uCeiling;
uniform float uSaturation;
// Smoothing: the most a pixel where few points fell is spread over (pixels of the buffer), and the buffer's levels
// above its first that it may take that from (none where it has none). Floor: the faintest density shown (on the
// display's scale, a.a * uK2; 0 for all). The fire's share: a blackbody's heat where the points are dense.
uniform float uSmooth;
uniform float uLevels;
uniform float uFloor;
uniform float uFire;
// The fluid's flow (cells a second, its grid's), the seconds of it the picture is read back along (0: none), and a
// cell's size in texture coordinates.
uniform sampler2D uVelocity;
uniform float uWarp;
uniform vec2 uUvPerCell;
out vec4 o;
${BAD}
// A sample of the buffer, none of it anything but a number (whatever a driver did to it before the scrub came by).
vec4 at(vec2 uv, float lod) { return clean(textureLod(uAcc, uv, lod)); }
vec4 raw(vec2 uv, float lod) { return textureLod(uAcc, uv, lod); }
// A blackbody's colours by its heat (0..1): black, a deep red, orange, yellow, white only at the very hottest.
vec3 heat(float t) { return clamp(vec3(2.2 * t, 2.2 * t - 0.8, 4.0 * t - 3.3), 0.0, 1.0); }
void main() {
  // A beat's swell: the picture drawn a little larger about its middle.
  vec2 uv = 0.5 + (gl_FragCoord.xy / uView - 0.5) / uZoom;
  if (uWarp > 0.0) uv -= uWarp * textureLod(uVelocity, uv, 0.0).xy * uUvPerCell;
  vec2 px = 1.0 / vec2(textureSize(uAcc, 0));
  vec4 a = at(uv, 0.0);
  // The pixel's points by band: shares of one.
  vec4 shareA = clamp(clean(texture(uBandsA, uv)) / max(a.a, 1e-6), 0.0, 1.0);
  vec2 shareB = clamp(clean(texture(uBandsB, uv)).xy / max(a.a, 1e-6), 0.0, 1.0);
  // Element by element: the Mali-G615's compiler refuses an array constructor here ("no default precision defined for
  // variable 'float[6]'", 2026-10-18), which left that phone without visuals.
  float share[${PARTS}];
  share[0] = shareA.x;
  share[1] = shareA.y;
  share[2] = shareA.z;
  share[3] = shareA.w;
  share[4] = shareB.x;
  share[5] = shareB.y;
  // The paper's density estimation, cheaply: where few points fell, the pixel is the mean of a wider ring around it
  // (up to uSmooth pixels out; past three, from a coarser level of the buffer, so the ring is a haze rather than
  // eight copies), where many fell it stays sharp. The ring is cleaned once, as a whole: a value not a number in it
  // costs that pixel its ring for a frame (the scrub clears it from the buffer), and cleaning each of its eight was
  // a third of a tablet's frame (Mali-G52: 13.6 ms to 9.3 ms, 2026-10-18).
  float radius = uSmooth / (1.0 + sqrt(a.a * uK2) * 2.0);
  if (radius > 0.4) {
    float lod = min(uLevels, max(0.0, log2(radius / 3.0)));
    vec2 d = px * radius;
    vec4 ring = raw(uv + vec2(d.x, 0.0), lod) + raw(uv - vec2(d.x, 0.0), lod)
              + raw(uv + vec2(0.0, d.y), lod) + raw(uv - vec2(0.0, d.y), lod)
              + raw(uv + d * 0.7071, lod) + raw(uv - d * 0.7071, lod)
              + raw(uv + vec2(d.x, -d.y) * 0.7071, lod) + raw(uv + vec2(-d.x, d.y) * 0.7071, lod);
    a = (a + clean(ring)) / 9.0;
  }
  if (a.a <= 0.0) { o = vec4(0.0, 0.0, 0.0, 1.0); return; }
  float x = a.a * uK2;
  float ls = uK1 * log(1.0 + x) / a.a;
  vec3 c = a.rgb * ls;
  float alpha = a.a * ls;
  // flam3's gamma threshold: below it the curve is a straight line, so the faintest pixels are not lifted into noise.
  const float T = 0.02;
  float g = alpha < T ? alpha / T * pow(T, 1.0 / uGamma) : pow(min(alpha, 1.0), 1.0 / uGamma);
  vec3 vib = c * (g / max(alpha, 1e-6));
  vec3 flat_ = pow(max(c, 0.0), vec3(1.0 / uGamma));
  // A beat's flash, after the gamma, so it shows as much in the faint parts as in the bright ones; a band's light
  // brightens the points of its colour and adds a glow of that colour, or (below 0) dims them, to 0.4 at -1. The
  // shares are of one, so the bands' lights are a mean here, never a sum.
  float lit = 0.0;
  vec3 glow = vec3(0.0);
  for (int k = 0; k < ${PARTS}; k++) {
    float h = uHit[k] * share[k];
    lit += h;
    glow += max(h, 0.0) * uBandRgb[k];
  }
  vec3 shown = mix(flat_, vib, uVibrancy);
  // Fire: the denser, the hotter, over the bands' own colours (which still tell them apart).
  shown = mix(shown, heat(g), 0.65 * uFire);
  shown = shown * uFlash * (1.0 + 0.6 * lit) + 0.2 * g * glow;
  // Floor: fainter than it, black (from half of it to twice it, so its edge does not crawl with the grain).
  if (uFloor > 0.0) shown *= smoothstep(-1.0, 1.0, log2(x / uFloor));
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
  /** Each part's light now, low to high (-1..3): its colour's points light up, or below 0 dim. */
  hits: number[];
  /** The most brightness shown (0..1), applied after everything else. */
  ceiling: number;
  /** The display's exposure against the points' mean density (1: as the paper has it): the visuals' auto-exposure. */
  exposure: number;
  /** How much of its colour it shows (0: grey, 1: all). */
  saturation: number;
  /** The most a pixel where few points fell is spread over, in pixels of the buffer (0: none, the raw grain). */
  smoothing: number;
  /** The faintest density shown, on the display's scale (0: all of it; 1: about the flame's mean). */
  floor: number;
  /** How much of a fire's heat shows (0..1): the densest points yellow-white, the faintest a deep red. */
  fire: number;
  /** How much the light flows (0..1, decision 0047): 0 fades it where it was drawn, as before; above, it is carried
   *  along by the fluid (fluid.ts), pushed by `forces` and moving as `flow` says, `dt` seconds a frame, and the picture
   *  is read back along the flow by `warp` seconds of it. */
  fluid: number;
  forces: Force[];
  flow: Flow;
  dt: number;
  warp: number;
};

export class FlameRenderer {
  private gl: WebGL2RenderingContext;
  // The flash meter (flashmeter.ts), made the first time the screen is measured; out where it cannot be made.
  private meter: FlashMeter | null = null;
  private meterOut = false;
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
  private bandSetsB: WebGLTexture[] = [];
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
  private bandsB: WebGLTexture | null = null;
  private bandRgb = new Float32Array(PARTS * 3);
  /** The colours buffer's levels above its first (0 where they cannot be filtered). */
  private levels = 0;
  /** The fluid the light flows in (decision 0047): made the first time the Fluid setting asks for it (nothing of it
   *  before), let go after ten seconds unasked, and not made again where this device cannot (`fluidOut`). */
  private fluid: Fluid | null = null;
  private fluidOut = false;
  private fluidUnwanted = 0;
  private accFbo: WebGLFramebuffer | null = null;
  private empty: WebGLVertexArrayObject;
  private flip = 0;
  /** The points: how many run the chaos game (the first `count` of buffers holding `capacity`), and how many of those
   *  are on the flame and drawn (`ready`): ones added later settle unseen for `settleLeft` more passes first. Fewer
   *  points only draw fewer; more only add new ones (2026-10-18: each change started every point again at random, the
   *  whole flame rebuilding, and on a tablet near its limit the step-down changed them every few seconds). */
  private count = 0;
  private capacity = 0;
  private ready = 0;
  private settleLeft = 0;
  private carrying: WebGLProgram;
  /** The buffers being replaced at a resize, their picture carried into the new ones. */
  private carryFrom: { accs: WebGLTexture[]; bands: WebGLTexture[]; bandsB: WebGLTexture[]; fbos: WebGLFramebuffer[]; cur: number; width: number; height: number } | null = null;
  private flame: Flame | null = null;
  private loc = new Map<string, WebGLUniformLocation | null>();
  /** The buffer's kind: 32-bit floats where they can be blended into and filtered, else 16-bit (a pixel stops counting
   *  near 2048 points, only in the densest cores, where the log display hardly shows it), else 8 bits (saturating
   *  soon). 16-bit ones were the default in preview 56 (4 to 20 % quicker): the densest pixels reach some 60,000 points
   *  (measured in 32-bit ones, 2026-10-18), against a 16-bit float's 65,504, and a GPU whose blending rounds up
   *  (Direct3D allows it) carries them over into infinity, black for good: a GTX 1650 showed such specks from 56 on, a
   *  tablet's Mali-G52 (16-bit then, a GPU that rounds to nearest, stuck at 2048) never did. 32-bit ones that cannot be
   *  filtered (that tablet's Mali-G52 blends them but has no OES_texture_float_linear, 2026-10-18; a phone's Mali-G615
   *  has it) were read pixel by pixel and scaled up to the screen blocky: the tablet's picture grainier in 59, and no
   *  coarser levels for the smoothing. */
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
    this.precision =
      floats && gl.getExtension('EXT_float_blend') && gl.getExtension('OES_texture_float_linear') ? 32 : floats || gl.getExtension('EXT_color_buffer_half_float') ? 16 : 8;
    this.iterate = program(gl, ITERATE, PLOT, ['vState', 'vSeed']);
    this.plot = this.iterate;
    this.fade = program(gl, FULL, FADE);
    this.show = program(gl, FULL, SHOW);
    this.scrubbing = program(gl, FULL, SCRUB);
    this.finding = program(gl, FULL, FIND);
    this.carrying = program(gl, FULL, CARRY);
    this.empty = gl.createVertexArray()!;
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

  /** Whether points added lately are still settling (the frames meanwhile are not the device's usual pace). */
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
      f[i * 5] = Math.random() * 2 - 1;
      f[i * 5 + 1] = Math.random() * 2 - 1;
      u[i * 5 + 4] = (Math.random() * 0xfffffffe + 1) >>> 0;
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
      f[i * 5] = Math.random() * 2 - 1;
      f[i * 5 + 1] = Math.random() * 2 - 1;
      u[i * 5 + 4] = (Math.random() * 0xfffffffe + 1) >>> 0;
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

  /** The drawing size in pixels (the buffer the points fall into; the screen scales it). */
  resize(width: number, height: number) {
    if (width === this.width && height === this.height) return;
    const gl = this.gl;
    this.fluid?.resize(...gridFor(width, height));
    // The old buffers kept until the new ones hold their picture (a failed try at a kind a driver refused: dropped).
    if (!this.carryFrom && this.accs.length && this.width && this.height) {
      this.carryFrom = { accs: this.accs, bands: this.bandSets, bandsB: this.bandSetsB, fbos: this.fbos, cur: this.cur, width: this.width, height: this.height };
    } else {
      for (const x of [...this.accs, ...this.bandSets, ...this.bandSetsB]) gl.deleteTexture(x);
      for (const f of this.fbos) gl.deleteFramebuffer(f);
    }
    this.width = width;
    this.height = height;
    this.accs = [];
    this.bandSets = [];
    this.bandSetsB = [];
    this.fbos = [];
    // 32-bit float textures filter only with an extension; without it the screen reads them nearest.
    const smooth = this.precision !== 32 || !!gl.getExtension('OES_texture_float_linear');
    // The colours in the buffer's kind; the bands' shares (only ever weighed against the count beside them) in 16-bit
    // floats at most: each point writes all three, and 32-bit ones for all cost the Intel two of its three steps.
    const texture = (bits: 32 | 16 | 8 = this.precision) => {
      const tex = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, tex);
      if (bits === 32) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, width, height, 0, gl.RGBA, gl.FLOAT, null);
      else if (bits === 16) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, width, height, 0, gl.RGBA, gl.HALF_FLOAT, null);
      else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, smooth ? gl.LINEAR : gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, smooth ? gl.LINEAR : gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return tex;
    };
    for (let k = 0; k < 2; k++) {
      const shares = this.precision === 32 ? 16 : this.precision;
      this.accs.push(texture());
      this.bandSets.push(texture(shares));
      this.bandSetsB.push(texture(shares));
      const fbo = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.accs[k], 0);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT1, gl.TEXTURE_2D, this.bandSets[k], 0);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT2, gl.TEXTURE_2D, this.bandSetsB[k], 0);
      gl.drawBuffers([gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1, gl.COLOR_ATTACHMENT2]);
      // A driver may still refuse the kind it offered: one step down, and again.
      if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE && this.precision !== 8) {
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.deleteFramebuffer(fbo);
        this.precision = this.precision === 32 ? 16 : 8;
        const kept = this.carryFrom;
        this.width = 0;
        this.carryFrom = kept;
        this.resize(width, height);
        return;
      }
      this.fbos.push(fbo);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
    this.cur = 0;
    this.use();
    this.carry();
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    // The colours' coarser levels, for a wide smoothing (made each frame it is asked for; here only so the texture is
    // complete with a filter that reads them), where they can be filtered.
    this.levels = smooth ? Math.floor(Math.log2(Math.max(width, height))) : 0;
    if (this.levels)
      for (const acc of this.accs) {
        gl.bindTexture(gl.TEXTURE_2D, acc);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
        gl.generateMipmap(gl.TEXTURE_2D);
      }
  }

  /** The old buffers' picture into the new set in use, then the old ones gone. */
  private carry() {
    const old = this.carryFrom;
    this.carryFrom = null;
    if (!old) return;
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.accFbo);
    gl.viewport(0, 0, this.width, this.height);
    gl.disable(gl.BLEND);
    gl.useProgram(this.carrying);
    const at = (n: string) => gl.getUniformLocation(this.carrying, n);
    [old.accs[old.cur], old.bands[old.cur], old.bandsB[old.cur]].forEach((t, i) => {
      gl.activeTexture(gl.TEXTURE0 + i);
      gl.bindTexture(gl.TEXTURE_2D, t);
    });
    gl.uniform1i(at('uAcc'), 0);
    gl.uniform1i(at('uBandsA'), 1);
    gl.uniform1i(at('uBandsB'), 2);
    gl.uniform2f(at('uSize'), this.width, this.height);
    gl.uniform1f(at('uArea'), (old.width * old.height) / (this.width * this.height));
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
    gl.activeTexture(gl.TEXTURE0);
    for (const x of [...old.accs, ...old.bands, ...old.bandsB]) gl.deleteTexture(x);
    for (const f of old.fbos) gl.deleteFramebuffer(f);
  }

  /** The set in use as `acc`, `bands` and `accFbo`. */
  private use() {
    this.acc = this.accs[this.cur];
    this.bands = this.bandSets[this.cur];
    this.bandsB = this.bandSetsB[this.cur];
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
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, this.bandSetsB[this.cur]);
    if (!this.query) {
      this.query = gl.createQuery();
      gl.useProgram(this.finding);
      gl.uniform1i(gl.getUniformLocation(this.finding, 'uAcc'), 0);
      gl.uniform1i(gl.getUniformLocation(this.finding, 'uBandsA'), 1);
      gl.uniform1i(gl.getUniformLocation(this.finding, 'uBandsB'), 2);
      gl.colorMask(false, false, false, false);
      gl.beginQuery(gl.ANY_SAMPLES_PASSED_CONSERVATIVE, this.query!);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.endQuery(gl.ANY_SAMPLES_PASSED_CONSERVATIVE);
      gl.colorMask(true, true, true, true);
    }
    gl.useProgram(this.scrubbing);
    gl.uniform1i(gl.getUniformLocation(this.scrubbing, 'uAcc'), 0);
    gl.uniform1i(gl.getUniformLocation(this.scrubbing, 'uBandsA'), 1);
    gl.uniform1i(gl.getUniformLocation(this.scrubbing, 'uBandsB'), 2);
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

    // Fade what is there; with the Fluid setting, carry it along the flow as it fades, into the other set of buffers
    // (as the scrub copies), and draw on there.
    const fluid = this.fluidFor(look.fluid);
    if (fluid) {
      fluid.push(look.forces);
      fluid.step(look.dt, look.flow);
      const other = 1 - this.cur;
      fluid.advect({ acc: this.acc!, bandsA: this.bands!, bandsB: this.bandsB! }, this.fbos[other], look.keep, this.width, this.height);
      this.cur = other;
      this.use();
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.accFbo);
      gl.viewport(0, 0, this.width, this.height);
      gl.enable(gl.BLEND);
    } else {
      gl.useProgram(this.fade);
      gl.uniform1f(this.u(this.fade, 'uKeep'), look.keep);
      gl.blendFunc(gl.ZERO, gl.SRC_ALPHA);
      gl.bindVertexArray(this.empty);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

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

    // Show it. The points held at a time: each frame's, kept 1/(1 − keep) frames; their mean per pixel sets k2.
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, viewWidth, viewHeight);
    gl.useProgram(this.show);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.acc);
    gl.uniform1i(this.u(this.show, 'uAcc'), 0);
    gl.uniform2f(this.u(this.show, 'uView'), viewWidth, viewHeight);
    const held = (Math.max(1, this.ready) * passes) / Math.max(0.02, 1 - look.keep);
    const perPixel = held / (this.width * this.height);
    gl.uniform1f(this.u(this.show, 'uK1'), look.brightness);
    gl.uniform1f(this.u(this.show, 'uK2'), ((this.precision === 8 ? 255 : 4) * look.exposure) / perPixel);
    gl.uniform1f(this.u(this.show, 'uGamma'), look.gamma);
    gl.uniform1f(this.u(this.show, 'uVibrancy'), look.vibrancy);
    gl.uniform1f(this.u(this.show, 'uZoom'), look.zoom);
    gl.uniform1f(this.u(this.show, 'uFlash'), look.flash);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.bands);
    gl.uniform1i(this.u(this.show, 'uBandsA'), 1);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, this.bandsB);
    gl.uniform1i(this.u(this.show, 'uBandsB'), 2);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1fv(this.u(this.show, 'uHit'), Float32Array.from({ length: PARTS }, (_, k) => look.hits[k] ?? 0));
    gl.uniform3fv(this.u(this.show, 'uBandRgb'), this.bandRgb);
    gl.uniform1f(this.u(this.show, 'uCeiling'), look.ceiling);
    gl.uniform1f(this.u(this.show, 'uSaturation'), look.saturation);
    // A smoothing wider than three pixels takes its ring from the buffer's coarser levels, made now (only then: the
    // levels cost a pass over the buffer).
    const levels = look.smoothing > 3 ? this.levels : 0;
    if (levels) {
      gl.bindTexture(gl.TEXTURE_2D, this.acc);
      gl.generateMipmap(gl.TEXTURE_2D);
    }
    gl.uniform1f(this.u(this.show, 'uSmooth'), look.smoothing);
    gl.uniform1f(this.u(this.show, 'uLevels'), levels);
    gl.uniform1f(this.u(this.show, 'uFloor'), look.floor);
    gl.uniform1f(this.u(this.show, 'uFire'), look.fire);
    const warp = fluid && look.warp > 0 ? look.warp : 0;
    gl.uniform1f(this.u(this.show, 'uWarp'), warp);
    if (fluid && warp) {
      gl.activeTexture(gl.TEXTURE3);
      gl.bindTexture(gl.TEXTURE_2D, fluid.velocity);
      gl.activeTexture(gl.TEXTURE0);
      gl.uniform1i(this.u(this.show, 'uVelocity'), 3);
      gl.uniform2fv(this.u(this.show, 'uUvPerCell'), fluid.uvPerCell);
    }
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** The fluid, where the Fluid setting asks for one (`amount` over 0) and this device can make it; let go after ten
   *  seconds unasked, so a passing 0 does not make it again and again. */
  /** The screen just shown (`viewWidth` x `viewHeight`) read into tiles for the flash meter (meters.ts), as of `t`
   *  seconds; read back a frame or two later with `tiles`. None where this device cannot (it is then never tried again). */
  measure(viewWidth: number, viewHeight: number, t: number) {
    if (this.meterOut) return;
    try {
      this.meter ??= new FlashMeter(this.gl);
      this.meter.capture(viewWidth, viewHeight, t);
    } catch (e) {
      console.warn('Visuals: no flash meter on this device', e);
      this.meter?.dispose();
      this.meter = null;
      this.meterOut = true;
    }
  }

  /** The screen's readings the GPU has finished, the oldest first. */
  tiles(): Tiles[] {
    return this.meter?.poll() ?? [];
  }

  private fluidFor(amount: number): Fluid | null {
    const now = performance.now();
    if (!(amount > 0)) {
      if (this.fluid && now - this.fluidUnwanted > 10_000) {
        this.fluid.dispose();
        this.fluid = null;
      }
      return null;
    }
    this.fluidUnwanted = now;
    if (this.fluid || this.fluidOut) return this.fluid;
    try {
      if (!Fluid.supported(this.gl)) throw new Error('not supported');
      this.fluid = new Fluid(this.gl, ...gridFor(this.width, this.height));
    } catch (e) {
      console.warn('Visuals: no fluid on this device', e);
      this.fluidOut = true;
    }
    return this.fluid;
  }

  /** Whether the GPU took the context away (the page should stop and drop the renderer). */
  get lost(): boolean {
    return this.gl.isContextLost();
  }

  dispose() {
    const gl = this.gl;
    this.fluid?.dispose();
    this.fluid = null;
    this.meter?.dispose();
    this.meter = null;
    for (const b of this.buffers) gl.deleteBuffer(b);
    for (const v of this.vaos) gl.deleteVertexArray(v);
    gl.deleteVertexArray(this.empty);
    gl.deleteTransformFeedback(this.feedback);
    for (const x of [...this.accs, ...this.bandSets, ...this.bandSetsB]) gl.deleteTexture(x);
    for (const f of this.fbos) gl.deleteFramebuffer(f);
    if (this.query) gl.deleteQuery(this.query);
    for (const p of new Set([this.iterate, this.plot, this.fade, this.show, this.scrubbing, this.finding, this.carrying])) gl.deleteProgram(p);
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
