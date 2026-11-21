// RS01, the one record sheet, as the page lays it out (SCENARIOS-DC1 RS1–RS8, RS14, RS23; DOMAIN-CORE §8): what the record's
// entity says, drawn as sent. The server decides every section, action and link, its words, icon, group and order (`decide()`,
// the when-table, the words table); the page groups them on the menu's cards and decides nothing of a record. No runes, so the
// tests take it as it is; lib/sheet/Record.svelte draws it.
import type { ActionView } from '../generated/ActionView';
import type { EntryGroup } from '../generated/EntryGroup';
import type { LinkView } from '../generated/LinkView';
import type { Section } from '../generated/Section';
import type { RecordEntity } from '../recordCache';
import { recordWord } from '../recordTypeRules';
import { when } from '../settings/when';

/** The places a record opens from (the when-table's `from`, shared/when.json; `label` the server reads as `missing`'s row). */
export const PLACES = ['library', 'artist', 'search', 'work', 'missing', 'label', 'discover', 'crate', 'playlist', 'player', 'downloads', 'incoming', 'problem', 'several', 'device', 'page', 'requests', 'soulseek'] as const;
export type PlaceWord = (typeof PLACES)[number];

/** The menu's groups in their order (decision 0058; the server's EntryGroup, as the page's GROUP_ORDER has them). */
const GROUPS: readonly EntryGroup[] = ['play', 'want', 'go', 'discover', 'feel', 'collect', 'take', 'know', 'change', 'library', 'remove'];

/** One entry of a section: a server's action, or a link (asking the server nothing), or the device's own Remove. */
export type Item = { name: string; title: string; icon: string; group: EntryGroup; action?: ActionView; link?: LinkView };
export type Card = { group: EntryGroup; items: Item[] };

const ofAction = (a: ActionView): Item => ({ name: a.name, title: a.title, icon: a.icon, group: a.group, action: a });
const ofLink = (l: LinkView): Item => ({ name: l.name, title: l.title, icon: l.icon, group: l.group, link: l });

/** The record's own page's opener (RS1: the `page` place counts its opener's libraries), derived from the panel's stack (the
 *  coordinator, 2026-11-21): what the panel views below the record names it first (an artist, a crate, a label, a playlist), else
 *  the page it shows over; none where neither names a place, so the server's Library fallback stays (Home's shelves are no
 *  crate). The prefixes are the panel's subjects' own (panel.svelte.ts, crates.svelte.ts, labelPage.ts). */
export function openerOf(below: string | null, page: string, searching: boolean): PlaceWord | undefined {
  if (below?.startsWith('artist:') || below?.startsWith('artist-tracks:')) return 'artist';
  if (below?.startsWith('crate:')) return 'crate';
  if (below?.startsWith('label:')) return 'label';
  if (below?.startsWith('server-playlist:') || (below && /^pl-/.test(below))) return 'playlist';
  if (searching) return 'search';
  const byPage: Partial<Record<string, PlaceWord>> = { library: 'library', discover: 'discover', crates: 'crate', playlists: 'playlist', health: 'problem' };
  return byPage[page];
}

/** The yes an action asks at a step (RS25, decision 0034): the server's words for it, in order, each asked before anything is
 *  sent (a main library's Delete asks twice); none left, it is sent. The page works out no step of its own. */
export const askAt = (a: Pick<ActionView, 'confirm'>, step: number): string | null => a.confirm?.[step] ?? null;

/** The Play row (RS3, CN4): the play section's actions in the order sent, Want last, or first from Discover. */
export const playRow = (e: RecordEntity): ActionView[] => e.actions.filter((a) => a.section === 'play');

/** Each section after the Head and the Play row, in the entity's order (RS1): its actions and links on cards by group, in the
 *  groups' order, each card's entries as sent (RS7, RS23). The Versions section's lines are `versionLines`. */
export function sectionsOf(e: RecordEntity): { section: Section; cards: Card[] }[] {
  return e.sections
    .filter((s) => s !== 'head' && s !== 'play')
    .map((section) => {
      const items = [...e.actions.filter((a) => a.section === section).map(ofAction), ...e.links.filter((l) => l.section === section).map(ofLink)];
      const cards = GROUPS.map((group) => ({ group, items: items.filter((i) => i.group === group) })).filter((c) => c.items.length);
      return { section, cards };
    });
}

/** A line of the Versions section (RS4): its words, whether it is the one that plays, and its tap: the server's Play from of
 *  that copy, or an edition's record to open; none on the line that plays. */
export type Line = { key: string; text: string; plays: boolean; play: ActionView | null; open: string | null };

/** The Versions section (RS4, RS5): a line per copy and edition, in the order sent (the order they would play). */
export function versionLines(e: RecordEntity): Line[] {
  return e.entities.map((v) => {
    const tracks = v.tracks ? `${v.tracks} ${v.tracks === 1 ? 'track' : 'tracks'}` : null;
    const text = [tracks, v.library, v.quality, v.plays ? 'plays' : null].filter(Boolean).join(' · ');
    const play = v.kind === 'copy' && v.action === 'play_from' ? (e.actions.find((a) => a.name === 'play_from' && a.fields?.from === v.id) ?? null) : null;
    const open = v.kind === 'edition' && v.action === 'view' ? `album:cratarr/${v.record}` : null;
    return { key: `${v.kind}:${v.id}`, text, plays: v.plays, play, open };
  });
}

/** The Head's state line (RS2): its words as sent, its time said by the page's one when() (RV8). */
export const stateLine = (e: RecordEntity, say: (s: number) => string = when) => (e.state.at ? `${e.state.words} ${say(e.state.at)}` : e.state.words);

/** The record's type with what it is by (VR39), as the References sheet says it: "EP · by its sources", the server's words; none
 *  while the type has no value (none until a hand picks). */
export function typeLine(p: RecordEntity['properties']): string | null {
  if (!p.type.value) return null;
  const word = recordWord(p.type.value);
  return [word.charAt(0).toUpperCase() + word.slice(1), p.type_why].filter(Boolean).join(' · ');
}

/** The Head's chips (RS2): its artists by the one formatter (`credit`), its type's word, its year. */
export function headChips(e: RecordEntity): string[] {
  const word = recordWord(e.properties.type.value);
  const year = e.properties.year.value;
  return [e.properties.credit, word.charAt(0).toUpperCase() + word.slice(1), year ? String(year) : ''].filter(Boolean);
}

/** On this device (RS8): in the app, of what the server sent, Remove from this device where the device keeps the record, Sync to
 *  this device and Download… where it does not (what the device holds is its own state, never a right); none in a browser, and
 *  none where the server sent none. */
export function deviceItems(e: RecordEntity, d: { inApp: boolean; keeps: boolean }): Item[] {
  const sent = e.actions.filter((a) => a.section === 'device').map(ofAction);
  if (!d.inApp || !sent.length) return [];
  if (d.keeps) return [{ name: 'remove_from_device', title: '', icon: 'trash', group: 'take' }];
  return [...sent.filter((i) => i.name === 'sync'), ...sent.filter((i) => i.name !== 'sync')];
}

/** One run of each action at a time (CN11, RS21): a second tap while the first is under way does nothing. */
export class Runs {
  readonly #on = new Set<string>();
  /** Runs `run` unless one of `key` is under way; whether it ran. */
  async once(key: string, run: () => Promise<void>): Promise<boolean> {
    if (this.#on.has(key)) return false;
    this.#on.add(key);
    try {
      await run();
      return true;
    } finally {
      this.#on.delete(key);
    }
  }
}
