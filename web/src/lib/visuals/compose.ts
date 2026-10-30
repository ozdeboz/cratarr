// What the compose step's walk (nodes/walker.ts) draws with besides its own modules (decision 0056 step 4; 0057 step
// 3): the picture modules it runs from patch.ts's (a layer's engines, the fluid in the loop), a source's audio
// inputs, the Mix in one pass and the picture's own loop as one pass (MilkDrop's feedback).
import { WARP_GLSL } from './loopmods';
import type { FrameInfo, PictureModule } from './patch';
import { SOURCE_TYPES } from './sources/index';
import { portsOf, type PortRef } from './ports';
import { createFluidLoop, FLUID_LOOP } from './fluid-loop';
import { createEngine } from './engine-module';

/** What an `audio` port's normal gives this frame: Music's sound (the frame's samples); nothing else gives one yet. */
const audioOf = (n: PortRef | undefined, frame: FrameInfo) => (n?.module === 'music' && n.port === 'audio' ? frame.waveform : null);
/** Each source type's audio inputs (0057 §1), by type. */
const AUDIO_IN = new Map(SOURCE_TYPES.map((t) => [t.type, portsOf(t).in.filter((p) => p.type === 'audio')]));

/** The frame as a module of `type` takes it: with its typed inputs (its audio ports' samples, from their normals; no
 *  cable carries sound yet), or as it is where it has none. */
export function withInputs(type: string, frame: FrameInfo): FrameInfo {
  const ports = AUDIO_IN.get(type);
  if (!ports?.length) return frame;
  return { ...frame, inputs: Object.fromEntries(ports.map((p) => [p.id, audioOf(p.normal, frame)])) };
}

/** The picture modules patch.ts's types run by: the engines a layer carries after its source, and the fluid in the
 *  picture's own loop. Each draws `input` into `out`, an exact copy at its neutral. */
export const MODULE_MAKERS: Record<string, (gl: WebGL2RenderingContext) => PictureModule> = {
  [FLUID_LOOP.type]: () => createFluidLoop(),
  // The effect engines on a layer (engine-module.ts): a kaleidoscope on the shapes.
  'kaleidoscope-engine': (gl) => createEngine('kaleidoscope-engine', gl),
  'eyedo-engine': (gl) => createEngine('eyedo-engine', gl),
  'hydra-engine': (gl) => createEngine('hydra-engine', gl),
};

/** The Mix of `n` layers over the picture in one pass (decision 0057 §4), each over the picture so far (both
 *  premultiplied) as its mode says (MIX_MODES): added, screened, laid over, multiplied, the picture seen through it
 *  (window: dimmed by its amount where the layer is not), or the brighter of the two (lighten), by its amount; the
 *  picture so far kept in the shader, not in a target between. A layer's glow and tint are modules of their own
 *  (nodes/modules.ts: Glow, Tint). No arrays (Mali's float[] miscompiles). */
export function mixShader(n: number): string {
  const layer = (i: number) => `  {
    vec4 b = texture(uLayer${i}, vUv);
    float cover = clamp(max(max(b.r, b.g), max(b.b, b.a)) * 1.5, 0.0, 1.0);
    b *= uAmount${i};
    if (uMode${i} == 1) c = c + b.rgb - c * b.rgb;
    else if (uMode${i} == 2) c = b.rgb + c * (1.0 - b.a);
    else if (uMode${i} == 3) c = c * mix(vec3(1.0), b.rgb / max(b.a, 1e-4), b.a);
    else if (uMode${i} == 4) c = c * mix(1.0 - uAmount${i}, 1.0, cover);
    else if (uMode${i} == 5) c = max(c, b.rgb);
    else if (uMode${i} == 6) c = max(vec3(0.0), c - b.rgb);
    else c = c + b.rgb;
  }`;
  const each = (f: (i: number) => string) => Array.from({ length: n }, (_, i) => f(i)).join('\n');
  return `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 outColor;
uniform sampler2D uBase;
uniform float uBaseGain;
${each((i) => `uniform sampler2D uLayer${i};\nuniform int uMode${i};\nuniform float uAmount${i};`)}
void main() {
  vec3 c = texture(uBase, vUv).rgb * uBaseGain;
${each(layer)}
  outColor = vec4(c, 1.0);
}`;
}

// The picture's own loop: the last picture read where the Warp moves it from (about its centre: zoomed and turned,
// alike or by place as its grid says, stretched, wobbled, drifted: loopmods.ts WARP_GLSL), kept by the Keep's share,
// and the new picture over it: the brighter of the two, so the trails fade as they are kept and never build past the
// picture itself (our layers are whole pictures each frame, not MilkDrop's few lines; added up they would run to
// white).
export const LOOP = `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 outColor;
uniform sampler2D uNow;
uniform sampler2D uLast;
uniform float uAspect;
uniform vec2 uDrift;
uniform float uKeep;
${WARP_GLSL}
void main() {
  vec2 p = warpFrom((vUv - 0.5) * vec2(uAspect, 1.0));
  vec2 uv = p / vec2(uAspect, 1.0) + 0.5 - uDrift;
  vec3 last = textureLod(uLast, uv, 0.0).rgb;
  vec3 now = texture(uNow, vUv).rgb;
  outColor = vec4(max(now, uKeep * last), 1.0);
}`;
