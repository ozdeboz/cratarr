// What stands behind every flow (the owner, 2026-10-26): one of Cratarr's photos (public/backdrops,
// from Unsplash: CREDITS.txt there), or one's own picture (lib/localCovers, kept on this device). App puts it on the
// page (--flow-photo); Home's sky takes it too where asked (Also on Home).
import { HOME_HEADER, localCovers } from './localCovers.svelte';
import { prefs, type FlowBackdrop } from './prefs.svelte';

export const BACKDROPS = [
  { value: 'wall', label: 'Wall' },
  { value: 'flowers', label: 'Flowers' },
  { value: 'leaf', label: 'Leaf' },
  { value: 'pastel', label: 'Pastel' },
  { value: 'records', label: 'Records' },
  // The owner, 2026-10-28: Grill (its value stays, so who chose it keeps it).
  { value: 'checks', label: 'Grill' },
  { value: 'mosaic', label: 'Mosaic' },
  { value: 'planks', label: 'Planks' },
  { value: 'amplifier', label: 'Amplifier' },
  { value: 'agave', label: 'Agave' },
  { value: 'rack', label: 'Rack' },
  { value: 'speakers', label: 'Speakers' },
  { value: 'radios', label: 'Radios' },
  { value: 'stickers', label: 'Stickers' },
  { value: 'textile', label: 'Textile' },
  { value: 'succulents', label: 'Succulents' },
] as const;
/** What stands there until another is chosen (the owner, 2026-10-26: no grey canvas any more). */
export const DEFAULT_BACKDROP = 'wall';

/** A photo of Cratarr's, whole against the page (a relative url() would resolve against a stylesheet's place). */
export const backdropPhoto = (name: string) => new URL(`backdrops/${name}.jpg`, document.baseURI).href;

/** Shuffle's timer, when every few minutes is asked. */
export const SHUFFLE_EVERY_MS = 5 * 60_000;

/** Shuffle's turn (the owner, 2026-10-27): the ticked one standing there now; null till the first is drawn. */
const shuffled = $state<{ now: FlowBackdrop | null; turns: number }>({ now: null, turns: 0 });

/** What Shuffle may take: the ticked ones there are (one's own only while there is one). */
const shuffleable = () => prefs.flowShuffled.filter((b) => b !== 'own' || localCovers[HOME_HEADER]);

/** Shuffle's next: another of the ticked ones than the one there now (lib/FlowStage and the other flows ask on
 *  opening, App at a start and on its timer, each as Settings › Appearance says). */
export function nextBackdrop() {
  if (!prefs.flowShuffle) return;
  const all = shuffleable();
  const others = all.length > 1 ? all.filter((b) => b !== shuffled.now) : all;
  shuffled.now = others.length ? others[Math.floor(Math.random() * others.length)] : null;
  shuffled.turns++;
}

/** How many turns Shuffle has taken: App's timer starts over at each (Next in Appearance too). */
export const shuffleTurns = () => shuffled.turns;

// A start draws the first.
nextBackdrop();

/** A flow opening, `seed` naming it (a crate's id, the Library's albums): Shuffle's next when each flow is asked; its
 *  own when each flow keeps its own (the owner, 2026-10-27: the same flow, the same photo, while the ticks stay). */
export function flowOpened(seed: string) {
  if (!prefs.flowShuffle) return;
  if (prefs.flowShuffleWhen === 'flow') nextBackdrop();
  else if (prefs.flowShuffleWhen === 'seeded') {
    const all = shuffleable();
    let h = 0;
    for (const c of seed) h = (Math.imul(h, 31) + c.charCodeAt(0)) | 0;
    if (all.length) shuffled.now = all[Math.abs(h) % all.length];
  }
}

/** The ticked one standing there now (Shuffle on), for the picker to mark. */
export const shuffleNow = () => (prefs.flowShuffle && !shuffleStale() ? shuffled.now : null);

/** Shuffle's turn no longer one it may take (Shuffle just on, that one unticked, one's own picture gone): App draws
 *  another. */
export const shuffleStale = () => prefs.flowShuffle && (!shuffled.now || !shuffleable().includes(shuffled.now));

/** The backdrop's picture now: one's own (until there is one, the default photo) or one of Cratarr's; with Shuffle on,
 *  the ticked one whose turn it is. */
export function backdropUrl(): string | null {
  let b = prefs.flowBackdrop;
  if (prefs.flowShuffle && !shuffleStale()) b = shuffled.now ?? b;
  if (b === 'own') return localCovers[HOME_HEADER] ?? backdropPhoto(DEFAULT_BACKDROP);
  return backdropPhoto(b);
}
