// One glass on a phone (the owner, 2026-10-25: "for proper animation, they should probably be the same component"):
// where the bar of pages runs along the top, the page's and the panel's floating glass are never shown together, so
// App draws one: back (or ✕) and the title on the left, one pill on the right. The panel on top hands its parts here
// (its left part, its pill's settings, a button of its own); else the page's show. One pill whose settings change, so
// its buttons slide in and out as they do (lib/TopPill), instead of one glass fading over another. With room (the
// sidebar), each keeps its own glass, as before.
import type { Snippet } from 'svelte';
import type TopPill from './TopPill.svelte';
import type { ComponentProps } from 'svelte';

export type PillSettings = ComponentProps<typeof TopPill>;
/** What a panel hands: its left part (back or ✕, its breadcrumbs or name), its pill's settings (none: no pill), and a
 *  button of its own beside the pill. */
export type GlassParts = {
  left: Snippet;
  pill: () => PillSettings | null;
  extra?: Snippet | null;
  /** The panel's action bar has the top (stuck): the glass goes just under it, as the panel's own pills did. */
  below?: () => boolean;
};

export const glass = $state<{ one: boolean; panel: GlassParts | null }>({ one: false, panel: null });

/** A panel beside its list (from 1024 px), not over it (a phone): only then does a list mark the item open in it (the
 *  owner, 2026-10-25: on a phone the mark showed through the panel fading in, frames before it was there). */
export const panelBeside = () => !glass.one;

/** A panel's glass handed to App while `when()` holds (on a phone, its layer on top); taken back when it does not or the
 *  panel goes. Called in the panel's script. */
export function handGlass(when: () => boolean, parts: GlassParts) {
  $effect(() => {
    if (!glass.one || !when()) return;
    glass.panel = parts;
    return () => {
      if (glass.panel === parts) glass.panel = null;
    };
  });
}
