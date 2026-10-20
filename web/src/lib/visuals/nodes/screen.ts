// The screen (decision 0057 step 3): a finished picture copied onto it, scaled up, and the flash meter reading what
// was shown (flashmeter.ts).
import { FlashMeter } from '../flashmeter';
import type { Tiles } from '../meters';
import { program } from '../sources/gl';
import { FULL, uniforms } from './shared';

// The picture slot's copy onto the screen: the finished picture (the buffer's size), scaled up as the show pass was.
const COPY = `#version 300 es
precision highp float;
uniform sampler2D uOut;
uniform vec2 uView;
out vec4 o;
void main() { o = vec4(textureLod(uOut, gl_FragCoord.xy / uView, 0.0).rgb, 1.0); }`;

export class Screen {
  private gl: WebGL2RenderingContext;
  private copying: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  // The flash meter (flashmeter.ts), made the first time the screen is measured; out where it cannot be made.
  private meter: FlashMeter | null = null;
  private meterOut = false;
  private u: (p: WebGLProgram, name: string) => WebGLUniformLocation | null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.u = uniforms(gl);
    this.copying = program(gl, FULL, COPY);
    this.empty = gl.createVertexArray()!;
  }

  /** A picture onto the screen, scaled up. */
  toScreen(picture: WebGLTexture, screenWidth: number, screenHeight: number) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, screenWidth, screenHeight);
    gl.disable(gl.BLEND);
    gl.useProgram(this.copying);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, picture);
    gl.uniform1i(this.u(this.copying, 'uOut'), 0);
    gl.uniform2f(this.u(this.copying, 'uView'), screenWidth, screenHeight);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** The screen just shown (`viewWidth` x `viewHeight`) read into tiles for the flash meter (meters.ts), as of `t`
   *  seconds; read back two or more frames later with `tiles`. None where this device cannot (it is then never tried
   *  again). */
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

  /** The screen's readings the GPU has finished, the oldest first. Once a frame, before `frame` or anything else draws
   *  (flashmeter.ts). */
  tiles(): Tiles[] {
    return this.meter?.poll() ?? [];
  }

  /** The flash meter's readings let go unread, its ring full. */
  get meterDropped(): number {
    return this.meter?.dropped ?? 0;
  }

  dispose() {
    this.meter?.dispose();
    this.meter = null;
    this.gl.deleteVertexArray(this.empty);
    this.gl.deleteProgram(this.copying);
  }
}
