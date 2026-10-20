// What the sources share on the GPU: a program, a mesh of triangles rebuilt each frame (no instancing, no vertex
// textures: plain attributes, as safe as it gets on the Malis), and the target made ready.

/** `feedback`: the varyings a transform feedback captures (interleaved). */
export function program(gl: WebGL2RenderingContext, vs: string, fs: string, feedback?: string[]): WebGLProgram {
  const shader = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'The shader did not compile.');
    return s;
  };
  const p = gl.createProgram()!;
  const [v, f] = [shader(gl.VERTEX_SHADER, vs), shader(gl.FRAGMENT_SHADER, fs)];
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  if (feedback) gl.transformFeedbackVaryings(p, feedback, gl.INTERLEAVED_ATTRIBS);
  gl.linkProgram(p);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'The program did not link.');
  return p;
}

/** Triangles from floats a vertex, the attributes at locations 0, 1, … of the sizes given. */
export class Mesh {
  readonly stride: number;
  private vao: WebGLVertexArrayObject;
  private buffer: WebGLBuffer;

  constructor(gl: WebGL2RenderingContext, sizes: number[]) {
    this.stride = sizes.reduce((a, b) => a + b, 0);
    this.vao = gl.createVertexArray()!;
    this.buffer = gl.createBuffer()!;
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    let at = 0;
    sizes.forEach((size, i) => {
      gl.enableVertexAttribArray(i);
      gl.vertexAttribPointer(i, size, gl.FLOAT, false, this.stride * 4, at * 4);
      at += size;
    });
    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
  }

  draw(gl: WebGL2RenderingContext, data: Float32Array, vertices: number) {
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data.subarray(0, vertices * this.stride), gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    gl.bindVertexArray(this.vao);
    gl.drawArrays(gl.TRIANGLES, 0, vertices);
    gl.bindVertexArray(null);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteVertexArray(this.vao);
    gl.deleteBuffer(this.buffer);
  }
}

/** The two triangles of a quad, as corners (−1..1). */
export const QUAD = [-1, -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, 1];

/** The target bound at its size and cleared to nothing: a source draws its layer alone (premultiplied). */
export function target(gl: WebGL2RenderingContext, out: WebGLFramebuffer | null, width: number, height: number) {
  gl.bindFramebuffer(gl.FRAMEBUFFER, out);
  gl.viewport(0, 0, width, height);
  gl.disable(gl.BLEND);
  gl.clearColor(0, 0, 0, 0);
  gl.clear(gl.COLOR_BUFFER_BIT);
}

/** GL left as the renderer expects it: no blending, the usual equation (each of its passes sets its own factors). */
export function done(gl: WebGL2RenderingContext) {
  gl.disable(gl.BLEND);
  gl.blendEquation(gl.FUNC_ADD);
}
