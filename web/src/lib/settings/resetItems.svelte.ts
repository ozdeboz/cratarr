// What each part of Settings resets, declared once (the owner, 2026-11-02: "the section reset and the panel reset
// disagree with what is default"): its preferences, each in its section. The part's Reset in the top right glass puts
// back all of a part's (lib/settings/reset offerReset), a section's Reset only its section's (lib/settings/OrderReset),
// the sheets that show a section outside Settings too (the player's extra details, its bands, the tape), so no reset
// keeps an idea of a default of its own.
import { appInfo, inApp, setDevice } from '../app.svelte';
import { BAND_LOOKS } from '../bandWaves.svelte';
import { setStartStopNow, startStopNow } from '../device.svelte';
import { type BandLookName, type BandTune, MOTION, prefs, startsOf, TAPE, type Motion } from '../prefs.svelte';
import { pref, type ResetItem } from './reset.svelte';

/** Content: Home's sections, and the music profiles. */
export const HOME = 'Home';
export const PROFILES = 'Music profiles';
export const contentItems = (): ResetItem[] => [
  pref('homeParts', "Home's sections", undefined, HOME),
  pref('homeCrates', 'Your crates on Home', undefined, HOME),
  pref('profile', 'Music profiles', undefined, PROFILES),
  pref('profileOpen', 'Music profiles open', undefined, PROFILES),
  pref('profileFoldAt', 'Music profiles folded', undefined, PROFILES),
  pref('profileRows', "Music profiles' rows", undefined, PROFILES),
];

/** Player: its extra details, its gestures, and each look of its bands (lib/bandTune: what the user changed kept apart
 *  from the look's own defaults, so a look as it came is no change at all). */
export const DETAILS = 'Details';
export const GESTURES = 'Gestures';
export const VISUALISATION = 'Visualisation';
const LOOKS = BAND_LOOKS.filter((l): l is { value: BandLookName; label: string } => l.value !== 'off');
export const lookName = (look: BandLookName) => LOOKS.find((l) => l.value === look)?.label ?? look;
export const bandItems = (): ResetItem[] =>
  LOOKS.map((l) => ({
    name: l.label,
    now: () => prefs.playerBandsTunes[l.value] ?? {},
    def: {} as Partial<BandTune>,
    set: (v: Partial<BandTune>) => {
      const { [l.value]: _, ...rest } = prefs.playerBandsTunes;
      prefs.playerBandsTunes = Object.keys(v).length ? { ...rest, [l.value]: v } : rest;
    },
    section: VISUALISATION,
  }));
export const playerItems = (): ResetItem[] => [
  pref('playerRow', "The player's details row", undefined, DETAILS),
  pref('playerFacts', 'Details shown', undefined, DETAILS),
  pref('playerSwipes', 'Gestures', undefined, GESTURES),
  ...bandItems(),
];

/** This device: Stay ready (the app's), and the tape: its mechanics and each motion (lib/device: its owner's copy). */
export const TAPE_SECTION = 'Tape';
export const TAPE_NAME = 'The tape';
/** The six motions, in pairs, back then forward (the owner, 2026-10-28): to the start and the end, the seeks, the
 *  holds (lib/settings/TapeItems). */
export const MOTIONS: { m: Motion; name: string; line: string }[] = [
  { m: 'toStart', name: 'Back to start', line: 'Previous' },
  { m: 'toEnd', name: 'Forward to end', line: 'Next' },
  { m: 'seekBack', name: 'Seek back', line: 'The waveform, a swipe on the cover' },
  { m: 'seekFwd', name: 'Seek forward', line: 'The waveform, a swipe on the cover' },
  { m: 'holdBack', name: 'Holding back', line: 'Previous, held' },
  { m: 'holdFwd', name: 'Holding forward', line: 'Next, held' },
];
export const tapeItems = (): ResetItem[] => [
  {
    name: TAPE_NAME,
    now: () => startStopNow().tape,
    def: startsOf(TAPE),
    set: (v: ReturnType<typeof startsOf>) => setStartStopNow({ tape: { ...startStopNow().tape, ...v } }),
    say: (v: Record<string, number>) => (JSON.stringify(v) === JSON.stringify(startsOf(TAPE)) ? 'As it came' : 'As you set it'),
    section: TAPE_SECTION,
  },
  ...MOTIONS.map(({ m, name }) => ({
    name,
    now: () => startStopNow().motions[m],
    def: MOTION[m].start as number,
    set: (v: number) => setStartStopNow({ motions: { ...startStopNow().motions, [m]: v } }),
    section: TAPE_SECTION,
  })),
];
export const deviceItems = (): ResetItem[] => [
  ...(inApp ? [{ name: 'Stay ready', now: () => !!appInfo.stayReady, def: false, set: (v: boolean) => setDevice({ stayReady: v }) }] : []),
  ...tapeItems(),
];
