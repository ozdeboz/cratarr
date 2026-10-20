// A picture as a thumbnail on a 2D canvas (the graph's, decision 0057): drawn down into one small target (four taps
// a pixel, upright), read back into one reused buffer, put on the canvas. One small pass and one small read.
import { program } from '../sources/gl';
import { thumbSize } from '../thumbnails';
import { FULL } from './shared';

const DOWN = `#version 300 es
precision highp float;
uniform sampler2D uIn;
uniform vec2 uView;
out vec4 o;
void main() {
  vec2 uv = gl_FragCoord.xy / uView;
  uv.y = 1.0 - uv.y;
  vec2 d = 0.25 / uView;
  vec3 c = texture(uIn, uv + vec2(-d.x, -d.y)).rgb + texture(uIn, uv + vec2(d.x, -d.y)).rgb + texture(uIn, uv + vec2(-d.x, d.y)).rgb + texture(uIn, uv + d).rgb;
  o = vec4(clamp(c * 0.25, 0.0, 1.0), 1.0);
}`;

export class Thumbs {
  private gl: WebGL2RenderingContext;
  private down: WebGLProgram | null = null;
  private empty: WebGLVertexArrayObject | null = null;
  private at: { tex: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number } | null = null;
  private pixels: ImageData | null = null;
  private bytes: Uint8Array | null = null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
  }

  /** `picture` (of `width` x `height`) onto `canvas`, at a sixteenth of its sides (the canvas sized so). */
  draw(picture: WebGLTexture, width: number, height: number, canvas: HTMLCanvasElement): boolean {
    const ctx = canvas.getContext('2d');
    if (!ctx) return false;
    const gl = this.gl;
    const [w, h] = thumbSize(width, height);
    this.down ??= program(gl, FULL, DOWN);
    this.empty ??= gl.createVertexArray()!;
    if (this.at && (this.at.w !== w || this.at.h !== h)) this.free();
    if (!this.at) {
      const tex = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      const fbo = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
      gl.bindTexture(gl.TEXTURE_2D, null);
      this.at = { tex, fbo, w, h };
      this.pixels = new ImageData(w, h);
      this.bytes = new Uint8Array(this.pixels.data.buffer);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.at.fbo);
    gl.viewport(0, 0, w, h);
    gl.disable(gl.BLEND);
    gl.useProgram(this.down);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, picture);
    gl.uniform1i(gl.getUniformLocation(this.down, 'uIn'), 0);
    gl.uniform2f(gl.getUniformLocation(this.down, 'uView'), w, h);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, this.bytes);
    if (canvas.width !== w || canvas.height !== h) [canvas.width, canvas.height] = [w, h];
    ctx.putImageData(this.pixels!, 0, 0);
    return true;
  }

  private free() {
    if (!this.at) return;
    this.gl.deleteTexture(this.at.tex);
    this.gl.deleteFramebuffer(this.at.fbo);
    this.at = null;
  }

  dispose() {
    this.free();
    if (this.down) this.gl.deleteProgram(this.down);
    if (this.empty) this.gl.deleteVertexArray(this.empty);
  }
}
