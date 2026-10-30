// The screen (decision 0057 step 3): a finished picture copied onto it, scaled up. What it shows is measured by the
// Meter (meter.ts).
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

  dispose() {
    this.gl.deleteVertexArray(this.empty);
    this.gl.deleteProgram(this.copying);
  }
}
