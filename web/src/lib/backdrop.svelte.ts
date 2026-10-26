// What stands behind every flow (the owner, 2026-10-26): the grey canvas, one of Cratarr's photos (public/backdrops,
// from Unsplash: CREDITS.txt there), or one's own picture (lib/localCovers, kept on this device). App puts it on the
// page (--flow-photo); Home's sky takes it too where asked (Also on Home).
import { HOME_HEADER, localCovers } from './localCovers.svelte';
import { prefs } from './prefs.svelte';

export const BACKDROPS = [
  { value: 'wall', label: 'Wall' },
  { value: 'lamp', label: 'Lamp' },
  { value: 'flowers', label: 'Flowers' },
  { value: 'leaf', label: 'Leaf' },
  { value: 'yellow', label: 'Yellow' },
  { value: 'amplifier', label: 'Amplifier' },
  { value: 'records', label: 'Records' },
  { value: 'checks', label: 'Checks' },
  { value: 'clock', label: 'Clock' },
  { value: 'planks', label: 'Planks' },
] as const;

/** A photo of Cratarr's, whole against the page (a relative url() would resolve against a stylesheet's place). */
export const backdropPhoto = (name: string) => new URL(`backdrops/${name}.jpg`, document.baseURI).href;

/** The backdrop's picture now; null for the grey canvas (or one's own, not chosen yet). */
export function backdropUrl(): string | null {
  const b = prefs.flowBackdrop;
  if (b === 'grey') return null;
  if (b === 'own') return localCovers[HOME_HEADER] ?? null;
  return backdropPhoto(b);
}
