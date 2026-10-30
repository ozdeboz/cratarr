// The visuals by themselves (0.10; the owner, 2026-10-30): where this device says so, while music plays and the screen
// is left alone a while, they open full screen, as a screensaver; a touch closes them (and does nothing else). Opened by
// hand they are as before. Not in local mode (decision 0052: no visuals there).
import { localMode } from './deviceMusic.svelte';
import { prefs } from './prefs.svelte';
import { playing, shownPaused } from './remote.svelte';
import { visuals } from './visuals/shown.svelte';

if (typeof window !== 'undefined') {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const plays = () => {
    const np = playing();
    return !!np && !shownPaused();
  };
  const arm = () => {
    clearTimeout(timer);
    if (!prefs.visualsAuto || visuals.open || localMode() || !plays()) return;
    timer = setTimeout(() => {
      if (!prefs.visualsAuto || visuals.open || localMode() || !plays()) return;
      visuals.auto = true;
      visuals.side = false;
      visuals.open = true;
    }, prefs.visualsAutoAfter);
  };
  // Any touch, key or wheel: the screen is in use. Over visuals that opened by themselves it closes them, and only that.
  const used = (e: Event) => {
    if (visuals.open && visuals.auto) {
      e.preventDefault();
      e.stopPropagation();
      visuals.open = false;
      // The tap's click would land on what lies under them, now shown: it is swallowed too.
      const eat = (c: Event) => (c.preventDefault(), c.stopPropagation());
      addEventListener('click', eat, { capture: true, once: true });
      setTimeout(() => removeEventListener('click', eat, { capture: true }), 600);
    }
    arm();
  };
  for (const t of ['pointerdown', 'keydown', 'wheel'] as const) addEventListener(t, used, { capture: true, passive: false });
  $effect.root(() => {
    // Again whenever what decides it changes: the setting, the music starting or pausing, the visuals closing.
    $effect(() => {
      void [prefs.visualsAuto, prefs.visualsAutoAfter, visuals.open, plays(), localMode()];
      if (!visuals.open) visuals.auto = false;
      arm();
    });
  });
}
