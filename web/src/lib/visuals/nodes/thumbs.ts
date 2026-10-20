// A picture as a thumbnail on a 2D canvas (the graph's, decision 0057): reduced twice by a quarter of each side, each
// pixel the brightest of the 4×4 it covers per channel (a thin bright line, Waves' or a Shape's outline, stays; an
// average would lose it), upright; read back into one reused buffer, put on the canvas. Two small passes, one read.
import { program } from '../sources/gl';
import { quarter, thumbSize } from '../thumbnails';
import { FULL } from './shared';

// Taps one by one (no arrays: the Mali rules), level 0; `uFlip` turns it upright on the last step.
const REDUCE = `#version 300 es
precision highp float;
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

type Small = { tex: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number };

export class Thumbs {
  private gl: WebGL2RenderingContext;
  private reduce: WebGLProgram | null = null;
  private empty: WebGLVertexArrayObject | null = null;
  /** The quarter step's target and the thumbnail's, each made again only when its size changes. */
  private mid: Small | null = null;
  private at: Small | null = null;
  private pixels: ImageData | null = null;
  private bytes: Uint8Array | null = null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
  }

  /** `picture` (of `width` x `height`) onto `canvas`, at about a sixteenth of its sides (the canvas sized so). */
  draw(picture: WebGLTexture, width: number, height: number, canvas: HTMLCanvasElement): boolean {
    const ctx = canvas.getContext('2d');
    if (!ctx) return false;
    const gl = this.gl;
    const [mw, mh] = quarter(width, height);
    const [w, h] = thumbSize(width, height);
    this.reduce ??= program(gl, FULL, REDUCE);
    this.empty ??= gl.createVertexArray()!;
    this.mid = this.small(this.mid, mw, mh);
    const fresh = !this.at || this.at.w !== w || this.at.h !== h;
    this.at = this.small(this.at, w, h);
    if (fresh) {
      this.pixels = new ImageData(w, h);
      this.bytes = new Uint8Array(this.pixels.data.buffer);
    }
    gl.disable(gl.BLEND);
    gl.useProgram(this.reduce);
    gl.bindVertexArray(this.empty);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1i(gl.getUniformLocation(this.reduce, 'uIn'), 0);
    this.step(picture, [width, height], this.mid, 0);
    this.step(this.mid.tex, [mw, mh], this.at, 1);
    gl.bindVertexArray(null);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, this.bytes);
    if (canvas.width !== w || canvas.height !== h) [canvas.width, canvas.height] = [w, h];
    ctx.putImageData(this.pixels!, 0, 0);
    return true;
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
    if (this.reduce) this.gl.deleteProgram(this.reduce);
    if (this.empty) this.gl.deleteVertexArray(this.empty);
  }
}
