// Quick crates (0.11, lib/quickCrate): whether this server makes them (its capabilities name `quick_crates`,
// lib/capabilities; one before 0.11 has no list: none then), and a crate grabbed from elsewhere. One place shows and
// builds a crate (0.13; the owner, 2026-10-31: "one place where to show contents or building of a crate, and that is in
// the panel"): a grabbed one opens there, in the builder (lib/CrateEditor), its sentence as read.
import { can } from './capabilities.svelte';
import { startCrate } from './crates.svelte';
import { CLOSE, conditionsOf, nameOf, namesOf, type Chip, type Modes } from './quickCrate';

/** The server makes them: everything about them shows only then (a server with no list, before 0.11, makes none). */
export const quickOn = () => can('quick_crates') === true;

/** The panel's builder on a crate already read (0.12): a menu's "Sounds like this" or "Grab a crate", a search's crate
 *  card. Its sentence as it was read; named as the sentence names it, so it saves as it is. */
export function grabCrate(chips: Chip[], modes: Modes = {}) {
  const sentence = { chips: chips.map((c) => ({ ...c })), min: CLOSE, modes };
  startCrate(nameOf(chips), conditionsOf(chips, CLOSE, modes), { names: namesOf(chips), sentence });
}

/** The panel's builder on words still to read (a menu's Grab a crate of a name not known): read as typed ones are. */
export function openQuickCrate(words = '') {
  startCrate('', [], { words });
}
