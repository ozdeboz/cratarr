// The picture as shown, read back by tiles for the flash meters (meters.ts): after each frame the screen is copied
// small (a blit: the screen cannot be sampled), each tile's mean colour worked out in linear light (relative luminance,
// CIE 1976 u'v', share of red), and read back two or more frames later without waiting for the GPU (a pixel buffer and
// a fence), so measuring never stalls a frame. `poll` goes first in a frame, before its own drawing: Chromium's
// getBufferSubData is always a round trip to the GPU process, so after the frame's draws it waits for that process to
// take all of them (the tablet's 18 frames a second against 30, 2026-10-20).
//
// Written to the Mali rules (decision 0047): no array constructors, no bits picking an answer. Each `capture` changes
// the framebuffers bound (read and draw), the viewport, the program, the vertex array and texture unit 0; the pixel
// pack buffer is left unbound. The renderer sets its own again after.

import type { Tiles } from './meters';

/** Each tile is sampled 16 x 16 times, each sample (bilinear) the mean of four pixels of the small copy. */
const SAMPLES = 16;
/** Readings in flight at once to start with, and at most (a browser may hold one back ten frames): with all of them
 *  pending, more are made, then the oldest reading is let go (counted in `dropped`) rather than waiting. */
const RING = 4;
const RING_MOST = 16;
/** A reading is read this many frames (polls) after its frame at the soonest. */
const AGE = 2;

const VERTEX = `#version 300 es
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

// One fragment a tile: its mean linear colour, then its luminance, chromaticity and red share. 8 bits each is plenty
// for steps of 0.1 and shifts of 0.2.
const TILES = `#version 300 es
precision highp float;
uniform sampler2D uSmall;
uniform vec2 uTiles;
out vec4 o;
vec3 linear(vec3 c) { return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c)); }
void main() {
  vec2 tile = floor(gl_FragCoord.xy);
  vec3 sum = vec3(0.0);
  for (int j = 0; j < ${SAMPLES}; j++) {
    for (int i = 0; i < ${SAMPLES}; i++) {
      vec2 uv = (tile + (vec2(float(i), float(j)) + 0.5) / ${SAMPLES}.0) / uTiles;
      sum += linear(texture(uSmall, uv).rgb);
    }
  }
  vec3 c = sum / ${SAMPLES * SAMPLES}.0;
  float l = dot(c, vec3(0.2126, 0.7152, 0.0722));
  float x = dot(c, vec3(0.4124, 0.3576, 0.1805));
  float z = dot(c, vec3(0.0193, 0.1192, 0.9505));
  float d = x + 15.0 * l + 3.0 * z;
  vec2 uv = d > 0.000001 ? vec2(4.0 * x, 9.0 * l) / d : vec2(0.1978, 0.4683);
  float s = c.r + c.g + c.b;
  o = vec4(l, uv / 0.65, s > 0.000001 ? c.r / s : 0.0);
}`;

type Pending = { buffer: WebGLBuffer; sync: WebGLSync | null; t: number; frame: number; cols: number; rows: number };

export class FlashMeter {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  private small: { texture: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number } | null = null;
  private tiles: { texture: WebGLTexture; fbo: WebGLFramebuffer } | null = null;
  private ring: Pending[] = [];
  private frame = 0;
  private uTiles: WebGLUniformLocation | null;
  private uSmall: WebGLUniformLocation | null;
  readonly cols: number;
  rows = 0;
  /** Readings let go unread, the ring full (to see whether it ever happens). */
  dropped = 0;

  /** `cols` tiles across; as many down as keep them square. */
  constructor(gl: WebGL2RenderingContext, cols = 16) {
    this.gl = gl;
    this.cols = cols;
    this.program = program(gl, VERTEX, TILES);
    this.uTiles = gl.getUniformLocation(this.program, 'uTiles');
    this.uSmall = gl.getUniformLocation(this.program, 'uSmall');
    this.empty = gl.createVertexArray()!;
    for (let i = 0; i < RING; i++) this.ring.push(this.slot());
  }

  /** The screen (`viewW` x `viewH`, just drawn) measured as of `t` seconds; read with `poll` two or more frames later. */
  capture(viewW: number, viewH: number, t = performance.now() / 1000) {
    const gl = this.gl;
    if (viewW < 1 || viewH < 1) return;
    let slot = this.ring.find((p) => !p.sync);
    if (!slot && this.ring.length < RING_MOST) this.ring.push((slot = this.slot()));
    if (!slot) {
      slot = this.ring.reduce((a, b) => (b.frame < a.frame ? b : a));
      gl.deleteSync(slot.sync);
      slot.sync = null;
      if (this.dropped++ === 0) console.warn(`Visuals: the flash meter let a reading go unread (${RING_MOST} in flight)`);
    }
    const rows = Math.max(2, Math.min(32, Math.round((this.cols * viewH) / viewW)));
    this.ensure(rows);
    const small = this.small!;
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, small.fbo);
    gl.blitFramebuffer(0, 0, viewW, viewH, 0, 0, small.w, small.h, gl.COLOR_BUFFER_BIT, gl.LINEAR);

    gl.bindFramebuffer(gl.FRAMEBUFFER, this.tiles!.fbo);
    gl.viewport(0, 0, this.cols, rows);
    gl.disable(gl.BLEND);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.empty);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, small.texture);
    gl.uniform1i(this.uSmall, 0);
    gl.uniform2f(this.uTiles, this.cols, rows);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, slot.buffer);
    gl.bufferData(gl.PIXEL_PACK_BUFFER, this.cols * rows * 4, gl.STREAM_READ);
    gl.readPixels(0, 0, this.cols, rows, gl.RGBA, gl.UNSIGNED_BYTE, 0);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, null);
    slot.sync = gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE, 0);
    gl.flush();
    Object.assign(slot, { t, frame: this.frame, cols: this.cols, rows });
  }

  /** The readings the GPU has finished, at least AGE frames old, the oldest first (none: an empty list). Once a frame,
   *  before it draws anything. */
  poll(): Tiles[] {
    const gl = this.gl;
    this.frame++;
    const ready = this.ring
      .filter((p) => p.sync && this.frame - p.frame >= AGE && gl.getSyncParameter(p.sync, gl.SYNC_STATUS) === gl.SIGNALED)
      .sort((a, b) => a.t - b.t);
    return ready.map((p) => {
      const n = p.cols * p.rows;
      const bytes = new Uint8Array(n * 4);
      gl.bindBuffer(gl.PIXEL_PACK_BUFFER, p.buffer);
      gl.getBufferSubData(gl.PIXEL_PACK_BUFFER, 0, bytes);
      gl.bindBuffer(gl.PIXEL_PACK_BUFFER, null);
      gl.deleteSync(p.sync);
      p.sync = null;
      const tiles: Tiles = {
        t: p.t,
        cols: p.cols,
        rows: p.rows,
        luminance: new Float32Array(n),
        u: new Float32Array(n),
        v: new Float32Array(n),
        red: new Float32Array(n),
      };
      for (let i = 0; i < n; i++) {
        tiles.luminance[i] = bytes[i * 4] / 255;
        tiles.u[i] = (bytes[i * 4 + 1] / 255) * 0.65;
        tiles.v[i] = (bytes[i * 4 + 2] / 255) * 0.65;
        tiles.red[i] = bytes[i * 4 + 3] / 255;
      }
      return tiles;
    });
  }

  dispose() {
    const gl = this.gl;
    for (const p of this.ring) {
      if (p.sync) gl.deleteSync(p.sync);
      gl.deleteBuffer(p.buffer);
    }
    this.freeTargets();
    gl.deleteProgram(this.program);
    gl.deleteVertexArray(this.empty);
  }

  private slot(): Pending {
    return { buffer: this.gl.createBuffer()!, sync: null, t: 0, frame: 0, cols: 0, rows: 0 };
  }

  private ensure(rows: number) {
    if (rows === this.rows && this.small) return;
    this.freeTargets();
    this.rows = rows;
    const w = this.cols * SAMPLES * 2;
    const h = rows * SAMPLES * 2;
    this.small = { ...this.target(w, h), w, h };
    this.tiles = this.target(this.cols, rows);
  }

  private target(w: number, h: number): { texture: WebGLTexture; fbo: WebGLFramebuffer } {
    const gl = this.gl;
    const texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    return { texture, fbo };
  }

  private freeTargets() {
    const gl = this.gl;
    for (const t of [this.small, this.tiles]) {
      if (!t) continue;
      gl.deleteFramebuffer(t.fbo);
      gl.deleteTexture(t.texture);
    }
    this.small = this.tiles = null;
  }
}

function program(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const shader = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'The flash meter’s shader did not compile.');
    return s;
  };
  const p = gl.createProgram()!;
  const [v, f] = [shader(gl.VERTEX_SHADER, vs), shader(gl.FRAGMENT_SHADER, fs)];
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'The flash meter’s program did not link.');
  return p;
}
