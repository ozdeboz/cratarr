// The graphics chip, once at start (the owner, 2026-10-26: "progressively enhance so we don't overload the tab"): a
// weak one (Arm's Mali, as the tablet's G52 and the phone's G615: they drew specks around a drop-shadow filter and a
// fraction-of-a-pixel shift, the tablet's player) marks the page `data-gpu="lite"`, and the effects that cost it (a
// filter's own layer) are left out there (lib/NowPlaying). Unknown: the full page.
export function markGpu() {
  try {
    const c = document.createElement('canvas');
    const gl = c.getContext('webgl');
    if (!gl) return;
    const info = gl.getExtension('WEBGL_debug_renderer_info');
    const renderer = String(info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    if (/mali/i.test(renderer)) document.documentElement.dataset.gpu = 'lite';
  } catch {
    /* the full page */
  }
}
