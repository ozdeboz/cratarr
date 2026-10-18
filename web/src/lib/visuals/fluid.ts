// A fluid the visuals' light can flow in (decision 0047): the flame's trails, instead of only fading where they were
// drawn, are carried along by a flow the music stirs, so they drift, curl and fold like smoke. The owner: "another way
// of having such dancing movement is through fluid dynamics", and in the next version "only if we can keep it
// development properly separate": this file is the fluid alone; the renderer calls it through the few methods below
// (decision 0047's seam) and the visuals turn the music into its forces.
//
// Stam's stable fluids on the GPU ("Real-Time Fluid Dynamics for Games", 2003; as in GPU Gems 1, ch. 38): a velocity
// field on a coarse grid of its own (a hundred cells or so across, whatever the screen), each step pushed by the
// forces, given back its small whirls (vorticity confinement, Fedkiw et al. 2001), made free of divergence (pressure by
// Jacobi iteration, then its gradient taken away) and carried along by itself; the light then carried along by it at
// the buffer's own size. In as few passes as that takes (eight a step): on a Mali tablet a pass cost more than its sums
// (seventeen were 5 ms a frame on a G52, the grid's size hardly counting; 2026-10-18). Velocities are in grid cells per second (`uvPerCell` turns them into texture coordinates).
//
// Written for the Mali GPUs Cratarr's devices have (2026-10-18): no array constructors in a shader; a value's bits only
// say whether it is a number, never what to answer with; no `isnan`; 16-bit float textures throughout (a G52 cannot
// filter 32-bit ones), drawn into with EXT_color_buffer_float.
//
// Each call changes the framebuffer bound, the program, the vertex array, the viewport, blending (off) and the
// textures bound to the first units: the renderer sets its own again after. `advect` uses the last `step`'s time, so
// a frame is `push`, `step`, then `advect`.

/** A push on the fluid: where (texture coordinates of the buffer, 0..1 from its bottom left), which way and how fast
 *  (texture coordinates per second, at strength 1), how far it reaches (in heights of the buffer) and how hard. */
export type Force = { x: number; y: number; dx: number; dy: number; radius: number; strength: number };

/** How the fluid moves now: `energy` 0..1 (how long its motion lasts: a calm passage settles within a second, a loud
 *  one flows on), `swirl` 0.. (how strongly its small whirls are kept). */
export type Flow = { energy: number; swirl: number };

/** The light the renderer keeps: its colour and density, and its two band-share buffers. */
export type Light = { acc: WebGLTexture; bandsA: WebGLTexture; bandsB: WebGLTexture };

/** The most forces taken a step; more are dropped (the strongest kept). */
export const MOST_FORCES = 16;
/** Jacobi iterations a step: enough for smoke that looks right, few enough for a tablet (each is a pass). */
const ITERATIONS = 6;
/** The fastest the fluid moves (cells a second): beyond it, a bad step could blow up. */
const FASTEST = 400;
/** What is left of the last step's pressure (a warm start for the iteration). */
const PRESSURE_KEPT = 0.8;
/** Vorticity confinement at swirl 1 (Dobryakov's WebGL fluid uses 30 at this scale). */
const SWIRL = 30;

const VERTEX = `#version 300 es
precision highp float;
out vec2 vUv;
// One triangle over the whole target: no buffers, only the vertex's number.
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

// Whether a value is not a number or infinite, from its bits (which no compiler's fast arithmetic assumes away).
const BAD = `bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }`;

const HEAD = `#version 300 es
precision highp float;
precision highp sampler2D;
in vec2 vUv;
${BAD}
`;

// The forces, and the small whirls given back (vorticity confinement): one pass, the curl worked out where it is
// needed (a pass of its own costs a Mali GPU more than the sums: each one is a flush of its tiles).
const FORCE = `${HEAD}
uniform sampler2D uVelocity;
uniform vec2 uTexel;
uniform float uAspect;
uniform vec2 uGrid;
uniform int uCount;
uniform vec4 uAt[${MOST_FORCES}];
uniform vec2 uWay[${MOST_FORCES}];
uniform float uSwirl;
uniform float uDt;
out vec4 o;
float curlAt(vec2 uv) {
  float l = texture(uVelocity, uv - vec2(uTexel.x, 0.0)).y;
  float r = texture(uVelocity, uv + vec2(uTexel.x, 0.0)).y;
  float b = texture(uVelocity, uv - vec2(0.0, uTexel.y)).x;
  float t = texture(uVelocity, uv + vec2(0.0, uTexel.y)).x;
  return 0.5 * (r - l - t + b);
}
void main() {
  vec2 v = texture(uVelocity, vUv).xy;
  float l = curlAt(vUv - vec2(uTexel.x, 0.0));
  float r = curlAt(vUv + vec2(uTexel.x, 0.0));
  float b = curlAt(vUv - vec2(0.0, uTexel.y));
  float t = curlAt(vUv + vec2(0.0, uTexel.y));
  float c = curlAt(vUv);
  vec2 f = 0.5 * vec2(abs(t) - abs(b), abs(r) - abs(l));
  f /= length(f) + 0.0001;
  v += f * uSwirl * c * vec2(1.0, -1.0) * uDt;
  for (int i = 0; i < ${MOST_FORCES}; i++) {
    if (i >= uCount) break;
    vec2 d = vUv - uAt[i].xy;
    d.x *= uAspect;
    float radius = max(uAt[i].z, 0.001);
    v += uWay[i] * uGrid * uAt[i].w * exp(-dot(d, d) / (radius * radius));
  }
  o = bad(v.x + v.y) ? vec4(0.0) : vec4(clamp(v, -${FASTEST}.0, ${FASTEST}.0), 0.0, 1.0);
}`;

// One Jacobi iteration of the pressure, the velocity's divergence worked out in it (the edges are walls: what flows
// into one comes back); `uWarm` scales the pressure it starts from (the last step's, a little of it kept).
const JACOBI = `${HEAD}
uniform sampler2D uPressure;
uniform sampler2D uVelocity;
uniform vec2 uTexel;
uniform float uWarm;
out vec4 o;
void main() {
  vec2 c = texture(uVelocity, vUv).xy;
  float vl = texture(uVelocity, vUv - vec2(uTexel.x, 0.0)).x;
  float vr = texture(uVelocity, vUv + vec2(uTexel.x, 0.0)).x;
  float vb = texture(uVelocity, vUv - vec2(0.0, uTexel.y)).y;
  float vt = texture(uVelocity, vUv + vec2(0.0, uTexel.y)).y;
  if (vUv.x - uTexel.x < 0.0) vl = -c.x;
  if (vUv.x + uTexel.x > 1.0) vr = -c.x;
  if (vUv.y - uTexel.y < 0.0) vb = -c.y;
  if (vUv.y + uTexel.y > 1.0) vt = -c.y;
  float d = 0.5 * (vr - vl + vt - vb);
  float l = texture(uPressure, vUv - vec2(uTexel.x, 0.0)).x;
  float r = texture(uPressure, vUv + vec2(uTexel.x, 0.0)).x;
  float b = texture(uPressure, vUv - vec2(0.0, uTexel.y)).x;
  float t = texture(uPressure, vUv + vec2(0.0, uTexel.y)).x;
  o = vec4(((l + r + b + t) * uWarm - d) * 0.25, 0.0, 0.0, 1.0);
}`;

// The pressure's gradient taken away (the flow free of divergence), and the result carried along by itself, a little
// of it lost: one pass, the free velocity worked out where it is sampled.
const CARRY = `${HEAD}
uniform sampler2D uVelocity;
uniform sampler2D uPressure;
uniform vec2 uTexel;
uniform float uDt;
uniform float uKeep;
out vec4 o;
vec2 free(vec2 uv) {
  float l = texture(uPressure, uv - vec2(uTexel.x, 0.0)).x;
  float r = texture(uPressure, uv + vec2(uTexel.x, 0.0)).x;
  float b = texture(uPressure, uv - vec2(0.0, uTexel.y)).x;
  float t = texture(uPressure, uv + vec2(0.0, uTexel.y)).x;
  return texture(uVelocity, uv).xy - vec2(r - l, t - b);
}
void main() {
  vec2 back = vUv - uDt * free(vUv) * uTexel;
  vec2 v = free(back) * uKeep;
  o = bad(v.x + v.y) ? vec4(0.0) : vec4(clamp(v, -${FASTEST}.0, ${FASTEST}.0), 0.0, 1.0);
}`;

// The light carried along by the velocity, at the buffer's own size, and as much of it kept as the fade would have. A
// sample that is not a number (anywhere in it) is nothing: carried, it would spread to its neighbours each frame.
const LIGHT = `${HEAD}
uniform sampler2D uVelocity;
uniform sampler2D uAcc;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
uniform vec2 uCell;
uniform float uDt;
uniform float uKeep;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
vec4 kept(vec4 v) { return bad(dot(v, vec4(1.0))) ? vec4(0.0) : v * uKeep; }
void main() {
  vec2 back = vUv - uDt * texture(uVelocity, vUv).xy * uCell;
  o = kept(texture(uAcc, back));
  bandA = kept(texture(uBandsA, back));
  bandB = kept(texture(uBandsB, back));
}`;

type Target = { texture: WebGLTexture; fbo: WebGLFramebuffer };

/** The fluid's grid for a buffer of `width` x `height`: about `cells` cells, as the buffer is shaped. */
export function gridFor(width: number, height: number, cells = 96 * 54): [number, number] {
  const aspect = Math.max(0.1, width / Math.max(1, height));
  const h = Math.max(8, Math.round(Math.sqrt(cells / aspect)));
  return [Math.max(8, Math.round(h * aspect)), h];
}

export class Fluid {
  private gl: WebGL2RenderingContext;
  private empty: WebGLVertexArrayObject;
  private programs = new Map<string, WebGLProgram>();
  private locations = new Map<WebGLProgram, Map<string, WebGLUniformLocation | null>>();
  private velocity2: [Target, Target] | null = null;
  private pressure2: [Target, Target] | null = null;
  private pending: Force[] = [];
  private lastDt = 0;
  width = 0;
  height = 0;

  /** Whether `gl` can hold the fluid: it draws into 16-bit float textures. */
  static supported(gl: WebGL2RenderingContext): boolean {
    return !!gl.getExtension('EXT_color_buffer_float');
  }

  /** The fluid on a grid of `gridW` x `gridH` cells (`gridFor`); throws where the device cannot hold it. */
  constructor(gl: WebGL2RenderingContext, gridW: number, gridH: number) {
    if (!Fluid.supported(gl)) throw new Error('This device cannot draw into float textures.');
    this.gl = gl;
    this.empty = gl.createVertexArray()!;
    const programs = { FORCE, JACOBI, CARRY, LIGHT };
    for (const [name, source] of Object.entries(programs)) this.programs.set(name, program(gl, VERTEX, source));
    this.resize(gridW, gridH);
  }

  /** The velocity now, a texture of the grid (RG: cells a second); `uvPerCell` turns it into texture coordinates. */
  get velocity(): WebGLTexture {
    return this.velocity2![0].texture;
  }

  /** Texture coordinates of the buffer per cell of the grid (x, y). */
  get uvPerCell(): [number, number] {
    return [1 / this.width, 1 / this.height];
  }

  /** The grid made anew at another size (still). */
  resize(gridW: number, gridH: number) {
    gridW = Math.max(8, Math.round(gridW));
    gridH = Math.max(8, Math.round(gridH));
    if (gridW === this.width && gridH === this.height) return;
    this.freeGrid();
    this.width = gridW;
    this.height = gridH;
    const gl = this.gl;
    this.velocity2 = [this.target(gl.RG16F, gl.RG), this.target(gl.RG16F, gl.RG)];
    this.pressure2 = [this.target(gl.R16F, gl.RED), this.target(gl.R16F, gl.RED)];
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** Forces for the next step (added to any not taken yet; the strongest `MOST_FORCES` are). */
  push(forces: Force[]) {
    this.pending.push(...forces.filter((f) => [f.x, f.y, f.dx, f.dy, f.radius, f.strength].every(Number.isFinite)));
  }

  /** One step of the flow, `dt` seconds on (at most a twentieth: a long pause does not throw it): two passes and the
   *  pressure's iterations, on the grid. */
  step(dt: number, flow: Flow) {
    const gl = this.gl;
    dt = Math.min(0.05, Math.max(0, dt));
    this.lastDt = dt;
    gl.disable(gl.BLEND);
    gl.bindVertexArray(this.empty);
    gl.viewport(0, 0, this.width, this.height);
    const texel = [1 / this.width, 1 / this.height] as const;
    const v = this.velocity2!;
    const p = this.pressure2!;

    const forces = this.pending.sort((a, b) => Math.abs(b.strength) - Math.abs(a.strength)).slice(0, MOST_FORCES);
    this.pending = [];
    const at = new Float32Array(MOST_FORCES * 4);
    const way = new Float32Array(MOST_FORCES * 2);
    forces.forEach((f, i) => {
      at.set([f.x, f.y, f.radius, f.strength], i * 4);
      way.set([f.dx, f.dy], i * 2);
    });
    this.draw('FORCE', v[1], { uVelocity: v[0].texture }, (u) => {
      gl.uniform2f(u('uTexel'), ...texel);
      gl.uniform1f(u('uAspect'), this.width / this.height);
      gl.uniform2f(u('uGrid'), this.width, this.height);
      gl.uniform1i(u('uCount'), forces.length);
      gl.uniform4fv(u('uAt'), at);
      gl.uniform2fv(u('uWay'), way);
      gl.uniform1f(u('uSwirl'), SWIRL * Math.max(0, flow.swirl));
      gl.uniform1f(u('uDt'), dt);
    });
    v.reverse();

    for (let i = 0; i < ITERATIONS; i++) {
      this.draw('JACOBI', p[1], { uPressure: p[0].texture, uVelocity: v[0].texture }, (u) => {
        gl.uniform2f(u('uTexel'), ...texel);
        gl.uniform1f(u('uWarm'), i === 0 ? PRESSURE_KEPT : 1);
      });
      p.reverse();
    }

    // How long the motion lasts: about three seconds at full energy, a third of one at none.
    const energy = Math.min(1, Math.max(0, flow.energy));
    const keep = Math.exp(-dt * (0.3 + 2.7 * (1 - energy)));
    this.draw('CARRY', v[1], { uVelocity: v[0].texture, uPressure: p[0].texture }, (u) => {
      gl.uniform2f(u('uTexel'), ...texel);
      gl.uniform1f(u('uDt'), dt);
      gl.uniform1f(u('uKeep'), keep);
    });
    v.reverse();
  }

  /** The light carried along by the flow into `dst` (a framebuffer of three targets as `light`'s, `width` x `height`),
   *  `keep` of it kept (as the fade keeps it), in place of the renderer's fade. Leaves blending off and `dst` bound. */
  advect(light: Light, dst: WebGLFramebuffer, keep: number, width: number, height: number) {
    const gl = this.gl;
    gl.disable(gl.BLEND);
    gl.bindVertexArray(this.empty);
    gl.bindFramebuffer(gl.FRAMEBUFFER, dst);
    gl.drawBuffers([gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1, gl.COLOR_ATTACHMENT2]);
    gl.viewport(0, 0, width, height);
    const prog = this.use('LIGHT');
    const u = (name: string) => this.location(prog, name);
    this.bind(prog, { uVelocity: this.velocity, uAcc: light.acc, uBandsA: light.bandsA, uBandsB: light.bandsB });
    gl.uniform2f(u('uCell'), ...this.uvPerCell);
    gl.uniform1f(u('uDt'), this.lastDt);
    gl.uniform1f(u('uKeep'), keep);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  dispose() {
    const gl = this.gl;
    this.freeGrid();
    for (const p of this.programs.values()) gl.deleteProgram(p);
    this.programs.clear();
    gl.deleteVertexArray(this.empty);
  }

  private freeGrid() {
    const gl = this.gl;
    for (const t of [...(this.velocity2 ?? []), ...(this.pressure2 ?? [])]) {
      if (!t) continue;
      gl.deleteFramebuffer(t.fbo);
      gl.deleteTexture(t.texture);
    }
    this.velocity2 = this.pressure2 = null;
  }

  private target(internal: number, format: number): Target {
    const gl = this.gl;
    const texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, this.width, this.height, 0, format, gl.HALF_FLOAT, null);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error('This device cannot draw into the fluid’s textures.');
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    return { texture, fbo };
  }

  private use(name: string): WebGLProgram {
    const p = this.programs.get(name)!;
    this.gl.useProgram(p);
    return p;
  }

  private location(p: WebGLProgram, name: string): WebGLUniformLocation | null {
    let m = this.locations.get(p);
    if (!m) this.locations.set(p, (m = new Map()));
    if (!m.has(name)) m.set(name, this.gl.getUniformLocation(p, name));
    return m.get(name) ?? null;
  }

  private bind(p: WebGLProgram, textures: Record<string, WebGLTexture>) {
    const gl = this.gl;
    Object.entries(textures).forEach(([name, texture], unit) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.uniform1i(this.location(p, name), unit);
    });
  }

  private draw(name: string, into: Target, textures: Record<string, WebGLTexture>, set: (u: (n: string) => WebGLUniformLocation | null) => void) {
    const gl = this.gl;
    const p = this.use(name);
    gl.bindFramebuffer(gl.FRAMEBUFFER, into.fbo);
    this.bind(p, textures);
    set((n) => this.location(p, n));
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}

function program(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const shader = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'A fluid shader did not compile.');
    return s;
  };
  const p = gl.createProgram()!;
  const [v, f] = [shader(gl.VERTEX_SHADER, vs), shader(gl.FRAGMENT_SHADER, fs)];
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'A fluid program did not link.');
  return p;
}
