// What this device draws with (the owner, 2026-10-26: "progressively enhance so we don't overload the tab, and have
// higher-spec devices run it with shaders"), once at start: `lite` for a weak chip (Arm's Mali, as the tablet's G52 and
// the phone's G615: they drew specks around a drop-shadow filter and a fraction-of-a-pixel shift on the tablet's player),
// or for a device that says it has little memory or few cores; `full` for the rest, where effects may be drawn with
// WebGL shaders. On the page as data-gpu (CSS: `:root[data-gpu='lite']` leaves a costly effect out) and as `gpu.tier`.
export const gpu = { tier: 'full' as 'lite' | 'full', renderer: '' };

export function markGpu() {
  try {
    const c = document.createElement('canvas');
    const gl = c.getContext('webgl');
    if (gl) {
      const info = gl.getExtension('WEBGL_debug_renderer_info');
      gpu.renderer = String(info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    }
    const memory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
    const weak = !gl || /mali/i.test(gpu.renderer) || (memory !== undefined && memory <= 3) || (navigator.hardwareConcurrency ?? 8) <= 4;
    gpu.tier = weak ? 'lite' : 'full';
  } catch {
    gpu.tier = 'lite';
  }
  document.documentElement.dataset.gpu = gpu.tier;
}
