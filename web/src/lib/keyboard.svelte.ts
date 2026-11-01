// The on-screen keyboard's height over the page (the owner, 2026-10-25: the bottom glass's search field went behind the
// keyboard): what floats along the bottom (App's lower glass row) stands just above it. In the app, Android's own
// measure, which the app tells (`keyboard`: its height in the page's pixels; the page is not made smaller for it). In a
// browser, the part of the page the keyboard hides: the window less what is still visible (the visual viewport). Kept as
// `--kb` on the page's root, 0 while it is down.
// In a browser the page is made smaller for the keyboard (index.html's interactive-widget=resizes-content; the owner,
// 2026-11-01: scrolling with the keyboard up carried the top glass off the screen, the page panning under it), so the
// keyboard covers none of it (`--kb` stays 0 where the browser does so); `open` says it is up either way: the window
// well short of its tallest at this width, with a field being typed in.
import { inApp, onApp } from './app.svelte';

export const keyboard = $state({ height: 0, open: false });

function set(px: number) {
  const h = Math.max(0, Math.round(px));
  if (inApp) keyboard.open = h > 0;
  if (h === keyboard.height) return;
  keyboard.height = h;
  document.documentElement.style.setProperty('--kb', `${h}px`);
}

/** The window's tallest at each width (an orientation), to tell the keyboard's shrinking of it. */
const tallest = new Map<number, number>();
const typingIn = () => {
  const el = document.activeElement as HTMLElement | null;
  return !!el && (el.isContentEditable || (el.tagName === 'INPUT' && !['button', 'checkbox', 'radio', 'range', 'submit'].includes((el as HTMLInputElement).type)) || el.tagName === 'TEXTAREA');
};
function shrunk() {
  const tall = Math.max(tallest.get(innerWidth) ?? 0, innerHeight);
  tallest.set(innerWidth, tall);
  keyboard.open = tall - innerHeight > 150 && typingIn();
}

if (typeof window !== 'undefined') {
  // The app tells the keyboard's height itself and its page is not made smaller: not twice.
  if (inApp) {
    const meta = document.querySelector<HTMLMetaElement>('meta[name="viewport"]');
    if (meta) meta.content = meta.content.replace(/,\s*interactive-widget=[\w-]+/, '');
  }
  if (inApp) onApp('keyboard', (m) => set(Number(m.height) || 0));
  else if (window.visualViewport) {
    const vv = window.visualViewport;
    const measure = () => (set(innerHeight - vv.height - vv.offsetTop), shrunk());
    vv.addEventListener('resize', measure);
    vv.addEventListener('scroll', measure);
    addEventListener('resize', measure);
    addEventListener('focusin', measure);
    addEventListener('focusout', () => setTimeout(measure, 0));
    measure();
  }
}
