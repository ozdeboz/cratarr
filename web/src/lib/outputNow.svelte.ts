// Where the music goes now: the output chosen, except that with Cratarr out of reach one of its players cannot be reached
// either, so this device plays for the moment (the owner, 2026-11-08: "Play here instead"; 0.16.0 pressed play on the tablet
// for the ThinkPad, and nothing played). The choice stays as it was, and is used again when Cratarr answers. Its own module,
// read by the player and Play On alike, which read each other.
import { prefs } from './prefs.svelte';
import { reach } from './reach.svelte';

export const outputNow = (): typeof prefs.output => (reach.away && prefs.output.startsWith('player:') ? 'local' : prefs.output);
