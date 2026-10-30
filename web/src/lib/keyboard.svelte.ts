// The on-screen keyboard's height over the page (the owner, 2026-10-25: the bottom glass's search field went behind the
// keyboard): what floats along the bottom (App's lower glass row) stands just above it. In the app, Android's own
// measure, which the app tells (`keyboard`: its height in the page's pixels; the page is not made smaller for it). In a
// browser, the part of the page the keyboard hides: the window less what is still visible (the visual viewport). Kept as
// `--kb` on the page's root, 0 while it is down.
import { inApp, onApp } from './app.svelte';

export const keyboard = $state({ height: 0 });

function set(px: number) {
  const h = Math.max(0, Math.round(px));
  if (h === keyboard.height) return;
  keyboard.height = h;
  document.documentElement.style.setProperty('--kb', `${h}px`);
}

if (typeof window !== 'undefined') {
  if (inApp) onApp('keyboard', (m) => set(Number(m.height) || 0));
  else if (window.visualViewport) {
    const vv = window.visualViewport;
    const measure = () => set(innerHeight - vv.height - vv.offsetTop);
    vv.addEventListener('resize', measure);
    vv.addEventListener('scroll', measure);
  }
}
