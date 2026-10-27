// What stands behind every flow (the owner, 2026-10-26): one of Cratarr's photos (public/backdrops,
// from Unsplash: CREDITS.txt there), or one's own picture (lib/localCovers, kept on this device). App puts it on the
// page (--flow-photo); Home's sky takes it too where asked (Also on Home).
import { HOME_HEADER, localCovers } from './localCovers.svelte';
import { prefs } from './prefs.svelte';

export const BACKDROPS = [
  { value: 'wall', label: 'Wall' },
  { value: 'flowers', label: 'Flowers' },
  { value: 'leaf', label: 'Leaf' },
  { value: 'pastel', label: 'Pastel' },
  { value: 'records', label: 'Records' },
  { value: 'checks', label: 'Checks' },
  { value: 'mosaic', label: 'Mosaic' },
  { value: 'planks', label: 'Planks' },
  { value: 'amplifier', label: 'Amplifier' },
  { value: 'agave', label: 'Agave' },
] as const;
/** What stands there until another is chosen (the owner, 2026-10-26: no grey canvas any more). */
export const DEFAULT_BACKDROP = 'wall';

/** A photo of Cratarr's, whole against the page (a relative url() would resolve against a stylesheet's place). */
export const backdropPhoto = (name: string) => new URL(`backdrops/${name}.jpg`, document.baseURI).href;

/** The backdrop's picture now: one's own (until there is one, the default photo) or one of Cratarr's. */
export function backdropUrl(): string | null {
  const b = prefs.flowBackdrop;
  if (b === 'own') return localCovers[HOME_HEADER] ?? backdropPhoto(DEFAULT_BACKDROP);
  return backdropPhoto(b);
}
