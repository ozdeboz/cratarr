// The flame's light (decision 0046; 0057 step 3: the `light` the flame's own loop works on, before Tone map): its
// accumulation buffers, two sets of three targets (the colours and density, and the bands' shares), resized with the
// picture so far carried over, every two seconds scrubbed of anything not a number. Its points (points.ts) draw into
// the set in use; the loop (carry.ts) copies it into the other and makes that the one in use.
import { program } from '../sources/gl';
import { BAD, FULL } from './shared';

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

export class Light {
  private gl: WebGL2RenderingContext;
  /** The buffers, two sets: the one in use (`cur`) and the one the next scrub copies into. */
  accs: WebGLTexture[] = [];
  bandSets: WebGLTexture[] = [];
  bandSetsB: WebGLTexture[] = [];
  fbos: WebGLFramebuffer[] = [];
  cur = 0;
  private scrubbing: WebGLProgram;
  private carrying: WebGLProgram;
  private finding: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  /** The buffers being replaced at a resize, their picture carried into the new ones. */
  private carryFrom: { accs: WebGLTexture[]; bands: WebGLTexture[]; bandsB: WebGLTexture[]; fbos: WebGLFramebuffer[]; cur: number; width: number; height: number } | null = null;
  private query: WebGLQuery | null = null;
  private frames = 0;
  /** How many scrubs found anything not a number or infinite in the buffers (the black specks' cause, if they come). */
  scrubbed = 0;
  acc: WebGLTexture | null = null;
  /** Beside `acc`: each pixel's points by band. */
  bands: WebGLTexture | null = null;
  bandsB: WebGLTexture | null = null;
  /** The colours buffer's levels above its first (0 where they cannot be filtered). */
  levels = 0;
  accFbo: WebGLFramebuffer | null = null;
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
  width = 0;
  height = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    const floats = !!gl.getExtension('EXT_color_buffer_float');
    this.precision =
      floats && gl.getExtension('EXT_float_blend') && gl.getExtension('OES_texture_float_linear') ? 32 : floats || gl.getExtension('EXT_color_buffer_half_float') ? 16 : 8;
    this.scrubbing = program(gl, FULL, SCRUB);
    this.finding = program(gl, FULL, FIND);
    this.carrying = program(gl, FULL, CARRY);
    this.empty = gl.createVertexArray()!;
  }

  /** The buffers' size in pixels. */
  resize(width: number, height: number) {
    if (width === this.width && height === this.height) return;
    const gl = this.gl;
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
  use() {
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

  /** Both sets to nothing (the flame drawn again after the lazy rules cut it: it gathers its light anew). */
  clear() {
    const gl = this.gl;
    gl.clearColor(0, 0, 0, 0);
    for (const f of this.fbos) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, f);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** Every 120 frames (two to four seconds), the scrub; first the last one's answer, where the GPU has it. */
  tend() {
    this.heard();
    if (++this.frames % 120 === 0) this.scrub();
  }

  dispose() {
    const gl = this.gl;
    gl.deleteVertexArray(this.empty);
    for (const x of [...this.accs, ...this.bandSets, ...this.bandSetsB]) gl.deleteTexture(x);
    for (const f of this.fbos) gl.deleteFramebuffer(f);
    if (this.query) gl.deleteQuery(this.query);
    for (const p of [this.scrubbing, this.finding, this.carrying]) gl.deleteProgram(p);
  }
}
