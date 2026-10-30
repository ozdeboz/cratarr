// The observed module's picture as a thumbnail on a 2D canvas (the graph's preview, decision 0057): reduced twice by a
// quarter of each side, each pixel the brightest of the 4×4 it covers per channel (a thin bright line, Waves' or a
// Shape's outline, stays; an average would lose it), upright; read back two or more frames later through a pixel
// buffer and a fence (flashmeter.ts's way), so a preview never stalls a frame. One reading in flight at a time.
import { program } from '../sources/gl';
import { quarter, thumbSize } from '../thumbnails';
import { FULL } from './shared';

// Taps one by one (no arrays: the Mali rules), level 0; `uFlip` turns it upright on the last step. Ints highp: a
// fragment shader's are mediump by default, 16 bits on Mali, and `q * uFrom` (215 x 862 at the tablet's size) wraps, the
// preview a few repeated pieces of the picture.
const REDUCE = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D uIn;
uniform ivec2 uFrom;
uniform ivec2 uTo;
uniform int uFlip;
out vec4 o;
vec4 at(ivec2 p) { return texelFetch(uIn, min(p, uFrom - 1), 0); }
vec4 row(ivec2 p) { return max(max(at(p), at(p + ivec2(1, 0))), max(at(p + ivec2(2, 0)), at(p + ivec2(3, 0)))); }
void main() {
  ivec2 q = ivec2(gl_FragCoord.xy);
  if (uFlip == 1) q.y = uTo.y - 1 - q.y;
  ivec2 b = (q * uFrom) / uTo;
  vec4 m = max(max(row(b), row(b + ivec2(0, 1))), max(row(b + ivec2(0, 2)), row(b + ivec2(0, 3))));
  o = uFlip == 1 ? vec4(clamp(m.rgb, 0.0, 1.0), 1.0) : m;
}`;

/** A reading is read this many frames (polls) after its capture at the soonest. */
const AGE = 2;

type Small = { tex: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number };

export class Thumbs {
  private gl: WebGL2RenderingContext;
  private reduce: WebGLProgram | null = null;
  private empty: WebGLVertexArrayObject | null = null;
  /** The quarter step's target and the thumbnail's, each made again only when its size changes. */
  private mid: Small | null = null;
  private at: Small | null = null;
  private buffer: WebGLBuffer | null = null;
  /** The reading in flight: whose, its fence, the frame it was captured in, its size. */
  private pending: { key: string; sync: WebGLSync; frame: number; w: number; h: number } | null = null;
  private frame = 0;
  /** The last reading read: whose, its pixels. */
  private ready: { key: string; image: ImageData } | null = null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
  }

  /** Whether a capture would be taken now (none while one is in flight). */
  get idle(): boolean {
    return !this.pending;
  }

  /** `picture` (of `width` x `height`, `key`'s) reduced to about a sixteenth of its sides and its read-back started. */
  capture(key: string, picture: WebGLTexture, width: number, height: number) {
    if (this.pending) return;
    const gl = this.gl;
    const [mw, mh] = quarter(width, height);
    const [w, h] = thumbSize(width, height);
    this.reduce ??= program(gl, FULL, REDUCE);
    this.empty ??= gl.createVertexArray()!;
    this.buffer ??= gl.createBuffer()!;
    this.mid = this.small(this.mid, mw, mh);
    this.at = this.small(this.at, w, h);
    gl.disable(gl.BLEND);
    gl.useProgram(this.reduce);
    gl.bindVertexArray(this.empty);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1i(gl.getUniformLocation(this.reduce, 'uIn'), 0);
    this.step(picture, [width, height], this.mid, 0);
    this.step(this.mid.tex, [mw, mh], this.at, 1);
    gl.bindVertexArray(null);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, this.buffer);
    gl.bufferData(gl.PIXEL_PACK_BUFFER, w * h * 4, gl.STREAM_READ);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, 0);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, null);
    this.pending = { key, sync: gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE, 0)!, frame: this.frame, w, h };
    gl.flush();
  }

  /** The reading in flight read, where the GPU has finished it. Once a frame, before anything draws. */
  poll() {
    const gl = this.gl;
    this.frame++;
    const p = this.pending;
    if (!p || this.frame - p.frame < AGE || gl.getSyncParameter(p.sync, gl.SYNC_STATUS) !== gl.SIGNALED) return;
    const image = this.ready?.image.width === p.w && this.ready.image.height === p.h ? this.ready.image : new ImageData(p.w, p.h);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, this.buffer);
    gl.getBufferSubData(gl.PIXEL_PACK_BUFFER, 0, new Uint8Array(image.data.buffer));
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, null);
    gl.deleteSync(p.sync);
    this.pending = null;
    this.ready = { key: p.key, image };
  }

  /** `key`'s last reading onto `canvas` (sized to it): false where there is none. */
  put(key: string, canvas: HTMLCanvasElement): boolean {
    const r = this.ready;
    const ctx = r?.key === key ? canvas.getContext('2d') : null;
    if (!r || !ctx) return false;
    if (canvas.width !== r.image.width || canvas.height !== r.image.height) [canvas.width, canvas.height] = [r.image.width, r.image.height];
    ctx.putImageData(r.image, 0, 0);
    return true;
  }

  /** Lets go of the last reading (what it showed is gone). */
  forget() {
    this.ready = null;
  }

  private step(from: WebGLTexture, size: [number, number], to: Small, flip: number) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, to.fbo);
    gl.viewport(0, 0, to.w, to.h);
    gl.bindTexture(gl.TEXTURE_2D, from);
    gl.uniform2i(gl.getUniformLocation(this.reduce!, 'uFrom'), size[0], size[1]);
    gl.uniform2i(gl.getUniformLocation(this.reduce!, 'uTo'), to.w, to.h);
    gl.uniform1i(gl.getUniformLocation(this.reduce!, 'uFlip'), flip);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /** `t` at `w` x `h`: as it is, or made again. */
  private small(t: Small | null, w: number, h: number): Small {
    if (t && t.w === w && t.h === h) return t;
    const gl = this.gl;
    this.free(t);
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    gl.bindTexture(gl.TEXTURE_2D, null);
    return { tex, fbo, w, h };
  }

  private free(t: Small | null) {
    if (!t) return;
    this.gl.deleteTexture(t.tex);
    this.gl.deleteFramebuffer(t.fbo);
  }

  dispose() {
    this.free(this.mid);
    this.free(this.at);
    this.mid = this.at = null;
    if (this.pending) this.gl.deleteSync(this.pending.sync);
    this.pending = null;
    if (this.buffer) this.gl.deleteBuffer(this.buffer);
    if (this.reduce) this.gl.deleteProgram(this.reduce);
    if (this.empty) this.gl.deleteVertexArray(this.empty);
  }
}
