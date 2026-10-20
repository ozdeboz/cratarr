// What the renderer's nodes share (decision 0057, step 3): the full-screen triangle, the test for a value not a
// number or infinite, and uniform locations looked up once.
export const FULL = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

// Not a number, or infinite, from the bits (see ITERATE's).
// A value not a number is nothing; an infinite one, the most a 16-bit float holds short of infinity (by its sign: a
// comparison is false for one not a number). The Mali-G615's compiler gets a fragment shader wrong that tests the bits
// and picks its answer from them too (0 came out as 60000, 0.12 as 0: a phone's picture white, with coloured noise;
// measured 2026-10-18), so the bits only answer whether.
export const BAD = `bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }
float clean1(float x) { return bad(x) ? (x > 0.0 ? 60000.0 : 0.0) : clamp(x, 0.0, 60000.0); }
vec4 clean(vec4 v) { return vec4(clean1(v.x), clean1(v.y), clean1(v.z), clean1(v.w)); }`;

/** A program's uniform locations, each looked up once. */
export function uniforms(gl: WebGL2RenderingContext) {
  const locs = new WeakMap<WebGLProgram, Map<string, WebGLUniformLocation | null>>();
  return (p: WebGLProgram, name: string): WebGLUniformLocation | null => {
    let at = locs.get(p);
    if (!at) locs.set(p, (at = new Map()));
    if (!at.has(name)) at.set(name, gl.getUniformLocation(p, name));
    return at.get(name) ?? null;
  };
}
